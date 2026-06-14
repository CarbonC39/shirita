use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use shirita_core::models::definition::Definition;

use crate::AppState;

#[derive(Deserialize)]
pub struct DefinitionBody {
    pub r#type: String,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub meta: Value,
}

fn build(id: String, body: DefinitionBody) -> Definition {
    let meta = if body.meta.is_null() {
        serde_json::json!({})
    } else {
        body.meta
    };
    Definition {
        id,
        def_type: body.r#type,
        name: body.name,
        content: body.content,
        meta,
    }
}

/// type 必须是保留类型，或已注册的容器类型。
async fn validate_type(state: &AppState, t: &str) -> Result<(), StatusCode> {
    if shirita_core::is_reserved(t) {
        return Ok(());
    }
    let containers = state
        .storage
        .list_container_types()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if containers.iter().any(|c| c.id == t) {
        Ok(())
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub r#type: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Definition>>, StatusCode> {
    let mut defs = state
        .storage
        .list_definitions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(t) = q.r#type {
        defs.retain(|d| d.def_type.as_str() == t);
    }
    Ok(Json(defs))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<DefinitionBody>,
) -> Result<Json<Definition>, StatusCode> {
    validate_type(&state, &body.r#type).await?;
    let def = build(uuid::Uuid::new_v4().to_string(), body);
    state
        .storage
        .create_definition(&def)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(def))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Definition>, StatusCode> {
    match state
        .storage
        .get_definition(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        Some(d) => Ok(Json(d)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<DefinitionBody>,
) -> Result<Json<Definition>, StatusCode> {
    validate_type(&state, &body.r#type).await?;
    let def = build(id.clone(), body);
    if state
        .storage
        .get_definition(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_none()
    {
        return Err(StatusCode::NOT_FOUND);
    }
    state
        .storage
        .update_definition(&def)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(def))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    state
        .storage
        .delete_definition(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}
