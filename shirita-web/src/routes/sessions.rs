use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use shirita_core::models::message::Message;
use shirita_core::models::prompt_node::OwnerKind;
use shirita_core::models::session::Session;
use shirita_core::state::{resolve_schema, schema_initials};

use crate::AppState;

/// Recreate `messages` under `new_session_id`, minting fresh ids and rewiring
/// parent links so the reply tree survives a copy.
async fn clone_messages(state: &AppState, messages: &[Message], new_session_id: &str) -> Result<(), StatusCode> {
    let idmap: HashMap<String, String> = messages
        .iter()
        .map(|m| (m.id.clone(), uuid::Uuid::new_v4().to_string()))
        .collect();
    for m in messages {
        let mut nm = m.clone();
        nm.id = idmap.get(&m.id).cloned().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        nm.session_id = new_session_id.to_string();
        nm.parent_id = m.parent_id.as_ref().and_then(|p| idmap.get(p).cloned());
        state.storage.create_message(&nm).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(())
}

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
    // 会话引用模板，不再深拷贝节点；组装时按 effective_nodes 解析（自有优先，否则引用模板）。
    session.template_id = body.template_id.clone();
    // 用声明变量的初值播种 current_state（seed 层；后续快照在其上演化）。
    let template_meta = match &session.template_id {
        Some(tid) => state.storage.get_template(tid).await.ok().flatten().map(|t| t.meta),
        None => None,
    };
    let schema = resolve_schema(template_meta.as_ref(), &session.override_config);
    session.current_state = Value::Object(schema_initials(&schema));
    state.storage.create_session(&session).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(session))
}

pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Session>, StatusCode> {
    state
        .storage
        .get_session(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
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

#[derive(Deserialize)]
pub struct ReorderSessions {
    /// Session ids top-to-bottom in the desired manual order.
    pub ids: Vec<String>,
}

pub async fn reorder_sessions(
    State(state): State<AppState>,
    Json(body): Json<ReorderSessions>,
) -> Result<StatusCode, StatusCode> {
    state.storage.reorder_sessions(&body.ids).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    state.storage.delete_session(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Copy a session (name + " copy"), its own node tree, and its message history.
pub async fn duplicate_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Session>, StatusCode> {
    let src = state.storage.get_session(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut dup = Session::new(format!("{} copy", src.name));
    dup.avatar = src.avatar.clone();
    dup.template_id = src.template_id.clone();
    dup.override_config = src.override_config.clone();
    dup.current_state = src.current_state.clone();
    dup.mounted_definitions = src.mounted_definitions.clone();
    state.storage.create_session(&dup).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // copy any session-owned (forked) node tree; no-op for reference-only sessions
    let _ = state.storage.copy_nodes(&OwnerKind::Session, &id, &OwnerKind::Session, &dup.id).await;
    let msgs = state.storage.list_messages(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    clone_messages(&state, &msgs, &dup.id).await?;
    Ok(Json(dup))
}

/// Export a session + its messages as re-importable JSON.
pub async fn export_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let session = state.storage.get_session(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let messages = state.storage.list_messages(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "version": 1, "kind": "shirita.session", "session": session, "messages": messages })))
}

#[derive(Deserialize)]
pub struct ImportSession {
    pub session: Session,
    #[serde(default)]
    pub messages: Vec<Message>,
}

/// Recreate a session (fresh ids) from previously exported JSON.
pub async fn import_session(
    State(state): State<AppState>,
    Json(body): Json<ImportSession>,
) -> Result<Json<Session>, StatusCode> {
    let mut s = Session::new(body.session.name.clone());
    s.avatar = body.session.avatar.clone();
    s.template_id = body.session.template_id.clone();
    s.override_config = body.session.override_config.clone();
    s.current_state = body.session.current_state.clone();
    s.mounted_definitions = body.session.mounted_definitions.clone();
    state.storage.create_session(&s).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    clone_messages(&state, &body.messages, &s.id).await?;
    Ok(Json(s))
}
