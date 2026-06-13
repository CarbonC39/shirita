use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::AppState;

pub async fn get_all(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let pairs = state.storage.list_settings().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut obj = serde_json::Map::new();
    for (key, value) in pairs { obj.insert(key, value); }
    Ok(Json(Value::Object(obj)))
}

pub async fn update_all(State(state): State<AppState>, Json(body): Json<Value>) -> Result<StatusCode, StatusCode> {
    if let Some(o) = body.as_object() {
        for (key, value) in o { state.storage.set_setting(key, value).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    }
    Ok(StatusCode::OK)
}
