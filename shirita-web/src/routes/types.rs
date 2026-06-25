use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use shirita_core::models::def_type::DefType;

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateTypeBody {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub sort: i64,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<DefType>>, StatusCode> {
    state
        .storage
        .list_container_types()
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateTypeBody>,
) -> Result<Json<DefType>, StatusCode> {
    // The ID must not conflict with reserved types and must not be empty.
    if body.id.trim().is_empty() || shirita_core::is_reserved(&body.id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let ty = DefType::new(body.id, body.label, body.sort);
    state
        .storage
        .create_def_type(&ty)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ty))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // Built-in types cannot be deleted.
    let containers = state
        .storage
        .list_container_types()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match containers.iter().find(|c| c.id == id) {
        Some(c) if c.builtin => Err(StatusCode::BAD_REQUEST),
        Some(_) => {
            state
                .storage
                .delete_def_type(&id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}
