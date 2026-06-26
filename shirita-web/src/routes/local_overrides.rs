use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

/// Existence check: If the session does not exist, return a 404 (maintain existing behavior).
async fn ensure_session(state: &AppState, session_id: &str) -> Result<(), StatusCode> {
    match state.storage.get_session(session_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(_) => Ok(()),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Write/replace the field-level patch for `def_id`。
pub async fn set_local_definition(
    State(state): State<AppState>,
    Path((session_id, def_id)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<StatusCode, StatusCode> {
    ensure_session(&state, &session_id).await?;
    state
        .storage
        .set_local_definition(&session_id, &def_id, &patch)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

/// Revert: drop the local patch for `def_id`。
pub async fn clear_local_definition(
    State(state): State<AppState>,
    Path((session_id, def_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    ensure_session(&state, &session_id).await?;
    state
        .storage
        .clear_local_definition(&session_id, &def_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

/// Ensure the session owns a node tree: if it has none yet, deep-copy its
/// template's nodes into `owner_kind=session`. Idempotent.
pub async fn materialize_nodes(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let session = state
        .storage
        .get_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    // Copy the template tree into the session on first call only. The storage
    // method makes the "is it empty?" check and the copy share one transaction,
    // so two concurrent calls can't both materialize (the old TOCTOU).
    if let Some(tid) = session.template_id.as_deref() {
        state
            .storage
            .materialize_session_nodes(&session_id, tid)
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

    // Fold the merged def into the global row and clear the local patch in one
    // transaction, so a promote can't half-apply (def updated but patch left).
    state
        .storage
        .promote_local_definition(&session_id, &def_id, &def)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}
