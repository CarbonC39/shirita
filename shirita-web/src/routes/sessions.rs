use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use shirita_core::models::message::Message;
use shirita_core::models::session::Session;

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateSession {
    pub name: String,
}

pub async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSession>,
) -> Result<Json<Session>, StatusCode> {
    let session = Session::new(body.name);
    state
        .storage
        .create_session(&session)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(session))
}

pub async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<Session>>, StatusCode> {
    let sessions = state
        .storage
        .list_sessions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(sessions))
}

pub async fn list_messages(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<Message>>, StatusCode> {
    let msgs = state
        .storage
        .list_messages(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(msgs))
}
