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
}

/// 进程级"正在总结的 session"集合，防 fire-and-forget 并发重复（语义等价 spec §2 的 per-session 互斥）。
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

/// 若该 session 未在总结，spawn 一个后台总结任务（不阻塞 SSE）。
fn spawn_summary(state: &AppState, session_id: String) {
    if !try_claim(&session_id) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        // 与生成同源：从 settings 解析实际 provider/model（未配置则 env 兜底）。
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
    // 运行期解析 provider/model：settings 配置胜出，未配置回退 env。
    let (provider, model) = resolve_provider(&state).await;
    let events = send_message(
        state.storage.clone(),
        provider,
        state.token_counter.clone(),
        model,
        session_id,
        body.text,
    );
    // A newer generation for the same session aborts this one (no racing writes).
    let (events, handle) = futures::stream::abortable(events);
    state.generations.replace(&reg_id, handle);

    // 回复流结束（Done）后后台触发滚动总结，绝不阻塞 SSE 主流。
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
        assert!(!try_claim(key)); // 已占用
        release(key);
        assert!(try_claim(key)); // 释放后可再占
        release(key);
    }
}
