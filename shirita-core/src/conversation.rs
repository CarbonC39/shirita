//! 对话服务：发送消息并流式返回助手回复，结束时落库。

use std::sync::Arc;

use futures::{Stream, StreamExt};

use crate::attachments::resolve_images;
use crate::model::{ChatMessage, ChatRequest, ModelProvider};
use crate::models::definition::Definition;
use crate::models::message::{Message, Role};
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::budget::trim_history;
use crate::models::session::Session;
use crate::models::summary::Summary;
use crate::state::{
    apply_updates, effective_state, parse_state_updates, resolve_schema, strip_state_tags, VarDecl,
};
use crate::storage::Storage;
use crate::tokenizer::TokenCounter;

/// 解析会话的有效变量 schema（系统 ∪ 模板 meta ∪ 会话 local）。
async fn session_schema(storage: &dyn Storage, session: &Session) -> Vec<VarDecl> {
    let template_meta = match &session.template_id {
        Some(tid) => storage.get_template(tid).await.ok().flatten().map(|t| t.meta),
        None => None,
    };
    resolve_schema(template_meta.as_ref(), &session.override_config)
}

/// 读上下文窗口（settings `context.window`，默认 200000）。
async fn context_window(storage: &dyn Storage) -> usize {
    storage
        .get_setting("context.window")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(200_000)
}

/// 读回复（输出）最大 token 数（settings `provider_max_tokens`）。`None` 时由各 provider 兜底
/// （Anthropic 8192 / OpenAI 省略）。注意：这是输出上限，与上下文窗口（`context.window`）无关。
async fn provider_max_tokens(storage: &dyn Storage) -> Option<u32> {
    storage
        .get_setting("provider_max_tokens")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
}

/// 选当前分支适用摘要：cutoff 必须落在 active path 上，多条取 path 中最靠后的那条。
/// 返回（摘要内容, path 中 cutoff 的下标）。
async fn applicable_summary(
    storage: &dyn Storage,
    session_id: &str,
    path: &[&Message],
) -> Option<(String, usize)> {
    let summaries: Vec<Summary> = storage.list_summaries(session_id).await.ok()?;
    let pos = |mid: &str| path.iter().position(|m| m.id == mid);
    summaries
        .into_iter()
        .filter_map(|s| pos(&s.cutoff_message_id).map(|i| (s.content, i)))
        .max_by_key(|(_, i)| *i)
}

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

/// 本会话生效的 regex 规则：全局 orphan 规则（不被任何节点引用，处处生效）+ 本会话
/// effective 树里被启用 ref 引用的 scoped 规则。两集合互斥；global 在前。
pub async fn effective_regex_rules(
    storage: &dyn Storage,
    session: &Session,
) -> crate::Result<Vec<Definition>> {
    let referenced: std::collections::HashSet<String> =
        storage.referenced_definition_ids().await?.into_iter().collect();
    let all = storage.list_definitions().await?;
    let mut rules: Vec<Definition> = all
        .iter()
        .filter(|d| d.def_type == "regex_rule" && !referenced.contains(&d.id))
        .cloned()
        .collect();
    let by_id: std::collections::HashMap<&str, &Definition> =
        all.iter().map(|d| (d.id.as_str(), d)).collect();
    for n in effective_nodes(storage, session).await? {
        if n.kind == crate::models::prompt_node::NodeKind::Ref && n.enabled {
            if let Some(d) = n.definition_id.as_deref().and_then(|id| by_id.get(id)) {
                if d.def_type == "regex_rule" {
                    rules.push((*d).clone());
                }
            }
        }
    }
    Ok(rules)
}

/// Build the provider request for a turn whose visible, ordered context is
/// `context` (hidden already filtered, ending with the latest user turn), plus
/// the regex rules used to clean the reply. Shared by send + regenerate.
async fn assemble_request(
    storage: &dyn Storage,
    session: &Session,
    model: String,
    context: &[ChatMessage],
    state: &serde_json::Value,
    summary: Option<String>,
) -> crate::Result<(ChatRequest, Vec<Definition>)> {
    let nodes = effective_nodes(storage, session).await?;
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

    // 扫描窗口：取最近若干条（已含最新 user）；每个世界书条目再按自己的
    // meta.scan.depth 在窗口内取末尾 N 条扫描（设置已下放到定义本身）。
    const MAX_SCAN_WINDOW: usize = 20;
    let mut recent: Vec<String> =
        context.iter().rev().take(MAX_SCAN_WINDOW).map(|m| m.content.clone()).collect();
    recent.reverse();

    // StdRng（非 ThreadRng）：Send，可安全跨越后续 await（SSE 流要求 Send）。
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_entropy();
    let mut plan = crate::assembly::assemble_from_nodes(
        &nodes,
        &defs,
        &local,
        state,
        &recent,
        &mut || rand::Rng::gen::<f64>(&mut rng),
    );

    // Auto-inject protocol instructions. Their text lives in builtin `protocol`
    // definitions (spec §4); each is injected after history when its meta.kind
    // trigger holds. state_update fires when the session declares a non-system
    // variable (and appends the live variable list); html_patch fires when the
    // conversation already holds a card. Both may coexist; the provider adapter
    // merges adjacent System segments.
    let has_card = context.iter().any(|m| {
        crate::html_patch::is_html_document(&m.content) || crate::html_patch::has_patch_blocks(&m.content)
    });
    let schema = session_schema(storage, session).await;
    let protocols = storage.list_definitions().await?;
    for pdef in protocols.iter().filter(|d| d.def_type == "protocol") {
        let kind = pdef.meta.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let content = match kind {
            "state_update" => match crate::state::variables_block(&schema, state) {
                Some(block) => format!("{}\n\n{}", pdef.content, block),
                None => continue,
            },
            "html_patch" => {
                if !has_card {
                    continue;
                }
                pdef.content.clone()
            }
            _ => continue,
        };
        plan.segments.push(crate::assembly::PromptSegment {
            placement: crate::assembly::Placement::AfterHistory,
            content,
            source: format!("protocol:{kind}"),
        });
    }

    // 有显式 history 节点时按其启用状态；没有节点（如无模板的自由会话）默认编入历史。
    let has_history_node = nodes.iter().any(|n| n.kind == NodeKind::History);
    let include_history = plan.history_enabled || !has_history_node;
    let chat_messages = crate::assembly::build_chat_messages(&plan, context, include_history);

    // Global orphan rules + this session's tree-scoped rules (see effective_regex_rules).
    let regex_rules = effective_regex_rules(storage, session).await?;

    let max_tokens = provider_max_tokens(storage).await;
    Ok((ChatRequest { model, messages: chat_messages, summary, max_tokens }, regex_rules))
}

/// 把一条已落库的 `Message` 转成发给 provider 的 `ChatMessage`，把它自己的
/// `attachments`（asset id）解析成 data URL 图片。
async fn chat_message_from(storage: &dyn Storage, assets_dir: &str, m: &Message) -> ChatMessage {
    let images = resolve_images(storage, assets_dir, &m.attachments).await;
    ChatMessage { role: m.role, content: m.raw_content.clone(), images }
}

/// The most recent rendered HTML "card" in this branch, if any — the base a new
/// reply's SEARCH/REPLACE patch edits against. Prefers the reconstructed
/// `display_content` (already a full doc for a prior patch turn), falling back
/// to `raw_content` (the originally-emitted full document).
fn latest_html_card(path: &[&Message]) -> Option<String> {
    path.iter().rev().find_map(|m| {
        if let Some(dc) = m.display_content.as_deref() {
            if crate::html_patch::is_html_document(dc) {
                return Some(dc.to_string());
            }
        }
        crate::html_patch::is_html_document(&m.raw_content).then(|| m.raw_content.clone())
    })
}

/// Compute an assistant message's `display_content` from its raw reply. An HTML
/// card patch (SEARCH/REPLACE blocks against the branch's latest card) is
/// reconstructed into the full document; otherwise the regular regex-rule /
/// state-tag-stripping path applies. `cleaned` is `full` with `<state_update>`
/// tags already stripped.
fn resolve_display(
    path: &[&Message],
    full: &str,
    cleaned: &str,
    regex_rules: &[Definition],
) -> Option<String> {
    if let Some(html) = crate::html_patch::reconstruct(latest_html_card(path).as_deref(), cleaned) {
        return Some(html);
    }
    match crate::assembly::apply_regex_rules(cleaned, regex_rules) {
        Some(s) => Some(s),
        None => (cleaned != full).then(|| cleaned.to_string()),
    }
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
    assets_dir: String,
    attachment_ids: Vec<String>,
) -> impl Stream<Item = SendEvent> {
    async_stream::stream! {
        // 0) 校验会话存在（在任何写入之前，避免依赖 FK 约束兜底）。
        let session = match storage.get_session(&session_id).await {
            Ok(Some(s)) => s,
            Ok(None) => { yield SendEvent::Error("session not found".into()); return; }
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };

        // 1) parent = 当前激活叶子（沿 active_leaf 的分支末端），落库 user 消息。
        let all = match storage.list_messages(&session_id).await {
            Ok(h) => h,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let path = crate::tree::active_path(&all, session.active_leaf_id.as_deref());
        let parent_id = path.last().map(|m| m.id.clone());

        // 分支有效状态：schema 初值 < seed < 当前叶子快照（读侧/写侧共用同一兜底）。
        let schema = session_schema(storage.as_ref(), &session).await;
        let leaf_snapshot = path.last().map(|m| m.snapshot_state.clone()).unwrap_or_else(|| serde_json::json!({}));
        let branch_state = effective_state(&schema, &session.current_state, &leaf_snapshot);

        let mut user_msg = Message::new(&session_id, parent_id, Role::User, &user_text);
        user_msg.snapshot_state = branch_state.clone();
        user_msg.attachments = attachment_ids.clone();
        if let Err(e) = storage.create_message(&user_msg).await {
            yield SendEvent::Error(e.to_string());
            return;
        }

        // 当前分支适用摘要：替换 cutoff 之前的历史，cutoff 之后照常带入。
        let summary = applicable_summary(storage.as_ref(), &session_id, &path).await;
        let visible_start = summary.as_ref().map(|(_, i)| i + 1).unwrap_or(0);
        let summary_text = summary.map(|(c, _)| c);

        // 2) 组装：context = cutoff 之后的分支可见消息（过滤隐藏）+ 本次 user。
        let mut context: Vec<ChatMessage> = Vec::new();
        for m in path[visible_start..].iter().filter(|m| !m.is_hidden) {
            context.push(chat_message_from(storage.as_ref(), &assets_dir, m).await);
        }
        let new_turn_images = resolve_images(storage.as_ref(), &assets_dir, &attachment_ids).await;
        context.push(ChatMessage { role: Role::User, content: user_text.clone(), images: new_turn_images });
        let (req, regex_rules) = match assemble_request(storage.as_ref(), &session, model, &context, &branch_state, summary_text.clone()).await {
            Ok(r) => r,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };

        // best-effort 裁剪：超窗口则丢最旧的中段历史（溢出由 provider 报错沿 Error 路径暴露）。
        let window = context_window(storage.as_ref()).await;
        let (trimmed, dropped) = trim_history(&req.messages, window, counter.as_ref());
        if dropped > 0 {
            tracing::warn!(dropped, "context over window: trimmed oldest history");
        }
        let req = ChatRequest { model: req.model, messages: trimmed, summary: req.summary, max_tokens: req.max_tokens };

        let prompt_text: String =
            req.messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n");
        tracing::debug!(prompt_tokens = counter.count(&prompt_text), "assembled prompt");

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

        // 4) 折叠 <state_update> 进快照、剥离展示文本，落库 assistant 消息，再 yield Done。
        let updates = parse_state_updates(&full);
        let new_snapshot = apply_updates(&branch_state, &schema, &updates);
        let cleaned = strip_state_tags(&full);
        let mut assistant = Message::new(&session_id, Some(user_msg.id.clone()), Role::Assistant, &full);
        assistant.snapshot_state = new_snapshot;
        assistant.display_content = resolve_display(&path, &full, &cleaned, &regex_rules);
        if let Err(e) = storage.create_message(&assistant).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
        // 激活叶子推进到新助手消息：下一轮发送将挂在它之下。
        let _ = storage.set_session_active_leaf(&session_id, Some(&assistant.id)).await;
        yield SendEvent::Done { message_id: assistant.id };
    }
}

/// Regenerate a fresh assistant reply as a *sibling* of `target_id` (same
/// parent), then point the active leaf at it. The target must be an assistant
/// message.
pub fn regenerate(
    storage: Arc<dyn Storage>,
    provider: Arc<dyn ModelProvider>,
    counter: Arc<dyn TokenCounter>,
    model: String,
    session_id: String,
    target_id: String,
    assets_dir: String,
) -> impl Stream<Item = SendEvent> {
    async_stream::stream! {
        let session = match storage.get_session(&session_id).await {
            Ok(Some(s)) => s,
            Ok(None) => { yield SendEvent::Error("session not found".into()); return; }
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let all = match storage.list_messages(&session_id).await {
            Ok(h) => h,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let target = match all.iter().find(|m| m.id == target_id) {
            Some(m) if m.role == Role::Assistant => m.clone(),
            _ => { yield SendEvent::Error("regenerate target must be an assistant message".into()); return; }
        };
        // context = path root→(target's parent = the user turn that prompted it)
        let path = crate::tree::active_path(&all, target.parent_id.as_deref());

        // 父分支适用摘要：替换 cutoff 之前的历史。
        let summary = applicable_summary(storage.as_ref(), &session_id, &path).await;
        let visible_start = summary.as_ref().map(|(_, i)| i + 1).unwrap_or(0);
        let summary_text = summary.map(|(c, _)| c);

        let mut context: Vec<ChatMessage> = Vec::new();
        for m in path[visible_start..].iter().filter(|m| !m.is_hidden) {
            context.push(chat_message_from(storage.as_ref(), &assets_dir, m).await);
        }

        // 父分支有效状态：折叠基准与 send_message 相同（schema 兜底 + seed + 父叶子快照）。
        let schema = session_schema(storage.as_ref(), &session).await;
        let leaf_snapshot = path.last().map(|m| m.snapshot_state.clone()).unwrap_or_else(|| serde_json::json!({}));
        let branch_state = effective_state(&schema, &session.current_state, &leaf_snapshot);

        let (req, regex_rules) = match assemble_request(storage.as_ref(), &session, model, &context, &branch_state, summary_text.clone()).await {
            Ok(r) => r,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };

        // best-effort 裁剪：超窗口则丢最旧的中段历史（溢出由 provider 报错沿 Error 路径暴露）。
        let window = context_window(storage.as_ref()).await;
        let (trimmed, dropped) = trim_history(&req.messages, window, counter.as_ref());
        if dropped > 0 {
            tracing::warn!(dropped, "context over window: trimmed oldest history");
        }
        let req = ChatRequest { model: req.model, messages: trimmed, summary: req.summary, max_tokens: req.max_tokens };

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
        let updates = parse_state_updates(&full);
        let new_snapshot = apply_updates(&branch_state, &schema, &updates);
        let cleaned = strip_state_tags(&full);
        let mut sibling = Message::new(&session_id, target.parent_id.clone(), Role::Assistant, &full);
        sibling.snapshot_state = new_snapshot;
        sibling.display_content = resolve_display(&path, &full, &cleaned, &regex_rules);
        if let Err(e) = storage.create_message(&sibling).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
        let _ = storage.set_session_active_leaf(&session_id, Some(&sibling.id)).await;
        yield SendEvent::Done { message_id: sibling.id };
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
            "".into(),
            Vec::new(),
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

    async fn drain(stream: impl futures::Stream<Item = SendEvent>) {
        futures::pin_mut!(stream);
        while futures::StreamExt::next(&mut stream).await.is_some() {}
    }

    #[tokio::test]
    async fn effective_regex_rules_global_plus_scoped() {
        let storage = Arc::new(temp_storage().await);
        // global orphan rule (referenced by no node)
        let mut g = crate::models::definition::Definition::new("regex_rule", "G", "");
        g.meta = serde_json::json!({ "pattern": "g", "replacement": "" });
        storage.create_definition(&g).await.unwrap();
        // scoped rule referenced by a template the session uses
        let mut s = crate::models::definition::Definition::new("regex_rule", "S", "");
        s.meta = serde_json::json!({ "pattern": "s", "replacement": "" });
        storage.create_definition(&s).await.unwrap();
        let tmpl = crate::models::template::Template::new("rx");
        storage.create_template(&tmpl).await.unwrap();
        storage.create_node(&crate::models::prompt_node::PromptNode::new_ref(
            crate::models::prompt_node::OwnerKind::Template, &tmpl.id, None, 0, &s.id)).await.unwrap();
        let mut session = Session::new("x");
        session.template_id = Some(tmpl.id.clone());
        storage.create_session(&session).await.unwrap();

        let rules = super::effective_regex_rules(storage.as_ref(), &session).await.unwrap();
        let names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["G", "S"], "global orphan first, then scoped");

        // A different session without that template gets only the global rule.
        let other = Session::new("y");
        storage.create_session(&other).await.unwrap();
        let other_rules = super::effective_regex_rules(storage.as_ref(), &other).await.unwrap();
        assert_eq!(other_rules.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["G"]);
    }

    #[tokio::test]
    async fn send_chains_under_active_leaf_and_updates_it() {
        let storage: Arc<dyn Storage> = Arc::new(temp_storage().await);
        let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let session = Session::new("Chat");
        storage.create_session(&session).await.unwrap();

        // first turn
        drain(send_message(storage.clone(), provider.clone(), counter.clone(),
            "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new())).await;
        let s1 = storage.get_session(&session.id).await.unwrap().unwrap();
        let msgs1 = storage.list_messages(&session.id).await.unwrap();
        assert_eq!(msgs1.len(), 2); // user + assistant
        let assistant1 = msgs1.iter().find(|m| m.role == Role::Assistant).unwrap();
        assert_eq!(s1.active_leaf_id.as_deref(), Some(assistant1.id.as_str()));

        // second turn chains under the previous assistant (the active leaf)
        drain(send_message(storage.clone(), provider.clone(), counter.clone(),
            "m".into(), session.id.clone(), "again".into(), "".into(), Vec::new())).await;
        let msgs2 = storage.list_messages(&session.id).await.unwrap();
        let user2 = msgs2.iter().find(|m| m.role == Role::User && m.raw_content == "again").unwrap();
        assert_eq!(user2.parent_id.as_deref(), Some(assistant1.id.as_str()));
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
            "".into(),
            Vec::new(),
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
    async fn send_message_respects_per_entry_recursive() {
        use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
        use crate::models::template::Template;
        let storage = Arc::new(temp_storage().await);

        // A is a constant source whose content mentions zion, but it opts out of
        // recursion (per-definition scan setting), so it must not activate B.
        let mut a = crate::models::definition::Definition::new("world", "A", "We mention zion here");
        a.meta = serde_json::json!({ "scan": { "recursive": false } });
        let mut b = crate::models::definition::Definition::new("world", "B", "Zion lore");
        b.meta = serde_json::json!({ "trigger": { "mode": "keyword", "keys": ["zion"] } });
        storage.create_definition(&a).await.unwrap();
        storage.create_definition(&b).await.unwrap();

        let t = Template::new("T");
        storage.create_template(&t).await.unwrap();
        let wf = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "world");
        storage.create_node(&wf).await.unwrap();
        storage.create_node(&PromptNode::new_ref(OwnerKind::Template, &t.id, Some(wf.id.clone()), 0, &a.id)).await.unwrap();
        storage.create_node(&PromptNode::new_ref(OwnerKind::Template, &t.id, Some(wf.id.clone()), 1, &b.id)).await.unwrap();
        let mut hist = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 1, "history");
        hist.kind = NodeKind::History; hist.tag = None;
        storage.create_node(&hist).await.unwrap();

        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        storage.create_session(&session).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        // user says nothing about zion → A constant active, B only if recursion scans A's content.
        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hello".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        let sys = &req.messages[0].content;
        assert!(sys.contains("We mention zion here"), "constant A present");
        assert!(!sys.contains("Zion lore"), "B must NOT activate with recursion off");
    }

    #[tokio::test]
    async fn regex_rule_sets_display_content() {
        let storage = Arc::new(temp_storage().await);
        let mut session = Session::new("t");
        let mut rule = crate::models::definition::Definition::new("regex_rule", "R", "");
        rule.meta = serde_json::json!({ "pattern": "STOP", "replacement": "" });
        storage.create_definition(&rule).await.unwrap();
        // Regex is now scoped to the session's template tree: build a template
        // that references this rule and attach it to the session.
        let tmpl = crate::models::template::Template::new("rx");
        storage.create_template(&tmpl).await.unwrap();
        let rxref = crate::models::prompt_node::PromptNode::new_ref(
            crate::models::prompt_node::OwnerKind::Template,
            &tmpl.id,
            None,
            0,
            &rule.id,
        );
        storage.create_node(&rxref).await.unwrap();
        session.template_id = Some(tmpl.id.clone());
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
            "".into(),
            Vec::new(),
        );
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let assistant = msgs.iter().find(|m| m.role == Role::Assistant).unwrap();
        assert_eq!(assistant.raw_content, "helloSTOP");
        assert_eq!(assistant.display_content.as_deref(), Some("hello"));
    }

    #[tokio::test]
    async fn html_card_patch_reconstructs_display_and_injects_instruction() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("t"); // tree-less: history defaults on
        storage.create_session(&session).await.unwrap();
        crate::seed::ensure_builtin_definitions(storage.as_ref()).await.unwrap();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        // Turn 1: the model emits a full HTML card. No card existed yet, so the
        // patch instruction must NOT be injected this turn.
        let card = "<!DOCTYPE html>\n<html><body><p>HP: 100</p></body></html>";
        let seen1 = Arc::new(Mutex::new(None));
        let p1: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen1.clone(), reply: card.into() });
        let s1 = send_message(storage.clone(), p1, counter.clone(), "m".into(), session.id.clone(), "draw".into(), "".into(), Vec::new());
        futures::pin_mut!(s1);
        while s1.next().await.is_some() {}
        let req1 = seen1.lock().unwrap().clone().unwrap();
        assert!(
            !req1.messages.iter().any(|m| m.content.contains("<<<<<<< SEARCH")),
            "no patch instruction before any card exists",
        );

        // Turn 2: a card is now in history → the instruction is injected, and a
        // SEARCH/REPLACE reply is reconstructed into a full document for display
        // while raw_content keeps the compact patch.
        let patch = "<<<<<<< SEARCH\n<p>HP: 100</p>\n=======\n<p>HP: 80</p>\n>>>>>>> REPLACE";
        let seen2 = Arc::new(Mutex::new(None));
        let p2: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen2.clone(), reply: patch.into() });
        let s2 = send_message(storage.clone(), p2, counter, "m".into(), session.id.clone(), "hit".into(), "".into(), Vec::new());
        futures::pin_mut!(s2);
        while s2.next().await.is_some() {}

        let req2 = seen2.lock().unwrap().clone().unwrap();
        assert!(
            req2.messages.iter().any(|m| m.role == Role::System && m.content.contains("<<<<<<< SEARCH")),
            "patch instruction injected once a card is present",
        );

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let patched = msgs.iter().filter(|m| m.role == Role::Assistant).last().unwrap();
        assert_eq!(patched.raw_content, patch, "raw_content keeps the compact patch");
        let display = patched.display_content.as_deref().unwrap();
        assert!(display.starts_with("<!DOCTYPE html>"), "display is the full reconstructed doc");
        assert!(display.contains("<p>HP: 80</p>"));
        assert!(!display.contains("HP: 100"));
    }

    #[tokio::test]
    async fn state_protocol_injected_only_when_user_vars_declared() {
        let storage = Arc::new(temp_storage().await);
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        // Template declaring a user variable `hp`.
        let mut t = crate::models::template::Template::new("T");
        t.meta = serde_json::json!({ "variables": [ {"name":"hp","type":"number","initial":100} ] });
        storage.create_template(&t).await.unwrap();
        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        storage.create_session(&session).await.unwrap();
        crate::seed::ensure_builtin_definitions(storage.as_ref()).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let s = send_message(storage.clone(), provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
        futures::pin_mut!(s);
        while s.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        let sys = req.messages.iter().filter(|m| m.role == Role::System).map(|m| m.content.clone()).collect::<Vec<_>>().join("\n");
        assert!(sys.contains("<state_update"), "protocol text injected");
        assert!(sys.contains("- hp (number) = 100"), "live variable list appended");
    }

    #[tokio::test]
    async fn state_protocol_absent_without_user_vars() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("s"); // no template → only system vars
        storage.create_session(&session).await.unwrap();
        crate::seed::ensure_builtin_definitions(storage.as_ref()).await.unwrap();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let s = send_message(storage.clone(), provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
        futures::pin_mut!(s);
        while s.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        assert!(!req.messages.iter().any(|m| m.content.contains("<state_update")), "no state protocol without user vars");
    }

    #[tokio::test]
    async fn global_regex_rule_applies_without_a_tree() {
        // A rule created in Settings is an orphan def (referenced by no node).
        // Hybrid model: global rules apply to every session's display output,
        // even one with no template/tree.
        let storage = Arc::new(temp_storage().await);
        let mut rule = crate::models::definition::Definition::new("regex_rule", "G", "");
        rule.meta = serde_json::json!({ "pattern": "STOP", "replacement": "" });
        storage.create_definition(&rule).await.unwrap();
        let session = Session::new("t"); // no template, no nodes
        storage.create_session(&session).await.unwrap();

        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: Arc::new(Mutex::new(None)),
            reply: "helloSTOP".into(),
        });
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage.clone(), provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let assistant = msgs.iter().find(|m| m.role == Role::Assistant).unwrap();
        assert_eq!(assistant.display_content.as_deref(), Some("hello"));
    }

    #[tokio::test]
    async fn scoped_regex_rule_does_not_leak_to_other_sessions() {
        // A rule mounted in loreset A's tree is NOT an orphan, so it must not
        // apply to an unrelated tree-less session.
        let storage = Arc::new(temp_storage().await);
        let mut rule = crate::models::definition::Definition::new("regex_rule", "R", "");
        rule.meta = serde_json::json!({ "pattern": "STOP", "replacement": "" });
        storage.create_definition(&rule).await.unwrap();
        let tmpl = crate::models::template::Template::new("rx");
        storage.create_template(&tmpl).await.unwrap();
        let rxref = crate::models::prompt_node::PromptNode::new_ref(
            crate::models::prompt_node::OwnerKind::Template, &tmpl.id, None, 0, &rule.id,
        );
        storage.create_node(&rxref).await.unwrap();
        // A different session that does NOT use that template.
        let session = Session::new("other");
        storage.create_session(&session).await.unwrap();

        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: Arc::new(Mutex::new(None)),
            reply: "helloSTOP".into(),
        });
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage.clone(), provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let assistant = msgs.iter().find(|m| m.role == Role::Assistant).unwrap();
        // Rule belongs to loreset A's tree, so the reply is untouched here.
        assert_eq!(assistant.display_content.as_deref(), None);
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
            "".into(),
            Vec::new(),
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

    #[tokio::test]
    async fn state_update_folds_into_snapshot_and_strips_display() {
        let storage = Arc::new(temp_storage().await);
        let mut t = crate::models::template::Template::new("T");
        t.meta = serde_json::json!({ "variables": [ {"name":"hp","type":"number","initial":100} ] });
        storage.create_template(&t).await.unwrap();
        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        session.current_state = serde_json::json!({ "hp": 100 });
        storage.create_session(&session).await.unwrap();

        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: Arc::new(Mutex::new(None)),
            reply: "You take a hit. <state_update action=\"SUB\" key=\"hp\" value=\"5\"/>".into(),
        });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let assistant = msgs.iter().find(|m| m.role == Role::Assistant).unwrap();
        assert_eq!(assistant.snapshot_state["hp"], 95); // folded
        assert_eq!(assistant.display_content.as_deref(), Some("You take a hit.")); // tag stripped
        assert!(assistant.raw_content.contains("<state_update")); // raw keeps the tag
    }

    #[tokio::test]
    async fn assembly_renders_the_active_branch_state() {
        let storage = Arc::new(temp_storage().await);
        // a char definition that renders {{hp}}
        let ch = crate::models::definition::Definition::new("char", "C", "HP is {{hp}}");
        storage.create_definition(&ch).await.unwrap();
        let t = crate::models::template::Template::new("T");
        storage.create_template(&t).await.unwrap();
        let f = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "char");
        storage.create_node(&f).await.unwrap();
        storage.create_node(&PromptNode::new_ref(OwnerKind::Template, &t.id, Some(f.id.clone()), 0, &ch.id)).await.unwrap();

        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        session.current_state = serde_json::json!({ "hp": 100 });
        storage.create_session(&session).await.unwrap();

        // seed an existing assistant leaf whose snapshot has hp=42, and point the leaf at it
        let mut leaf = Message::new(&session.id, None, Role::Assistant, "prior");
        leaf.snapshot_state = serde_json::json!({ "hp": 42 });
        storage.create_message(&leaf).await.unwrap();
        storage.set_session_active_leaf(&session.id, Some(&leaf.id)).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        assert!(req.messages[0].content.contains("HP is 42"), "assembly must read the branch leaf snapshot");
    }

    #[tokio::test]
    async fn send_with_attachments_persists_ids_and_resolves_images_for_provider() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("s");
        storage.create_session(&session).await.unwrap();

        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("pic.png"), b"\x89PNG-fake-bytes").await.unwrap();
        let asset = crate::models::asset::Asset {
            id: "a1".into(),
            name: "pic".into(),
            path: "pic.png".into(),
            created_at: "".into(),
        };
        storage.create_asset(&asset).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(
            storage_dyn,
            provider,
            counter,
            "m".into(),
            session.id.clone(),
            "look at this".into(),
            dir.path().to_str().unwrap().to_string(),
            vec!["a1".to_string()],
        );
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let user = msgs.iter().find(|m| m.role == Role::User).unwrap();
        assert_eq!(user.attachments, vec!["a1".to_string()]);

        let req = seen.lock().unwrap().clone().unwrap();
        let user_turn = req.messages.last().unwrap();
        assert_eq!(user_turn.content, "look at this");
        assert_eq!(user_turn.images.len(), 1);
        assert!(user_turn.images[0].starts_with("data:image/png;base64,"));
    }

    #[tokio::test]
    async fn oversized_history_is_trimmed_before_send() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("s");
        storage.create_session(&session).await.unwrap();
        // 造一串长 user/assistant 历史（无摘要）
        let mut parent: Option<String> = None;
        let mut leaf = String::new();
        for i in 0..6 {
            let content = format!("turn-{i}-{}", "x".repeat(50));
            let role = if i % 2 == 0 { Role::User } else { Role::Assistant };
            let m = Message::new(&session.id, parent.clone(), role, &content);
            storage.create_message(&m).await.unwrap();
            parent = Some(m.id.clone());
            leaf = m.id.clone();
        }
        storage.set_session_active_leaf(&session.id, Some(&leaf)).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        // window 设得很小 → 触发裁剪
        storage.set_setting("context.window", &serde_json::json!(20)).await.unwrap();

        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "newest".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        let joined: String = req.messages.iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("|");
        assert!(joined.contains("newest"), "末条本次 user 必须保留");
        assert!(!joined.contains("turn-0"), "最旧历史应被裁掉");
    }

    #[tokio::test]
    async fn assembly_uses_applicable_summary_and_truncates_history() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("s");
        storage.create_session(&session).await.unwrap();

        // 线性历史：u1 -> a1 -> u2 -> a2（a2 为活动叶子）
        let u1 = Message::new(&session.id, None, Role::User, "u1");
        storage.create_message(&u1).await.unwrap();
        let a1 = Message::new(&session.id, Some(u1.id.clone()), Role::Assistant, "a1");
        storage.create_message(&a1).await.unwrap();
        let u2 = Message::new(&session.id, Some(a1.id.clone()), Role::User, "u2");
        storage.create_message(&u2).await.unwrap();
        let a2 = Message::new(&session.id, Some(u2.id.clone()), Role::Assistant, "a2");
        storage.create_message(&a2).await.unwrap();
        storage.set_session_active_leaf(&session.id, Some(&a2.id)).await.unwrap();

        // 摘要覆盖到 a1（cutoff = a1）：u1/a1 不应进 context，summary 应被携带。
        let sum = crate::models::summary::Summary::new(&session.id, &a1.id, "[sum] u1 a1 happened");
        storage.create_summary(&sum).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "u3".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        assert_eq!(req.summary.as_deref(), Some("[sum] u1 a1 happened"));
        let joined: String = req.messages.iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("|");
        assert!(!joined.contains("u1"), "cutoff 之前的历史不应进 context: {joined}");
        assert!(!joined.contains("a1"), "cutoff 之前的历史不应进 context: {joined}");
        assert!(joined.contains("u2"), "cutoff 之后的历史应保留");
        assert!(joined.contains("u3"), "本次 user 应保留");
    }

    #[tokio::test]
    async fn regenerate_folds_state_from_the_parent_branch() {
        let storage = Arc::new(temp_storage().await);
        let mut t = crate::models::template::Template::new("T");
        t.meta = serde_json::json!({ "variables": [ {"name":"hp","type":"number","initial":100} ] });
        storage.create_template(&t).await.unwrap();
        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        session.current_state = serde_json::json!({ "hp": 100 });
        storage.create_session(&session).await.unwrap();

        // user -> assistant(hp 90); regenerate the assistant from the same parent
        let user = Message::new(&session.id, None, Role::User, "go");
        storage.create_message(&user).await.unwrap();
        let mut a1 = Message::new(&session.id, Some(user.id.clone()), Role::Assistant, "first");
        a1.snapshot_state = serde_json::json!({ "hp": 90 });
        storage.create_message(&a1).await.unwrap();
        storage.set_session_active_leaf(&session.id, Some(&a1.id)).await.unwrap();

        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: Arc::new(Mutex::new(None)),
            reply: "retry <state_update action=\"SUB\" key=\"hp\" value=\"20\"/>".into(),
        });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = regenerate(storage_dyn, provider, counter, "m".into(), session.id.clone(), a1.id.clone(), "".into());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let sibling = msgs.iter().filter(|m| m.role == Role::Assistant).find(|m| m.id != a1.id).unwrap();
        // parent branch state at the user turn is hp=100 (the user carries the seed); SUB 20 -> 80
        assert_eq!(sibling.snapshot_state["hp"], 80);
        assert_eq!(sibling.display_content.as_deref(), Some("retry"));
    }
}
