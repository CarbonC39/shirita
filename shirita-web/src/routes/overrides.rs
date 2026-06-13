use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::AppState;

pub async fn set_override(State(state): State<AppState>, Path((session_id, def_id)): Path<(String, String)>, Json(body): Json<Value>) -> Result<StatusCode, StatusCode> {
    let session = state.storage.get_session(&session_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let content = body.get("content").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let mut overrides = session.override_config.get("local_definitions").cloned().unwrap_or_else(|| serde_json::json!({}));
    overrides[&def_id] = Value::String(content.to_string());
    let mut config = session.override_config.clone();
    config["local_definitions"] = overrides;
    state.storage.update_session_override_config(&session_id, &config).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn reset_override(State(state): State<AppState>, Path((session_id, def_id)): Path<(String, String)>) -> Result<StatusCode, StatusCode> {
    let session = state.storage.get_session(&session_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let mut overrides = session.override_config.get("local_definitions").cloned().unwrap_or_else(|| serde_json::json!({}));
    overrides.as_object_mut().map(|o| o.remove(&def_id));
    let mut config = session.override_config.clone();
    config["local_definitions"] = overrides;
    state.storage.update_session_override_config(&session_id, &config).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn promote_override(State(state): State<AppState>, Path((session_id, def_id)): Path<(String, String)>) -> Result<StatusCode, StatusCode> {
    let session = state.storage.get_session(&session_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let overrides = session.override_config.get("local_definitions").cloned().unwrap_or_else(|| serde_json::json!({}));
    let new_content = overrides.get(&def_id).and_then(|v| v.as_str()).ok_or(StatusCode::NOT_FOUND)?;
    let mut def = state.storage.get_definition(&def_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    def.content = new_content.to_string();
    state.storage.update_definition(&def).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut overrides_obj = overrides.as_object().cloned().unwrap_or_default();
    overrides_obj.remove(&def_id);
    let mut config = session.override_config.clone();
    config["local_definitions"] = serde_json::to_value(overrides_obj).unwrap_or_default();
    state.storage.update_session_override_config(&session_id, &config).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn list_overrides(State(state): State<AppState>, Path(session_id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let session = state.storage.get_session(&session_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let overrides = session.override_config.get("local_definitions").cloned().unwrap_or_else(|| serde_json::json!({}));
    Ok(Json(overrides))
}
