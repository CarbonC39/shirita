use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;

use shirita_core::{Definition, OwnerKind};

use crate::AppState;

/// 文件名安全化：仅保留字母数字/`-`/`_`，其余转 `_`。
fn safe_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if s.is_empty() { "export".into() } else { s }
}

/// GET /api/definitions/{id}/export — 单定义原创 JSON（附下载头）。
pub async fn export_definition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let def = state
        .storage
        .get_definition(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let v = shirita_core::export_definition(&def);
    let cd = format!("attachment; filename=\"{}.json\"", safe_filename(&def.name));
    Ok(([(header::CONTENT_DISPOSITION, cd)], Json(v)))
}

/// GET /api/templates/{id}/export — 模板「启用部分」原创 JSON（附下载头）。
pub async fn export_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let tmpl = state
        .storage
        .get_template(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let nodes = state
        .storage
        .list_nodes(&OwnerKind::Template, &id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let all = state.storage.list_definitions().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let defs: HashMap<String, Definition> = all.into_iter().map(|d| (d.id.clone(), d)).collect();
    let v = shirita_core::export_template(&tmpl, &nodes, &defs);
    let cd = format!("attachment; filename=\"{}.json\"", safe_filename(&tmpl.name));
    Ok(([(header::CONTENT_DISPOSITION, cd)], Json(v)))
}
