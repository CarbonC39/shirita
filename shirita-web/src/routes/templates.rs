use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use shirita_core::{OwnerKind, Template};

use crate::AppState;

#[derive(Deserialize)]
pub struct TemplateBody { pub name: String, #[serde(default)] pub meta: Value }

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Template>>, StatusCode> {
    state.storage.list_templates().await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn create(State(state): State<AppState>, Json(body): Json<TemplateBody>) -> Result<Json<Template>, StatusCode> {
    let mut t = Template::new(body.name);
    if !body.meta.is_null() { t.meta = body.meta; }
    state.storage.create_template(&t).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(t))
}

pub async fn get(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Template>, StatusCode> {
    state.storage.get_template(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.map(Json).ok_or(StatusCode::NOT_FOUND)
}

pub async fn update(State(state): State<AppState>, Path(id): Path<String>, Json(body): Json<TemplateBody>) -> Result<Json<Template>, StatusCode> {
    let mut t = state.storage.get_template(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    t.name = body.name;
    if !body.meta.is_null() { t.meta = body.meta; }
    t.updated_at = chrono::Utc::now().to_rfc3339();
    state.storage.update_template(&t).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(t))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, StatusCode> {
    state.storage.delete_template(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn duplicate(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Template>, StatusCode> {
    let original = state.storage.get_template(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let mut copy = Template::new(format!("{} (copy)", original.name));
    copy.meta = original.meta;
    state.storage.create_template(&copy).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state.storage.copy_nodes(&OwnerKind::Template, &id, &OwnerKind::Template, &copy.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(copy))
}
