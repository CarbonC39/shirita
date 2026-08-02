use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::{Mutex, OnceLock};

use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;

use shirita_core::{regenerate, send_message, summarize, SendEvent, StopHandle};

use crate::{resolve_provider, AppState};

#[derive(Deserialize)]
pub struct SendBody {
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<String>,
}

/// A process-level collection of “sessions currently being summarized” (to prevent “fire-and-forget” concurrency and duplication).
fn summarizing() -> &'static Mutex<HashSet<String>> {
    static S: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}
fn try_claim(session_id: &str) -> bool {
    let mut g = summarizing().lock().unwrap();
    if g.contains(session_id) {
        false
    } else {
        g.insert(session_id.to_string());
        true
    }
}
fn release(session_id: &str) {
    summarizing().lock().unwrap().remove(session_id);
}

/// If this session has not been summarized, spawn a background summary task (without blocking SSE).
fn spawn_summary(state: &AppState, session_id: String) {
    if !try_claim(&session_id) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        // Same as generation: Parse the actual provider/model from settings (if not configured, fall back to env).
        let (provider, model) = resolve_provider(&state).await;
        summarize::run(state.storage.clone(), provider, state.token_counter.clone(), model, session_id.clone()).await;
        release(&session_id);
    });
}

pub async fn send(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<SendBody>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let reg_id = session_id.clone();
    // Runtime resolution: provider/model—the settings configuration takes precedence; if not configured, fall back to env.
    let (provider, model) = resolve_provider(&state).await;
    // The cooperative stop token lets the Stop button / abort route end this
    // stream gracefully (persisting partial text) instead of a hard abort.
    let (stop_handle, stop_token) = StopHandle::new();
    let events = send_message(
        state.storage.clone(),
        provider,
        state.token_counter.clone(),
        model,
        session_id,
        body.text,
        state.config.assets_dir.clone(),
        body.attachments,
        stop_token,
    );
    // A newer generation for the same session aborts this one (no racing writes).
    let (events, handle) = futures::stream::abortable(events);
    let gen_id = state.generations.replace(&reg_id, handle, stop_handle);

    // After the reply stream ends (Done), the background process triggers a scroll summary, without ever blocking the SSE main thread.
    let state_for_summary = state.clone();
    let sid_for_summary = reg_id.clone();
    let sse = events.map(move |ev| {
        // Terminal events end this generation — de-register our slot so the
        // registry doesn't accumulate one entry per session forever.
        match &ev {
            SendEvent::Done { .. } => {
                spawn_summary(&state_for_summary, sid_for_summary.clone());
                state_for_summary.generations.finish(&sid_for_summary, gen_id);
            }
            SendEvent::Stopped { .. } => state_for_summary.generations.finish(&sid_for_summary, gen_id),
            SendEvent::Error(_) => state_for_summary.generations.finish(&sid_for_summary, gen_id),
            _ => {}
        }
        let payload = match ev {
            SendEvent::Delta(text) => json!({ "type": "delta", "text": text }),
            SendEvent::Activity { round, message } => json!({ "type": "activity", "round": round, "message": message }),
            SendEvent::RunStart { run_id } => json!({ "type": "run_start", "run_id": run_id }),
            SendEvent::ToolStart { call_id, name } => json!({ "type": "tool_start", "call_id": call_id, "name": name }),
            SendEvent::ToolResult { call_id, name, status } => json!({ "type": "tool_result", "call_id": call_id, "name": name, "status": status }),
            SendEvent::WorkspaceChanged { revision } => json!({ "type": "workspace_mutation", "revision": revision }),
            SendEvent::Finish { run_id } => json!({ "type": "finish", "run_id": run_id }),
            SendEvent::Status(message) => json!({ "type": "status", "message": message }),
            SendEvent::Usage { input_tokens, output_tokens } => json!({ "type": "usage", "input_tokens": input_tokens, "output_tokens": output_tokens }),
            SendEvent::Done { message_id } => json!({ "type": "done", "message_id": message_id }),
            SendEvent::Stopped { message_id } => json!({ "type": "stopped", "message_id": message_id }),
            SendEvent::Error(message) => json!({ "type": "error", "message": message }),
        };
        Ok(Event::default().data(payload.to_string()))
    });

    Sse::new(sse)
}

pub async fn regenerate_message(
    State(state): State<AppState>,
    Path((session_id, msg_id)): Path<(String, String)>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let reg_id = session_id.clone();
    let (provider, model) = resolve_provider(&state).await;
    let (stop_handle, stop_token) = StopHandle::new();
    let events = regenerate(
        state.storage.clone(),
        provider,
        state.token_counter.clone(),
        model,
        session_id,
        msg_id,
        state.config.assets_dir.clone(),
        stop_token,
    );
    let (events, handle) = futures::stream::abortable(events);
    let gen_id = state.generations.replace(&reg_id, handle, stop_handle);
    let state_for_summary = state.clone();
    let sid_for_summary = reg_id.clone();
    let sse = events.map(move |ev| {
        match &ev {
            SendEvent::Done { .. } => {
                spawn_summary(&state_for_summary, sid_for_summary.clone());
                state_for_summary.generations.finish(&sid_for_summary, gen_id);
            }
            SendEvent::Stopped { .. } => state_for_summary.generations.finish(&sid_for_summary, gen_id),
            SendEvent::Error(_) => state_for_summary.generations.finish(&sid_for_summary, gen_id),
            _ => {}
        }
        let payload = match ev {
            SendEvent::Delta(text) => json!({ "type": "delta", "text": text }),
            SendEvent::Activity { round, message } => json!({ "type": "activity", "round": round, "message": message }),
            SendEvent::RunStart { run_id } => json!({ "type": "run_start", "run_id": run_id }),
            SendEvent::ToolStart { call_id, name } => json!({ "type": "tool_start", "call_id": call_id, "name": name }),
            SendEvent::ToolResult { call_id, name, status } => json!({ "type": "tool_result", "call_id": call_id, "name": name, "status": status }),
            SendEvent::WorkspaceChanged { revision } => json!({ "type": "workspace_mutation", "revision": revision }),
            SendEvent::Finish { run_id } => json!({ "type": "finish", "run_id": run_id }),
            SendEvent::Status(message) => json!({ "type": "status", "message": message }),
            SendEvent::Usage { input_tokens, output_tokens } => json!({ "type": "usage", "input_tokens": input_tokens, "output_tokens": output_tokens }),
            SendEvent::Done { message_id } => json!({ "type": "done", "message_id": message_id }),
            SendEvent::Stopped { message_id } => json!({ "type": "stopped", "message_id": message_id }),
            SendEvent::Error(message) => json!({ "type": "error", "message": message }),
        };
        Ok(Event::default().data(payload.to_string()))
    });
    Sse::new(sse)
}

/// Cooperatively stop the in-flight generation for `session_id` (Stop button
/// or navigate-away). The stream persists whatever was generated so far, then
/// ends. Idempotent — calling it when nothing is in flight is a no-op.
pub async fn abort_message(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Json<serde_json::Value> {
    let stopped = state.generations.stop(&session_id);
    Json(json!({ "stopped": stopped }))
}

#[cfg(test)]
mod tests {
    use super::{release, try_claim};

    #[test]
    fn try_claim_is_exclusive_until_release() {
        let key = "claim-test-unique-key";
        assert!(try_claim(key));
        assert!(!try_claim(key)); // in use
        release(key);
        assert!(try_claim(key)); // can be reused after release
        release(key);
    }
}
