use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use shirita_core::state::{apply_updates, effective_state, resolve_schema_with_packs, Action, Update};
use shirita_core::tree::active_path;
use shirita_core::{Message, Role};

use crate::AppState;

/// 返回当前激活分支的有效变量状态 + schema（合并在服务端完成，单一真相）。
pub async fn get_state(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let session = state.storage.get_session(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let template_meta = match &session.template_id {
        Some(tid) => state.storage.get_template(tid).await.ok().flatten().map(|t| t.meta),
        None => None,
    };
    let mut pack_metas = Vec::new();
    for pid in &session.mounted_packs {
        if let Ok(Some(p)) = state.storage.get_pack(pid).await {
            pack_metas.push(p.meta);
        }
    }
    let schema = resolve_schema_with_packs(template_meta.as_ref(), &pack_metas, &session.override_config);
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
    let template_meta = match &session.template_id {
        Some(tid) => state.storage.get_template(tid).await.ok().flatten().map(|t| t.meta),
        None => None,
    };
    let mut pack_metas = Vec::new();
    for pid in &session.mounted_packs {
        if let Ok(Some(p)) = state.storage.get_pack(pid).await {
            pack_metas.push(p.meta);
        }
    }
    let schema = resolve_schema_with_packs(template_meta.as_ref(), &pack_metas, &session.override_config);

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
    state.storage.create_message(&node).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state.storage.set_session_active_leaf(&id, Some(&node.id)).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let values = effective_state(&schema, &session.current_state, &new_snapshot);
    Ok(Json(json!({ "values": values })))
}

/// 替换会话本地变量声明（存于 override_config.local_variables）。
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
    // 原子整列替换 override_config.local_variables（无读改写竞争）。
    state.storage.set_local_variables(&id, &body.variables).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}
