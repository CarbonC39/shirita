use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::{Mutex, OnceLock};

use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;

use shirita_core::{regenerate, send_message, summarize, SendEvent};

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
    let events = send_message(
        state.storage.clone(),
        provider,
        state.token_counter.clone(),
        model,
        session_id,
        body.text,
        state.config.assets_dir.clone(),
        body.attachments,
    );
    // A newer generation for the same session aborts this one (no racing writes).
    let (events, handle) = futures::stream::abortable(events);
    state.generations.replace(&reg_id, handle);

    // After the reply stream ends (Done), the background process triggers a scroll summary, without ever blocking the SSE main thread.
    let state_for_summary = state.clone();
    let sid_for_summary = reg_id.clone();
    let sse = events.map(move |ev| {
        if matches!(ev, SendEvent::Done { .. }) {
            spawn_summary(&state_for_summary, sid_for_summary.clone());
        }
        let payload = match ev {
            SendEvent::Delta(text) => json!({ "type": "delta", "text": text }),
            SendEvent::Done { message_id } => json!({ "type": "done", "message_id": message_id }),
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
    let events = regenerate(
        state.storage.clone(),
        provider,
        state.token_counter.clone(),
        model,
        session_id,
        msg_id,
        state.config.assets_dir.clone(),
    );
    let (events, handle) = futures::stream::abortable(events);
    state.generations.replace(&reg_id, handle);
    let state_for_summary = state.clone();
    let sid_for_summary = reg_id.clone();
    let sse = events.map(move |ev| {
        if matches!(ev, SendEvent::Done { .. }) {
            spawn_summary(&state_for_summary, sid_for_summary.clone());
        }
        let payload = match ev {
            SendEvent::Delta(text) => json!({ "type": "delta", "text": text }),
            SendEvent::Done { message_id } => json!({ "type": "done", "message_id": message_id }),
            SendEvent::Error(message) => json!({ "type": "error", "message": message }),
        };
        Ok(Event::default().data(payload.to_string()))
    });
    Sse::new(sse)
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
