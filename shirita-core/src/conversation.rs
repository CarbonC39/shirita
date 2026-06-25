//! Conversation Service: Sends messages and streams the assistant's replies; stores them in the database upon completion.

use std::sync::Arc;

use futures::{Stream, StreamExt};

use crate::assembly::capture_panel_updates;
use crate::attachments::resolve_images;
use crate::model::{ChatMessage, ChatRequest, ModelProvider};
use crate::models::definition::Definition;
use crate::models::message::{Message, Role};
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::budget::trim_history;
use crate::models::session::Session;
use crate::models::summary::Summary;
use crate::state::{
    apply_updates, effective_state, parse_state_updates, strip_state_tags, VarDecl,
};
use crate::storage::Storage;
use crate::tokenizer::TokenCounter;

/// Load the definitions referenced by a node tree, de-duplicated by id.
pub(crate) async fn load_defs(
    storage: &dyn Storage,
    nodes: &[PromptNode],
) -> crate::Result<std::collections::HashMap<String, Definition>> {
    let mut defs = std::collections::HashMap::new();
    for n in nodes {
        if let Some(did) = &n.definition_id {
            if !defs.contains_key(did) {
                if let Ok(Some(d)) = storage.get_definition(did).await {
                    defs.insert(did.clone(), d);
                }
            }
        }
    }
    Ok(defs)
}

/// Resolve a session's effective variable schema from `variables` bricks across
/// the effective template/session tree and each mounted pack (mount order).
pub async fn resolve_session_schema(storage: &dyn Storage, session: &Session) -> Vec<VarDecl> {
    let nodes = effective_nodes(storage, session).await.unwrap_or_default();
    let defs = load_defs(storage, &nodes).await.unwrap_or_default();
    let template_decls = crate::state::variables_from_nodes(&nodes, &defs);

    let mut pack_decls = Vec::new();
    for pid in &session.mounted_packs {
        let pnodes = storage.list_nodes(&OwnerKind::Pack, pid).await.unwrap_or_default();
        let pdefs = load_defs(storage, &pnodes).await.unwrap_or_default();
        pack_decls.push(crate::state::variables_from_nodes(&pnodes, &pdefs));
    }
    crate::state::resolve_schema_from_bricks(template_decls, pack_decls, &session.override_config)
}

/// Read the context window (settings `context.window`, default 200000).
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

/// The maximum number of tokens for reading replies (output) (settings `provider_max_tokens`). If `None`, the default value is determined by each provider.
/// (Anthropic 8192 / OpenAI omitted). Note: This is the output limit and is independent of the context window (`context.window`).
async fn provider_max_tokens(storage: &dyn Storage) -> Option<u32> {
    storage
        .get_setting("provider_max_tokens")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
}

/// Selects the summary applicable to the current branch: The `cutoff` must fall on the `active` path; if there are multiple paths, the one with the latest `cutoff` is chosen.
/// Returns (the summary content, the index of the `cutoff` in the path).
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

/// Session valid node tree: Give priority to own nodes (after a fork); otherwise, use the template.
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

/// The node trees of the session's mounted packs, in mount order (empty trees skipped).
pub async fn mounted_pack_trees(
    storage: &dyn Storage,
    session: &Session,
) -> crate::Result<Vec<Vec<PromptNode>>> {
    let mut trees = Vec::new();
    for pid in &session.mounted_packs {
        let nodes = storage.list_nodes(&OwnerKind::Pack, pid).await?;
        if !nodes.is_empty() {
            trees.push(nodes);
        }
    }
    Ok(trees)
}

/// Regex rules in effect for this session: global orphan rules (not referenced by any node; apply everywhere) + scoped rules
/// referenced by `ref` in the `effective` tree for this session. These two sets are mutually exclusive; global rules take precedence.
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
    // Mounted packs' scoped regex rules, in mount order then pack node order —
    // extends the deterministic pipeline after the template/session tree's rules.
    for pid in &session.mounted_packs {
        for n in storage.list_nodes(&crate::models::prompt_node::OwnerKind::Pack, pid).await? {
            if n.kind == crate::models::prompt_node::NodeKind::Ref && n.enabled {
                if let Some(d) = n.definition_id.as_deref().and_then(|id| by_id.get(id)) {
                    if d.def_type == "regex_rule" {
                        rules.push((*d).clone());
                    }
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
    // Load mounted-pack node trees and their definitions.
    let pack_trees = mounted_pack_trees(storage, session).await?;
    for tree in &pack_trees {
        for n in tree {
            if let Some(did) = &n.definition_id {
                if !defs.contains_key(did) {
                    if let Ok(Some(d)) = storage.get_definition(did).await {
                        defs.insert(did.clone(), d);
                    }
                }
            }
        }
    }
    let local = session
        .override_config
        .get("local_definitions")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    // Scan window: Retrieve the most recent entries (including the latest user); for each World Book entry, scan the last N entries within the window based on its own
    // meta.scan.depth (the setting has been delegated to the definition itself).
    const MAX_SCAN_WINDOW: usize = 20;
    let mut recent: Vec<String> =
        context.iter().rev().take(MAX_SCAN_WINDOW).map(|m| m.content.clone()).collect();
    recent.reverse();

    // StdRng (not ThreadRng): Send; can safely span subsequent `await` statements (SSE streams require `Send`).
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_entropy();
    let mut plan = crate::assembly::assemble_from_nodes_with_packs(
        &nodes,
        &pack_trees,
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
    let schema = resolve_session_schema(storage, session).await;
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

    // Global orphan rules + this session's tree-scoped rules (see effective_regex_rules).
    let regex_rules = effective_regex_rules(storage, session).await?;

    // Prompt-side regex: rewrite the outgoing copy of each chat message by role
    // (scope ∈ {prompt, both}); raw_content is untouched. World-info scanning above
    // already used the original `context`, so triggers are unaffected.
    let prompt_context: Vec<ChatMessage> = context
        .iter()
        .map(|m| {
            let target = match m.role {
                Role::Assistant => Some(crate::assembly::RegexTarget::AiOutput),
                Role::User => Some(crate::assembly::RegexTarget::UserInput),
                Role::System => None,
            };
            match target {
                Some(t) => {
                    let content = crate::assembly::apply_regex_rules_for(
                        &m.content, &regex_rules, t, crate::assembly::RegexPhase::Prompt,
                    )
                    .unwrap_or_else(|| m.content.clone());
                    ChatMessage { content, ..m.clone() }
                }
                None => m.clone(),
            }
        })
        .collect();

    // If an explicit history node exists, use its enabled status; if no node exists (such as in a free session without a template), it is included in the history by default.
    let has_history_node = nodes.iter().any(|n| n.kind == NodeKind::History);
    let include_history = plan.history_enabled || !has_history_node;
    let chat_messages = crate::assembly::build_chat_messages(&plan, &prompt_context, include_history);

    let max_tokens = provider_max_tokens(storage).await;
    Ok((ChatRequest { model, messages: chat_messages, summary, max_tokens }, regex_rules))
}

/// Convert a `Message` that has already been stored in the database into a `ChatMessage` to be sent to the provider, and parse its
/// `attachments` (asset IDs) into image data URLs.
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

/// Write-side display_content: Only transformations unrelated to regex rules—prioritize HTML-card reconstruction, otherwise
/// Text after state tags have been stripped (stored only if different from the original). Display-side regex calculations are now performed on the read side in real time
fn resolve_display(path: &[&Message], full: &str, cleaned: &str) -> Option<String> {
    if let Some(html) = crate::html_patch::reconstruct(latest_html_card(path).as_deref(), cleaned) {
        return Some(html);
    }
    (cleaned != full).then(|| cleaned.to_string())
}

/// Events exposed by the streaming send process.
#[derive(Debug, Clone, PartialEq)]
pub enum SendEvent {
    /// A text increment.
    Delta(String),
    /// Completion, with an assistant message ID.
    Done { message_id: String },
    /// Error (the stream terminates afterward).
    Error(String),
}

/// Send a user message: store user → assemble history → call provider for streaming → accumulate → store assistant.
/// Returns an event stream; assistant messages are written to storage after a complete response is received, and only then does it yield `Done`.
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
        // 0) Verify that the session exists (before any writes, to avoid relying on foreign key constraints as a fallback).
        let session = match storage.get_session(&session_id).await {
            Ok(Some(s)) => s,
            Ok(None) => { yield SendEvent::Error("session not found".into()); return; }
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };

        // 1) parent = the currently active leaf (at the end of the branch of `active_leaf`); store the `user` message in the database.
        let all = match storage.list_messages(&session_id).await {
            Ok(h) => h,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let path = crate::tree::active_path(&all, session.active_leaf_id.as_deref());
        let parent_id = path.last().map(|m| m.id.clone());

        // Valid branch state: schema initial value < seed < current leaf snapshot (read and write sides share the same fallback).
        let schema = resolve_session_schema(storage.as_ref(), &session).await;
        let leaf_snapshot = path.last().map(|m| m.snapshot_state.clone()).unwrap_or_else(|| serde_json::json!({}));
        let branch_state = effective_state(&schema, &session.current_state, &leaf_snapshot);

        let mut user_msg = Message::new(&session_id, parent_id, Role::User, &user_text);
        user_msg.snapshot_state = branch_state.clone();
        user_msg.attachments = attachment_ids.clone();
        if let Err(e) = storage.create_message(&user_msg).await {
            yield SendEvent::Error(e.to_string());
            return;
        }

        // Summary for the current branch: Replaces the history prior to the cutoff; history after the cutoff is included as usual.
        let summary = applicable_summary(storage.as_ref(), &session_id, &path).await;
        let visible_start = summary.as_ref().map(|(_, i)| i + 1).unwrap_or(0);
        let summary_text = summary.map(|(c, _)| c);

        // 2) Assembly: context = the messages visible in the branch after the cutoff (filtered to hide certain messages) + the current user.
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

        // Best-effort pruning: If the data exceeds the window size, the oldest mid-range history is discarded (overflow is reported as an error by the provider and exposed via the Error path).
        let window = context_window(storage.as_ref()).await;
        let (trimmed, dropped) = trim_history(&req.messages, window, counter.as_ref());
        if dropped > 0 {
            tracing::warn!(dropped, "context over window: trimmed oldest history");
        }
        let req = ChatRequest { model: req.model, messages: trimmed, summary: req.summary, max_tokens: req.max_tokens };

        let prompt_text: String =
            req.messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n");
        tracing::debug!(prompt_tokens = counter.count(&prompt_text), "assembled prompt");

        // 3) Process the provider stream, accumulating and yielding one delta at a time.
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

        // 4) Fold <state_update> into the snapshot, strip the display text, store the assistant message in the database, and then yield Done.
        // map the capture variables from the panel (regex_rule.meta.capture_vars) to the <state_update> tag
        // synchronous folding: The former is read-only extraction and does not affect the stripping of display text. Captures come first, followed by `state_update`—
        // when keys with the same name conflict, explicit model instructions take precedence over the extracted values (since `apply_updates` is executed in order, the latter overrides the former).
        let mut updates = capture_panel_updates(&full, &regex_rules);
        updates.extend(parse_state_updates(&full));
        let new_snapshot = apply_updates(&branch_state, &schema, &updates);
        let cleaned = strip_state_tags(&full);
        let mut assistant = Message::new(&session_id, Some(user_msg.id.clone()), Role::Assistant, &full);
        assistant.snapshot_state = new_snapshot;
        assistant.display_content = resolve_display(&path, &full, &cleaned);
        if let Err(e) = storage.create_message(&assistant).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
        // Activate the leaf node to advance to the new assistant message: The next round of messages will be attached to it.
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

        // Summary for the parent branch: Replaces the history prior to the cutoff.
        let summary = applicable_summary(storage.as_ref(), &session_id, &path).await;
        let visible_start = summary.as_ref().map(|(_, i)| i + 1).unwrap_or(0);
        let summary_text = summary.map(|(c, _)| c);

        let mut context: Vec<ChatMessage> = Vec::new();
        for m in path[visible_start..].iter().filter(|m| !m.is_hidden) {
            context.push(chat_message_from(storage.as_ref(), &assets_dir, m).await);
        }

        // Valid state of the parent branch: The folding reference point is the same as `send_message` (schema fallback + seed + parent leaf snapshot).
        let schema = resolve_session_schema(storage.as_ref(), &session).await;
        let leaf_snapshot = path.last().map(|m| m.snapshot_state.clone()).unwrap_or_else(|| serde_json::json!({}));
        let branch_state = effective_state(&schema, &session.current_state, &leaf_snapshot);

        let (req, regex_rules) = match assemble_request(storage.as_ref(), &session, model, &context, &branch_state, summary_text.clone()).await {
            Ok(r) => r,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };

        // Best-effort pruning: If the data exceeds the window size, the oldest mid-range history is discarded (overflow is reported as an error by the provider and exposed via the Error path).
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
        // Same as above: `capture` comes first, followed by `state_update`; in case of a conflict, the explicit instruction takes precedence.
        let mut updates = capture_panel_updates(&full, &regex_rules);
        updates.extend(parse_state_updates(&full));
        let new_snapshot = apply_updates(&branch_state, &schema, &updates);
        let cleaned = strip_state_tags(&full);
        let mut sibling = Message::new(&session_id, target.parent_id.clone(), Role::Assistant, &full);
        sibling.snapshot_state = new_snapshot;
        sibling.display_content = resolve_display(&path, &full, &cleaned);
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

    /// Add a `variables` brick (ref node) to a template's root, declaring `decls`
    /// (each a `{"name", "type", "initial"}` json object). Mirrors what the UI
    /// does when a user drops a `variables` brick into a template tree.
    async fn add_variables_brick(
        storage: &dyn Storage,
        template_id: &str,
        decls: serde_json::Value,
    ) {
        use crate::models::definition::Definition;
        use crate::models::prompt_node::{OwnerKind, PromptNode};

        let mut def = Definition::new("variables", "Vars", "");
        def.meta = serde_json::json!({ "decls": decls });
        storage.create_definition(&def).await.unwrap();
        let r = PromptNode::new_ref(OwnerKind::Template, template_id, None, 0, &def.id);
        storage.create_node(&r).await.unwrap();
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

        // Persistence check: one entry each for "user" and "assistant"; the content is correct.
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

        // Template tree: char container → ref(char), plus the history magic node.
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
        // After the history node, the user forwards this to the provider.
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
    async fn mounted_pack_content_reaches_the_prompt() {
        let storage: Arc<dyn Storage> = Arc::new(temp_storage().await);
        // template: content + history
        let t = crate::models::template::Template::new("T");
        storage.create_template(&t).await.unwrap();
        let mut content = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "content");
        content.kind = NodeKind::Content; content.tag = None;
        storage.create_node(&content).await.unwrap();
        let mut hist = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 1, "history");
        hist.kind = NodeKind::History; hist.tag = None;
        storage.create_node(&hist).await.unwrap();
        // pack with a char def
        let p = crate::models::pack::Pack::new("Alice");
        storage.create_pack(&p).await.unwrap();
        let mut def = Definition::new("char", "Alice", "Alice is brave.");
        def.id = "d_alice".into();
        storage.create_definition(&def).await.unwrap();
        storage.create_node(&PromptNode::new_ref(OwnerKind::Pack, &p.id, None, 0, &def.id)).await.unwrap();
        // session mounting template + pack
        let mut session = Session::new("Chat");
        session.template_id = Some(t.id.clone());
        session.mounted_packs = vec![p.id.clone()];
        storage.create_session(&session).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage.clone(), provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().expect("a request was sent");
        let system_blob: String = req.messages.iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("\n");
        assert!(system_blob.contains("<char>") && system_blob.contains("Alice is brave."),
            "mounted pack char content appears in the assembled prompt");
    }

    #[tokio::test]
    async fn state_protocol_injected_only_when_user_vars_declared() {
        let storage = Arc::new(temp_storage().await);
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        // Template declaring a user variable `hp`.
        let t = crate::models::template::Template::new("T");
        storage.create_template(&t).await.unwrap();
        add_variables_brick(
            storage.as_ref(),
            &t.id,
            serde_json::json!([ {"name":"hp","type":"number","initial":100} ]),
        )
        .await;
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
    async fn effective_regex_includes_mounted_pack_rules_in_order() {
        let storage: Arc<dyn Storage> = Arc::new(temp_storage().await);
        // a global orphan rule (referenced by nothing)
        let mut global = Definition::new("regex_rule", "global", "");
        global.id = "r_global".into();
        global.meta = serde_json::json!({ "pattern": "a", "replacement": "b" });
        storage.create_definition(&global).await.unwrap();
        // a pack with a scoped regex rule
        let p = crate::models::pack::Pack::new("FX");
        storage.create_pack(&p).await.unwrap();
        let mut scoped = Definition::new("regex_rule", "scoped", "");
        scoped.id = "r_scoped".into();
        scoped.meta = serde_json::json!({ "pattern": "x", "replacement": "y" });
        storage.create_definition(&scoped).await.unwrap();
        storage.create_node(&PromptNode::new_ref(OwnerKind::Pack, &p.id, None, 0, &scoped.id)).await.unwrap();

        let mut session = Session::new("Chat");
        session.mounted_packs = vec![p.id.clone()];
        storage.create_session(&session).await.unwrap();

        let rules = super::effective_regex_rules(storage.as_ref(), &session).await.unwrap();
        let ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["r_global", "r_scoped"], "global first, then mounted-pack scoped");
    }

    #[tokio::test]
    async fn prompt_side_regex_rewrites_outgoing_not_raw() {
        let storage = Arc::new(temp_storage().await);
        // rule: replace "dog"->"cat" on user_input, prompt scope.
        let mut rule = crate::models::definition::Definition::new("regex_rule", "R", "");
        rule.meta = serde_json::json!({ "pattern": "dog", "replacement": "cat", "scope": "prompt", "targets": ["user_input"] });
        storage.create_definition(&rule).await.unwrap();
        let tmpl = crate::models::template::Template::new("rx");
        storage.create_template(&tmpl).await.unwrap();
        storage.create_node(&crate::models::prompt_node::PromptNode::new_ref(
            crate::models::prompt_node::OwnerKind::Template, &tmpl.id, None, 0, &rule.id)).await.unwrap();
        let mut session = Session::new("s");
        session.template_id = Some(tmpl.id.clone());
        storage.create_session(&session).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let s = send_message(storage.clone(), provider, counter, "m".into(), session.id.clone(), "my dog".into(), "".into(), Vec::new());
        futures::pin_mut!(s);
        while s.next().await.is_some() {}

        // Outgoing prompt has "my cat"; stored user raw_content keeps "my dog".
        let req = seen.lock().unwrap().clone().unwrap();
        assert!(req.messages.iter().any(|m| m.role == Role::User && m.content == "my cat"));
        let msgs = storage.list_messages(&session.id).await.unwrap();
        assert!(msgs.iter().any(|m| m.role == Role::User && m.raw_content == "my dog"));
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
        // Key point: No messages were created.
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

    async fn seed_template_with_variables_brick(
        decls: &[(&str, &str, serde_json::Value)],
    ) -> (Arc<dyn Storage>, Session) {
        use crate::models::definition::Definition;
        use crate::models::prompt_node::{OwnerKind, PromptNode};
        use crate::models::template::Template;

        let storage: Arc<dyn Storage> = Arc::new(temp_storage().await);

        let mut def = Definition::new("variables", "Vars", "");
        let decl_json: Vec<serde_json::Value> = decls
            .iter()
            .map(|(name, ty, initial)| {
                serde_json::json!({ "name": name, "type": ty, "initial": initial })
            })
            .collect();
        def.meta = serde_json::json!({ "decls": decl_json });
        storage.create_definition(&def).await.unwrap();

        let t = Template::new("T");
        storage.create_template(&t).await.unwrap();
        let r = PromptNode::new_ref(OwnerKind::Template, &t.id, None, 0, &def.id);
        storage.create_node(&r).await.unwrap();

        let mut sess = Session::new("s");
        sess.template_id = Some(t.id.clone());
        storage.create_session(&sess).await.unwrap();

        (storage, sess)
    }

    #[tokio::test]
    async fn resolve_session_schema_reads_template_variables_bricks() {
        use serde_json::json;
        let (storage, session) =
            seed_template_with_variables_brick(&[("hp", "number", json!(100))]).await;

        let schema = super::resolve_session_schema(storage.as_ref(), &session).await;
        assert!(schema.iter().any(|d| d.name == "hp" && d.scope.as_deref() == Some("template")));
        assert!(schema.iter().any(|d| d.name == "$avatar")); // system always present
    }

    #[tokio::test]
    async fn state_update_folds_into_snapshot_and_strips_display() {
        let storage = Arc::new(temp_storage().await);
        let t = crate::models::template::Template::new("T");
        storage.create_template(&t).await.unwrap();
        add_variables_brick(
            storage.as_ref(),
            &t.id,
            serde_json::json!([ {"name":"hp","type":"number","initial":100} ]),
        )
        .await;
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
    async fn converted_panel_capture_folds_into_snapshot_alongside_state_update_tags() {
        let storage = Arc::new(temp_storage().await);
        let t = crate::models::template::Template::new("T");
        storage.create_template(&t).await.unwrap();
        add_variables_brick(
            storage.as_ref(),
            &t.id,
            serde_json::json!([
                { "name": "hp", "type": "number", "initial": 100 },
                { "name": "field1", "type": "string", "initial": "" }
            ]),
        )
        .await;

        // An orphan (unreferenced) regex_rule with capture_vars — same shape
        // `try_convert_status_panel` produces — is globally effective per
        // `effective_regex_rules`.
        let mut rule = Definition::new("regex_rule", "status", "");
        rule.meta = serde_json::json!({
            "pattern": "<mood>(\\w+)</mood>",
            "replacement": "$1",
            "capture_vars": ["field1"]
        });
        storage.create_definition(&rule).await.unwrap();

        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        session.current_state = serde_json::json!({ "hp": 100, "field1": "" });
        storage.create_session(&session).await.unwrap();

        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: Arc::new(Mutex::new(None)),
            reply: "<mood>calm</mood> You take a hit. <state_update action=\"SUB\" key=\"hp\" value=\"5\"/>".into(),
        });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let assistant = msgs.iter().find(|m| m.role == Role::Assistant).unwrap();
        assert_eq!(assistant.snapshot_state["hp"], 95); // <state_update> tag still folds in
        assert_eq!(assistant.snapshot_state["field1"], "calm"); // regex capture folds in too
    }

    #[tokio::test]
    async fn explicit_state_update_tag_wins_over_panel_capture_on_key_collision() {
        // If a converted panel's capture variable shares a name with one the
        // model also sets explicitly via <state_update>, the explicit tag —
        // the model's deliberate instruction — must win, not whichever update
        // happens to be appended last.
        let storage = Arc::new(temp_storage().await);
        let t = crate::models::template::Template::new("T");
        storage.create_template(&t).await.unwrap();
        add_variables_brick(
            storage.as_ref(),
            &t.id,
            serde_json::json!([ { "name": "mood", "type": "string", "initial": "" } ]),
        )
        .await;

        let mut rule = Definition::new("regex_rule", "status", "");
        rule.meta = serde_json::json!({
            "pattern": "<mood>(\\w+)</mood>",
            "replacement": "$1",
            "capture_vars": ["mood"]
        });
        storage.create_definition(&rule).await.unwrap();

        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        session.current_state = serde_json::json!({ "mood": "" });
        storage.create_session(&session).await.unwrap();

        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: Arc::new(Mutex::new(None)),
            reply: "<mood>calm</mood> <state_update action=\"SET\" key=\"mood\" value=\"furious\"/>".into(),
        });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let assistant = msgs.iter().find(|m| m.role == Role::Assistant).unwrap();
        assert_eq!(assistant.snapshot_state["mood"], "furious");
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
            kind: "background".into(),
            hash: None,
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
        // Generate a long string of user/assistant history (without summaries)
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
        // The window is set to a very small size → triggers cropping
        storage.set_setting("context.window", &serde_json::json!(20)).await.unwrap();

        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "newest".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        let joined: String = req.messages.iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("|");
        assert!(joined.contains("newest"), "The last entry for this user must be retained");
        assert!(!joined.contains("turn-0"), "The oldest entry should be removed");
    }

    #[tokio::test]
    async fn assembly_uses_applicable_summary_and_truncates_history() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("s");
        storage.create_session(&session).await.unwrap();

        // Linear history: u1 -> a1 -> u2 -> a2 (where a2 is the active leaf)
        let u1 = Message::new(&session.id, None, Role::User, "u1");
        storage.create_message(&u1).await.unwrap();
        let a1 = Message::new(&session.id, Some(u1.id.clone()), Role::Assistant, "a1");
        storage.create_message(&a1).await.unwrap();
        let u2 = Message::new(&session.id, Some(a1.id.clone()), Role::User, "u2");
        storage.create_message(&u2).await.unwrap();
        let a2 = Message::new(&session.id, Some(u2.id.clone()), Role::Assistant, "a2");
        storage.create_message(&a2).await.unwrap();
        storage.set_session_active_leaf(&session.id, Some(&a2.id)).await.unwrap();

        // Summary extends to a1 (cutoff = a1): u1/a1 should not enter the context, and the summary should be carried over.
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
        assert!(!joined.contains("u1"), "History prior to the cutoff should not be included in context: {joined}");
        assert!(!joined.contains("a1"), "History prior to the cutoff should not be included in context: {joined}");
        assert!(joined.contains("u2"), "History after the cutoff should be retained");
        assert!(joined.contains("u3"), "This user should be retained");
    }

    #[tokio::test]
    async fn regenerate_folds_state_from_the_parent_branch() {
        let storage = Arc::new(temp_storage().await);
        let t = crate::models::template::Template::new("T");
        storage.create_template(&t).await.unwrap();
        add_variables_brick(
            storage.as_ref(),
            &t.id,
            serde_json::json!([ {"name":"hp","type":"number","initial":100} ]),
        )
        .await;
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
