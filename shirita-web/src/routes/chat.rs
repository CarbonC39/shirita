use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;

use shirita_core::{regenerate, send_message, SendEvent};

use crate::AppState;

#[derive(Deserialize)]
pub struct SendBody {
    pub text: String,
}

pub async fn send(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<SendBody>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let reg_id = session_id.clone();
    let events = send_message(
        state.storage.clone(),
        state.provider.clone(),
        state.token_counter.clone(),
        state.model.clone(),
        session_id,
        body.text,
    );
    // A newer generation for the same session aborts this one (no racing writes).
    let (events, handle) = futures::stream::abortable(events);
    state.generations.replace(&reg_id, handle);

    let sse = events.map(|ev| {
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
    let events = regenerate(
        state.storage.clone(),
        state.provider.clone(),
        state.token_counter.clone(),
        state.model.clone(),
        session_id,
        msg_id,
    );
    let (events, handle) = futures::stream::abortable(events);
    state.generations.replace(&reg_id, handle);
    let sse = events.map(|ev| {
        let payload = match ev {
            SendEvent::Delta(text) => json!({ "type": "delta", "text": text }),
            SendEvent::Done { message_id } => json!({ "type": "done", "message_id": message_id }),
            SendEvent::Error(message) => json!({ "type": "error", "message": message }),
        };
        Ok(Event::default().data(payload.to_string()))
    });
    Sse::new(sse)
}
