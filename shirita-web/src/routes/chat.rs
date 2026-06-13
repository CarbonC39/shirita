use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;

use shirita_core::{send_message, SendEvent};

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
    let events = send_message(
        state.storage.clone(),
        state.provider.clone(),
        state.token_counter.clone(),
        state.model.clone(),
        session_id,
        body.text,
    );

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
