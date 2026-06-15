use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use shirita_core::OwnerKind;

use crate::AppState;

fn ensure_obj(v: &mut Value) -> &mut serde_json::Map<String, Value> {
    if !v.is_object() {
        *v = json!({});
    }
    v.as_object_mut().unwrap()
}

/// Read the session's override_config and mutate `local_definitions` via `f`.
async fn with_local_defs<F>(state: &AppState, session_id: &str, f: F) -> Result<(), StatusCode>
where
    F: FnOnce(&mut serde_json::Map<String, Value>),
{
    let session = state
        .storage
        .get_session(session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut cfg = session.override_config.clone();
    let cfg_obj = ensure_obj(&mut cfg);
    let mut locals = cfg_obj.get("local_definitions").cloned().unwrap_or_else(|| json!({}));
    f(ensure_obj(&mut locals));
    cfg_obj.insert("local_definitions".into(), locals);
    state
        .storage
        .update_session_override_config(session_id, &cfg)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Write/replace the field-level patch for `def_id` (only the changed fields).
pub async fn set_local_definition(
    State(state): State<AppState>,
    Path((session_id, def_id)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<StatusCode, StatusCode> {
    with_local_defs(&state, &session_id, |locals| {
        locals.insert(def_id.clone(), patch);
    })
    .await?;
    Ok(StatusCode::OK)
}

/// Revert: drop the local patch for `def_id`.
pub async fn clear_local_definition(
    State(state): State<AppState>,
    Path((session_id, def_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    with_local_defs(&state, &session_id, |locals| {
        locals.remove(&def_id);
    })
    .await?;
    Ok(StatusCode::OK)
}

/// Ensure the session owns a node tree: if it has none yet, deep-copy its
/// template's nodes into `owner_kind=session`. Idempotent.
pub async fn materialize_nodes(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let existing = state
        .storage
        .list_nodes(&OwnerKind::Session, &session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !existing.is_empty() {
        return Ok(StatusCode::OK); // already materialized
    }
    let session = state
        .storage
        .get_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if let Some(tid) = session.template_id.as_deref() {
        let _ = state
            .storage
            .copy_nodes(&OwnerKind::Template, tid, &OwnerKind::Session, &session_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(StatusCode::OK)
}

/// Sync to global: fold the patch into the global definition, then clear it.
pub async fn promote_local_definition(
    State(state): State<AppState>,
    Path((session_id, def_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    let session = state
        .storage
        .get_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let patch = session
        .override_config
        .get("local_definitions")
        .and_then(|l| l.get(&def_id))
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut def = state
        .storage
        .get_definition(&def_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(c) = patch.get("content").and_then(|v| v.as_str()) {
        def.content = c.to_string();
    }
    if let Some(n) = patch.get("name").and_then(|v| v.as_str()) {
        def.name = n.to_string();
    }
    // trigger / scan live under the definition's meta object
    if !def.meta.is_object() {
        def.meta = json!({});
    }
    let meta = def.meta.as_object_mut().unwrap();
    if let Some(t) = patch.get("trigger") {
        meta.insert("trigger".into(), t.clone());
    }
    if let Some(s) = patch.get("scan") {
        meta.insert("scan".into(), s.clone());
    }

    state
        .storage
        .update_definition(&def)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    with_local_defs(&state, &session_id, |locals| {
        locals.remove(&def_id);
    })
    .await?;
    Ok(StatusCode::OK)
}
