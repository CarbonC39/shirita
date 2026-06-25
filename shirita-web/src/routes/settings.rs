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
    for (key, value) in o {
        state.storage.set_setting(key, value).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(StatusCode::OK)
}
