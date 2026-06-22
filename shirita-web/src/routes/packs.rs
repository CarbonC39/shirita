use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use shirita_core::{Definition, OwnerKind, Pack, PackIdentity};

use crate::AppState;

#[derive(Deserialize)]
pub struct PackBody {
    pub name: String,
    #[serde(default)]
    pub identity: PackIdentity,
    #[serde(default)]
    pub meta: Value,
}

#[derive(Deserialize)]
pub struct DeleteQuery {
    #[serde(default)]
    pub delete_orphans: bool,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Pack>>, StatusCode> {
    state.storage.list_packs().await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn create(State(state): State<AppState>, Json(body): Json<PackBody>) -> Result<Json<Pack>, StatusCode> {
    let mut p = Pack::new(body.name);
    p.identity = body.identity;
    if !body.meta.is_null() {
        p.meta = body.meta;
    }
    state.storage.create_pack(&p).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(p))
}

pub async fn get(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Pack>, StatusCode> {
    state.storage.get_pack(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.map(Json).ok_or(StatusCode::NOT_FOUND)
}

pub async fn update(State(state): State<AppState>, Path(id): Path<String>, Json(body): Json<PackBody>) -> Result<Json<Pack>, StatusCode> {
    let mut p = state.storage.get_pack(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    p.name = body.name;
    p.identity = body.identity;
    if !body.meta.is_null() {
        p.meta = body.meta;
    }
    p.updated_at = chrono::Utc::now().to_rfc3339();
    state.storage.update_pack(&p).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(p))
}

pub async fn orphan_definitions(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Vec<Definition>>, StatusCode> {
    state.storage.orphaned_definitions_for_pack(&id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<String>, Query(q): Query<DeleteQuery>) -> Result<StatusCode, StatusCode> {
    state.storage.delete_pack(&id, q.delete_orphans).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn duplicate(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Pack>, StatusCode> {
    let original = state.storage.get_pack(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let mut copy = Pack::new(format!("{} (copy)", original.name));
    copy.identity = original.identity;
    copy.meta = original.meta;
    state.storage.create_pack(&copy).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state.storage.copy_nodes(&OwnerKind::Pack, &id, &OwnerKind::Pack, &copy.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(copy))
}
