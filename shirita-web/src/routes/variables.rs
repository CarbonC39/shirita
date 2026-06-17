use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use shirita_core::state::{effective_state, resolve_schema};
use shirita_core::tree::active_path;

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
    let schema = resolve_schema(template_meta.as_ref(), &session.override_config);
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
