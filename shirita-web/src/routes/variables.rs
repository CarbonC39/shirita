use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use shirita_core::state::{apply_updates, effective_state, Action, Update};
use shirita_core::tree::active_path;
use shirita_core::{Message, Role};

use crate::AppState;

/// Returns the valid variable state + schema for the currently active branch (merged on the server, single source of truth).
pub async fn get_state(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let session = state.storage.get_session(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let schema = shirita_core::conversation::resolve_session_schema(state.storage.as_ref(), &session).await;
    let all = state.storage.list_messages(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let leaf = active_path(&all, session.active_leaf_id.as_deref())
        .last()
        .map(|m| m.snapshot_state.clone())
        .unwrap_or_else(|| json!({}));
    let values = effective_state(&schema, &session.current_state, &leaf);
    Ok(Json(json!({ "schema": schema, "values": values })))
}

#[derive(Deserialize)]
pub struct LocalVarsBody {
    pub variables: Value, // a JSON array of {name,type,initial}
}

#[derive(Deserialize)]
pub struct StateUpdateItem {
    pub action: String,
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Deserialize)]
pub struct StateUpdatesBody {
    pub updates: Vec<StateUpdateItem>,
}

/// Apply panel-driven variable diffs mid-conversation: fold them into a hidden
/// system state-carrier node on the active branch and advance the leaf to it.
/// Mirrors the M5 fold in conversation.rs, but triggered by a panel action.
pub async fn apply_state_updates(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<StateUpdatesBody>,
) -> Result<Json<Value>, StatusCode> {
    let session = state.storage.get_session(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Same schema GET …/state resolves: template + mounted packs + locals.
    let schema = shirita_core::conversation::resolve_session_schema(state.storage.as_ref(), &session).await;

    // Current branch state = the active leaf's folded snapshot.
    let all = state.storage.list_messages(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let leaf_snapshot = active_path(&all, session.active_leaf_id.as_deref())
        .last().map(|m| m.snapshot_state.clone()).unwrap_or_else(|| json!({}));
    let branch_state = effective_state(&schema, &session.current_state, &leaf_snapshot);

    // Build typed diffs; drop unknown ops here, undeclared/type-mismatched keys
    // are ignored inside apply_updates.
    let updates: Vec<Update> = body.updates.iter().filter_map(|u| {
        Action::parse(&u.action).map(|action| Update {
            action,
            key: u.key.clone(),
            value: u.value.clone(),
        })
    }).collect();
    let new_snapshot = apply_updates(&branch_state, &schema, &updates);

    // Anchor the change: a hidden, content-less system node, then advance the leaf.
    let mut node = Message::new(&id, session.active_leaf_id.clone(), Role::System, "");
    node.is_hidden = true;
    node.snapshot_state = new_snapshot.clone();
    // Insert the carrier node and advance the leaf onto it atomically, so a
    // failure can't leave a node the active branch never reaches.
    state.storage.create_message_and_advance_leaf(&node).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let values = effective_state(&schema, &session.current_state, &new_snapshot);
    Ok(Json(json!({ "values": values })))
}

/// Replaces the session local variable declarations (stored in `override_config.local_variables`).
pub async fn set_local_variables(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<LocalVarsBody>,
) -> Result<StatusCode, StatusCode> {
    if state.storage.get_session(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_none()
    {
        return Err(StatusCode::NOT_FOUND);
    }
    // Atomic block replacement: override_config.local_variables (no read-write contention).
    state.storage.set_local_variables(&id, &body.variables).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}
