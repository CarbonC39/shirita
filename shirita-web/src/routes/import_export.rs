use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::AppState;

async fn persist(state: &AppState, defs: Vec<shirita_core::Definition>) -> Result<usize, StatusCode> {
    let n = defs.len();
    for d in defs {
        state.storage.create_definition(&d).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(n)
}

/// POST /api/import/worldinfo — body 为 ST 世界书 JSON。
pub async fn import_worldinfo(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let defs = shirita_core::worldinfo_to_defs(&body);
    let created = persist(&state, defs).await?;
    Ok(Json(serde_json::json!({ "created": created })))
}

/// POST /api/import/charcard — body 为 chara_card_v2/v3 JSON。
pub async fn import_charcard(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let (ch, book) = shirita_core::charcard_to_defs(&body);
    let mut all = vec![ch];
    all.extend(book);
    let created = persist(&state, all).await?;
    Ok(Json(serde_json::json!({ "created": created })))
}
