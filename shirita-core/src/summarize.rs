//! Rolling summary pipeline: self-check threshold → select waterline → construct request → aggregation provider → store summary in database.
//! Background “fire-and-forget” call (spawned by the web layer), idempotent and reentrant (see M6 spec §2).

use std::sync::Arc;

use futures::StreamExt;

use crate::model::{ChatMessage, ChatRequest, ModelProvider};
use crate::models::message::Role;
use crate::models::summary::Summary;
use crate::storage::Storage;
use crate::tokenizer::TokenCounter;

/// Built-in default summary instruction (can be overridden globally via the `summarize.instruction` setting).
pub const DEFAULT_INSTRUCTION: &str = "Summarize the prior conversation faithfully and concisely. \
Preserve facts, decisions, character state, world details and any unresolved threads. \
Write plain prose, third person, no preamble and no meta commentary.";

/// Selects the range to be collapsed `[start, end)` (path indices): start = after the previous waterline, end = `keep_recent`
/// positions before. Returns None if there is nothing to collapse.
pub fn fold_range(path_len: usize, prev_cutoff_idx: Option<usize>, keep_recent: usize) -> Option<(usize, usize)> {
    let start = prev_cutoff_idx.map(|i| i + 1).unwrap_or(0);
    let end = path_len.saturating_sub(keep_recent);
    if start < end {
        Some((start, end))
    } else {
        None
    }
}

async fn setting_usize(s: &dyn Storage, key: &str, default: usize) -> usize {
    s.get_setting(key).await.ok().flatten()
        .and_then(|v| v.as_u64()).map(|n| n as usize).unwrap_or(default)
}
async fn setting_f64(s: &dyn Storage, key: &str, default: f64) -> f64 {
    s.get_setting(key).await.ok().flatten()
        .and_then(|v| v.as_f64()).unwrap_or(default)
}
async fn setting_string(s: &dyn Storage, key: &str, default: &str) -> String {
    s.get_setting(key).await.ok().flatten()
        .and_then(|v| v.as_str().map(|x| x.to_string())).unwrap_or_else(|| default.to_string())
}
async fn setting_bool(s: &dyn Storage, key: &str, default: bool) -> bool {
    s.get_setting(key).await.ok().flatten().and_then(|v| v.as_bool()).unwrap_or(default)
}

/// Attempts to run a rolling summary in the background (idempotent and reentrant): returns silently if the threshold is not exceeded or there is no content to collapse;
/// Otherwise, feeds the “previous summary + original text to be collapsed” to the summary command, aggregates the provider's output, and writes it to `summaries`.
pub async fn run(
    storage: Arc<dyn Storage>,
    provider: Arc<dyn ModelProvider>,
    counter: Arc<dyn TokenCounter>,
    model: String,
    session_id: String,
) {
    let Ok(Some(session)) = storage.get_session(&session_id).await else { return };
    let Ok(all) = storage.list_messages(&session_id).await else { return };
    let path = crate::tree::active_path(&all, session.active_leaf_id.as_deref());
    if path.is_empty() {
        return;
    }

    // Opt-out: auto-summarize can be disabled in settings (default on).
    if !setting_bool(storage.as_ref(), "summarize.enabled", true).await {
        return;
    }

    // Previous summary: The cutoff is on the active path, specifically the one furthest to the back.
    let summaries = storage.list_summaries(&session_id).await.unwrap_or_default();
    let prev = summaries
        .iter()
        .filter_map(|s| path.iter().position(|m| m.id == s.cutoff_message_id).map(|i| (s, i)))
        .max_by_key(|(_, i)| *i);
    let prev_idx = prev.as_ref().map(|(_, i)| *i);
    let prev_content = prev.as_ref().map(|(s, _)| s.content.clone());

    let window = setting_usize(storage.as_ref(), "context.window", crate::budget::DEFAULT_CONTEXT_WINDOW).await;
    let threshold = setting_f64(storage.as_ref(), "context.threshold", crate::budget::DEFAULT_OVER_THRESHOLD).await;
    let keep_recent = setting_usize(storage.as_ref(), "context.keep_recent", crate::budget::DEFAULT_KEEP_RECENT).await;

    // Self-check: whether the tokens from the unfolded history (visible after the cutoff) and the previous summary have crossed the trigger threshold.
    let start_visible = prev_idx.map(|i| i + 1).unwrap_or(0);
    let mut hist_tokens = prev_content.as_deref().map(|c| counter.count(c)).unwrap_or(0);
    for m in &path[start_visible..] {
        if !m.is_hidden {
            hist_tokens += counter.count(&m.raw_content);
        }
    }
    if !crate::budget::over_threshold(hist_tokens, window, threshold) {
        return;
    }

    let Some((s, e)) = fold_range(path.len(), prev_idx, keep_recent) else { return };
    let new_cutoff = path[e - 1].id.clone();

    let mut body = String::new();
    if let Some(pc) = &prev_content {
        body.push_str("[Previous summary]\n");
        body.push_str(pc);
        body.push_str("\n\n");
    }
    for m in &path[s..e] {
        if m.is_hidden {
            continue;
        }
        body.push_str(m.role.as_str());
        body.push_str(": ");
        body.push_str(&m.raw_content);
        body.push('\n');
    }

    let instruction = setting_string(storage.as_ref(), "summarize.instruction", DEFAULT_INSTRUCTION).await;
    let max_tokens = storage
        .get_setting("provider_max_tokens")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        // 0 is the UI's "Unlimited" sentinel → no output cap (provider default).
        .filter(|&n| n > 0);
    let req = ChatRequest {
        model,
        messages: vec![
            ChatMessage { role: Role::System, content: instruction, ..Default::default() },
            ChatMessage { role: Role::User, content: body, ..Default::default() },
        ],
        summary: None,
        max_tokens,
    };

    // Aggregate call (non-streaming semantics: collects all deltas).
    let mut stream = match provider.stream_chat(req).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, %session_id, "summary provider request failed");
            return;
        }
    };
    let mut full = String::new();
    while let Some(item) = stream.next().await {
        match item {
            Ok(d) => full.push_str(&d),
            Err(e) => {
                tracing::warn!(error = %e, %session_id, "summary stream error");
                return;
            }
        }
    }
    let full = full.trim();
    if full.is_empty() {
        tracing::warn!(%session_id, "summary provider returned empty output");
        return;
    }

    let summary = Summary::new(&session_id, &new_cutoff, full);
    if let Err(e) = storage.create_summary(&summary).await {
        tracing::warn!(error = %e, "summary persist failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use futures::stream::BoxStream;
    use serde_json::json;
    use crate::models::message::Message;
    use crate::models::session::Session;
    use crate::storage::sqlite::SqliteStorage;
    use crate::tokenizer::tiktoken::TiktokenCounter;

    async fn temp_storage() -> SqliteStorage {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sum.db");
        std::mem::forget(dir);
        let s = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
        s.run_migrations().await.unwrap();
        s
    }

    struct FixedProvider(String);
    #[async_trait::async_trait]
    impl ModelProvider for FixedProvider {
        async fn stream_chat(&self, _req: ChatRequest) -> crate::Result<BoxStream<'static, crate::Result<String>>> {
            let r = self.0.clone();
            Ok(Box::pin(futures::stream::iter(vec![Ok(r)])))
        }
    }

    async fn long_session(storage: &SqliteStorage, turns: usize) -> (Session, String) {
        let session = Session::new("s");
        storage.create_session(&session).await.unwrap();
        let mut parent: Option<String> = None;
        let mut leaf = String::new();
        for i in 0..turns {
            let role = if i % 2 == 0 { Role::User } else { Role::Assistant };
            let m = Message::new(&session.id, parent.clone(), role, format!("turn-{i}-{}", "x".repeat(40)));
            storage.create_message(&m).await.unwrap();
            parent = Some(m.id.clone());
            leaf = m.id.clone();
        }
        storage.set_session_active_leaf(&session.id, Some(&leaf)).await.unwrap();
        (session, leaf)
    }

    #[tokio::test]
    async fn run_folds_history_when_over_threshold() {
        let storage = Arc::new(temp_storage().await);
        let (session, leaf) = long_session(&storage, 14).await;
        storage.set_setting("context.window", &json!(50)).await.unwrap(); // small window → exceed threshold

        let provider: Arc<dyn ModelProvider> = Arc::new(FixedProvider("SUMMARY-TEXT".into()));
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        run(storage.clone(), provider, counter, "m".into(), session.id.clone()).await;

        let sums = storage.list_summaries(&session.id).await.unwrap();
        assert_eq!(sums.len(), 1);
        assert_eq!(sums[0].content, "SUMMARY-TEXT");
        // len=14, keep_recent=10(默认) → end=4 → cutoff = path[3]
        let all = storage.list_messages(&session.id).await.unwrap();
        let path = crate::tree::active_path(&all, Some(&leaf));
        assert_eq!(sums[0].cutoff_message_id, path[3].id);
    }

    #[tokio::test]
    async fn run_noop_when_under_threshold() {
        let storage = Arc::new(temp_storage().await);
        let (session, _leaf) = long_session(&storage, 4).await; // short history
        let provider: Arc<dyn ModelProvider> = Arc::new(FixedProvider("X".into()));
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        run(storage.clone(), provider, counter, "m".into(), session.id.clone()).await;
        assert!(storage.list_summaries(&session.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn run_skipped_when_disabled() {
        let storage = Arc::new(temp_storage().await);
        let (session, _leaf) = long_session(&storage, 14).await;
        storage.set_setting("context.window", &json!(50)).await.unwrap(); // would normally fold
        storage.set_setting("summarize.enabled", &json!(false)).await.unwrap();
        let provider: Arc<dyn ModelProvider> = Arc::new(FixedProvider("X".into()));
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        run(storage.clone(), provider, counter, "m".into(), session.id.clone()).await;
        assert!(storage.list_summaries(&session.id).await.unwrap().is_empty());
    }

    #[test]
    fn fold_range_first_fold_keeps_recent() {
        assert_eq!(fold_range(20, None, 10), Some((0, 10)));
    }

    #[test]
    fn fold_range_advances_from_prev_cutoff() {
        assert_eq!(fold_range(20, Some(4), 10), Some((5, 10)));
    }

    #[test]
    fn fold_range_none_when_nothing_new_to_fold() {
        assert_eq!(fold_range(12, Some(4), 10), None); // start 5 >= end 2
        assert_eq!(fold_range(8, None, 10), None); // end saturates to 0
    }
}
