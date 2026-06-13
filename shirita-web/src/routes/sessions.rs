use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use shirita_core::models::message::Message;
use shirita_core::models::session::Session;
use shirita_core::OwnerKind;

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateSession {
    pub name: String,
    pub template_id: Option<String>,
}

pub async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSession>,
) -> Result<Json<Session>, StatusCode> {
    let mut session = Session::new(body.name);
    session.template_id = body.template_id.clone();
    state.storage.create_session(&session).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(tid) = body.template_id {
        if state.storage.get_template(&tid).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.is_some() {
            state.storage.copy_nodes(&OwnerKind::Template, &tid, &OwnerKind::Session, &session.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }
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

#[derive(Deserialize)]
pub struct SetMounts {
    pub definition_ids: Vec<String>,
}

pub async fn set_mounts(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<SetMounts>,
) -> Result<StatusCode, StatusCode> {
    if state
        .storage
        .get_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_none()
    {
        return Err(StatusCode::NOT_FOUND);
    }
    state
        .storage
        .set_mounted_definitions(&session_id, &body.definition_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}
