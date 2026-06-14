//! 对话服务：发送消息并流式返回助手回复，结束时落库。

use std::sync::Arc;

use futures::{Stream, StreamExt};

use crate::model::{ChatMessage, ChatRequest, ModelProvider};
use crate::models::message::{Message, Role};
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::models::session::Session;
use crate::storage::Storage;
use crate::tokenizer::TokenCounter;

/// 会话有效节点树：自有节点优先（fork 后），否则引用模板。
pub async fn effective_nodes(
    storage: &dyn Storage,
    session: &Session,
) -> crate::Result<Vec<PromptNode>> {
    let own = storage.list_nodes(&OwnerKind::Session, &session.id).await?;
    if !own.is_empty() {
        return Ok(own);
    }
    if let Some(tid) = &session.template_id {
        return storage.list_nodes(&OwnerKind::Template, tid).await;
    }
    Ok(Vec::new())
}

/// 流式发送过程对外暴露的事件。
#[derive(Debug, Clone, PartialEq)]
pub enum SendEvent {
    /// 一段文本增量。
    Delta(String),
    /// 完成，附助手消息 id。
    Done { message_id: String },
    /// 出错（流随后结束）。
    Error(String),
}

/// 发送一条 user 消息：落库 user → 组装历史 → 调用 provider 流式 → 累积 → 落库 assistant。
/// 返回一个事件流；assistant 消息在收到完整回复后写入存储，然后才 yield `Done`。
pub fn send_message(
    storage: Arc<dyn Storage>,
    provider: Arc<dyn ModelProvider>,
    counter: Arc<dyn TokenCounter>,
    model: String,
    session_id: String,
    user_text: String,
) -> impl Stream<Item = SendEvent> {
    async_stream::stream! {
        // 0) 校验会话存在（在任何写入之前，避免依赖 FK 约束兜底）。
        let session = match storage.get_session(&session_id).await {
            Ok(Some(s)) => s,
            Ok(None) => { yield SendEvent::Error("session not found".into()); return; }
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };

        // 1) 落库 user 消息（parent = 当前末条消息）。
        let history = match storage.list_messages(&session_id).await {
            Ok(h) => h,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let last_id = history.last().map(|m| m.id.clone());
        let user_msg = Message::new(&session_id, last_id, Role::User, &user_text);
        if let Err(e) = storage.create_message(&user_msg).await {
            yield SendEvent::Error(e.to_string());
            return;
        }

        // 2) 取会话有效树 + 定义 + 局部覆盖 + 最近消息，按树组装结构化段。
        let nodes = match effective_nodes(storage.as_ref(), &session).await {
            Ok(n) => n,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let mut defs = std::collections::HashMap::new();
        for n in &nodes {
            if let Some(did) = &n.definition_id {
                if !defs.contains_key(did) {
                    if let Ok(Some(d)) = storage.get_definition(did).await {
                        defs.insert(did.clone(), d);
                    }
                }
            }
        }
        let local = session
            .override_config
            .get("local_definitions")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        // 扫描窗口：最近 N 条（含将发送的 user）。
        let scan_depth = 4usize;
        let mut recent: Vec<String> = history
            .iter()
            .rev()
            .take(scan_depth.saturating_sub(1))
            .map(|m| m.raw_content.clone())
            .collect();
        recent.reverse();
        recent.push(user_text.clone());

        // StdRng（非 ThreadRng）：Send，可安全跨越后续 await（SSE 流要求 Send）。
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_entropy();
        let plan = crate::assembly::assemble_from_nodes(
            &nodes,
            &defs,
            &local,
            &session.current_state,
            &recent,
            true,
            scan_depth,
            &mut || rand::Rng::gen::<f64>(&mut rng),
        );

        // 真实历史（过滤隐藏）+ 本次 user，交给 build_chat_messages 按 history 节点拼装。
        let mut hist_msgs: Vec<ChatMessage> = history
            .iter()
            .filter(|m| !m.is_hidden)
            .map(|m| ChatMessage { role: m.role, content: m.raw_content.clone() })
            .collect();
        hist_msgs.push(ChatMessage { role: Role::User, content: user_text.clone() });

        // 有显式 history 节点时按其启用状态；没有节点（如无模板的自由会话）默认编入历史。
        let has_history_node = nodes.iter().any(|n| n.kind == NodeKind::History);
        let include_history = plan.history_enabled || !has_history_node;
        let chat_messages = crate::assembly::build_chat_messages(&plan, &hist_msgs, include_history);

        // regex 规则：所有 regex_rule 定义（Settings 拥有，全局生效）。
        let regex_rules: Vec<_> = match storage.list_definitions().await {
            Ok(all) => all
                .into_iter()
                .filter(|d| d.def_type == "regex_rule")
                .collect(),
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };

        let prompt_text: String =
            chat_messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n");
        tracing::debug!(prompt_tokens = counter.count(&prompt_text), "assembled prompt");

        let req = ChatRequest { model, messages: chat_messages };

        // 3) 调 provider 流，逐 delta 累积 + yield。
        let mut full = String::new();
        let mut stream = match provider.stream_chat(req).await {
            Ok(s) => s,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        while let Some(item) = stream.next().await {
            match item {
                Ok(delta) => { full.push_str(&delta); yield SendEvent::Delta(delta); }
                Err(e) => { yield SendEvent::Error(e.to_string()); return; }
            }
        }

        // 4) 落库 assistant 消息（含 regex 清洗后的 display_content），再 yield Done。
        let mut assistant = Message::new(&session_id, Some(user_msg.id.clone()), Role::Assistant, &full);
        assistant.display_content = crate::assembly::apply_regex_rules(&full, &regex_rules);
        if let Err(e) = storage.create_message(&assistant).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
        yield SendEvent::Done { message_id: assistant.id };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EchoProvider;
    use crate::models::session::Session;
    use crate::storage::sqlite::SqliteStorage;
    use crate::tokenizer::tiktoken::TiktokenCounter;

    async fn temp_storage() -> SqliteStorage {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conv.db");
        std::mem::forget(dir);
        let s = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
        s.run_migrations().await.unwrap();
        s
    }

    #[tokio::test]
    async fn echo_send_streams_and_persists() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("t");
        storage.create_session(&session).await.unwrap();

        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        let stream = send_message(
            storage_dyn,
            provider,
            counter,
            "test-model".into(),
            session.id.clone(),
            "hello".into(),
        );
        futures::pin_mut!(stream);

        let mut deltas = String::new();
        let mut done_id = None;
        while let Some(ev) = stream.next().await {
            match ev {
                SendEvent::Delta(d) => deltas.push_str(&d),
                SendEvent::Done { message_id } => done_id = Some(message_id),
                SendEvent::Error(e) => panic!("unexpected error: {e}"),
            }
        }
        assert_eq!(deltas, "echo: hello");
        assert!(done_id.is_some());

        // 持久化校验：user + assistant 各一条，内容正确。
        let msgs = storage.list_messages(&session.id).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].raw_content, "hello");
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[1].raw_content, "echo: hello");
        assert_eq!(msgs[1].parent_id.as_deref(), Some(msgs[0].id.as_str()));
    }

    use crate::model::{ChatRequest, ModelProvider};
    use futures::stream::{self, BoxStream};
    use std::sync::Mutex;

    struct RecordingProvider {
        seen: Arc<Mutex<Option<ChatRequest>>>,
        reply: String,
    }
    #[async_trait::async_trait]
    impl ModelProvider for RecordingProvider {
        async fn stream_chat(
            &self,
            req: ChatRequest,
        ) -> crate::Result<BoxStream<'static, crate::Result<String>>> {
            *self.seen.lock().unwrap() = Some(req);
            let reply = self.reply.clone();
            Ok(Box::pin(stream::iter(vec![Ok(reply)])))
        }
    }

    #[tokio::test]
    async fn assembled_system_is_sent() {
        use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
        use crate::models::template::Template;
        let storage = Arc::new(temp_storage().await);
        let ch = crate::models::definition::Definition::new("char", "C", "I am {{who}}");
        storage.create_definition(&ch).await.unwrap();

        // 模板树：char 容器 → ref(char)，再加 history 魔法节点。
        let t = Template::new("T");
        storage.create_template(&t).await.unwrap();
        let f = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "char");
        storage.create_node(&f).await.unwrap();
        let r = PromptNode::new_ref(OwnerKind::Template, &t.id, Some(f.id.clone()), 0, &ch.id);
        storage.create_node(&r).await.unwrap();
        let mut hist = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 1, "history");
        hist.kind = NodeKind::History;
        hist.tag = None;
        storage.create_node(&hist).await.unwrap();

        let mut session = Session::new("t");
        session.template_id = Some(t.id.clone());
        session.current_state = serde_json::json!({ "who": "Neo" });
        storage.create_session(&session).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: seen.clone(),
            reply: "ok".into(),
        });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        let stream = send_message(
            storage_dyn,
            provider,
            counter,
            "m".into(),
            session.id.clone(),
            "hi".into(),
        );
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        assert_eq!(req.messages[0].role, Role::System);
        assert!(req.messages[0].content.contains("<char>"));
        assert!(req.messages[0].content.contains("I am Neo"));
        // history 节点之后，本次 user 转发给 provider。
        assert!(req.messages.iter().any(|m| m.role == Role::User && m.content == "hi"));
    }

    #[tokio::test]
    async fn regex_rule_sets_display_content() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("t");
        // regex 规则现在是全局的（Settings 拥有），无需挂载即生效。
        let mut rule = crate::models::definition::Definition::new("regex_rule", "R", "");
        rule.meta = serde_json::json!({ "pattern": "STOP", "replacement": "" });
        storage.create_definition(&rule).await.unwrap();
        storage.create_session(&session).await.unwrap();

        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: Arc::new(Mutex::new(None)),
            reply: "helloSTOP".into(),
        });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        let stream = send_message(
            storage_dyn,
            provider,
            counter,
            "m".into(),
            session.id.clone(),
            "hi".into(),
        );
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let assistant = msgs.iter().find(|m| m.role == Role::Assistant).unwrap();
        assert_eq!(assistant.raw_content, "helloSTOP");
        assert_eq!(assistant.display_content.as_deref(), Some("hello"));
    }

    #[tokio::test]
    async fn send_to_unknown_session_errors_cleanly() {
        let storage = Arc::new(temp_storage().await);
        let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        let stream = send_message(
            storage_dyn,
            provider,
            counter,
            "m".into(),
            "ghost-session".into(),
            "hi".into(),
        );
        futures::pin_mut!(stream);

        match stream.next().await.unwrap() {
            SendEvent::Error(msg) => assert!(msg.contains("session not found"), "got: {msg}"),
            other => panic!("expected clean Error, got {other:?}"),
        }
        assert!(stream.next().await.is_none(), "no events after error");
        // 关键：未创建任何消息。
        assert!(storage
            .list_messages("ghost-session")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn effective_nodes_prefers_session_else_template() {
        use crate::models::prompt_node::{OwnerKind, PromptNode};
        use crate::models::template::Template;
        let storage = temp_storage().await;
        // template with one folder node
        let t = Template::new("T");
        storage.create_template(&t).await.unwrap();
        let f = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "char");
        storage.create_node(&f).await.unwrap();

        // session references template, has no own nodes
        let mut sess = Session::new("s");
        sess.template_id = Some(t.id.clone());
        storage.create_session(&sess).await.unwrap();

        let nodes = super::effective_nodes(&storage, &sess).await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].tag.as_deref(), Some("char"));
    }
}
