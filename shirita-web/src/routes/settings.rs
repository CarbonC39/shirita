use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::AppState;

pub async fn get_all(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let pairs = state.storage.list_settings().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let obj: serde_json::Map<String, Value> = pairs.into_iter().collect();
    Ok(Json(Value::Object(obj)))
}

pub async fn update_all(State(state): State<AppState>, Json(body): Json<Value>) -> Result<StatusCode, StatusCode> {
    let o = body.as_object().ok_or(StatusCode::BAD_REQUEST)?;
    // Persist every key in one transaction so a mid-batch failure can't leave
    // settings half-applied.
    let pairs: Vec<(String, Value)> = o.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    state.storage.set_settings(&pairs).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}
