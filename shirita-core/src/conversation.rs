//! 对话服务：发送消息并流式返回助手回复，结束时落库。

use std::sync::Arc;

use futures::{Stream, StreamExt};

use crate::model::{ChatMessage, ChatRequest, ModelProvider};
use crate::models::message::{Message, Role};
use crate::storage::Storage;
use crate::tokenizer::TokenCounter;

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

        // 2) 载入会话以取挂载/覆盖/状态，并组装 system。
        let session = match storage.get_session(&session_id).await {
            Ok(Some(s)) => s,
            Ok(None) => { yield SendEvent::Error("session not found".into()); return; }
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let mut mounted = Vec::new();
        for id in &session.mounted_definitions {
            match storage.get_definition(id).await {
                Ok(Some(d)) => mounted.push(d),
                Ok(None) => {}
                Err(e) => { yield SendEvent::Error(e.to_string()); return; }
            }
        }
        let local = session
            .override_config
            .get("local_definitions")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let system = crate::assembly::assemble_system_prompt(&mounted, &local, &session.current_state);
        let regex_rules: Vec<_> = mounted
            .iter()
            .filter(|d| d.def_type == crate::models::definition::DefinitionType::RegexRule)
            .cloned()
            .collect();

        // 组装请求消息：system（若非空） + 历史（过滤隐藏） + 新 user。
        let mut chat_messages: Vec<ChatMessage> = Vec::new();
        if !system.is_empty() {
            chat_messages.push(ChatMessage { role: Role::System, content: system });
        }
        chat_messages.extend(history.iter().filter(|m| !m.is_hidden).map(|m| ChatMessage {
            role: m.role,
            content: m.raw_content.clone(),
        }));
        chat_messages.push(ChatMessage { role: Role::User, content: user_text.clone() });

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
        let storage = Arc::new(temp_storage().await);
        let mut session = Session::new("t");
        let ch = crate::models::definition::Definition::new(
            crate::models::definition::DefinitionType::Char,
            "C",
            "I am {{who}}",
        );
        storage.create_definition(&ch).await.unwrap();
        session.mounted_definitions = vec![ch.id.clone()];
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
        assert!(req.messages[0].content.contains("<characters>"));
        assert!(req.messages[0].content.contains("I am Neo"));
    }

    #[tokio::test]
    async fn regex_rule_sets_display_content() {
        let storage = Arc::new(temp_storage().await);
        let mut session = Session::new("t");
        let mut rule = crate::models::definition::Definition::new(
            crate::models::definition::DefinitionType::RegexRule,
            "R",
            "",
        );
        rule.meta = serde_json::json!({ "pattern": "STOP", "replacement": "" });
        storage.create_definition(&rule).await.unwrap();
        session.mounted_definitions = vec![rule.id.clone()];
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
}
