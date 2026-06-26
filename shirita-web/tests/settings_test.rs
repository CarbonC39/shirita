//! PUT/GET /api/settings: a key/value object round-trips, and a non-object body
//! is rejected. Guards the update_all handler against regressions.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings_test.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "test-model".into(), generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client() }
}

async fn send(state: &AppState, method: &str, uri: &str, body: Option<&str>) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secret-token");
    let body = match body {
        Some(json) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(json.to_string())
        }
        None => Body::empty(),
    };
    let res = app(state.clone()).oneshot(builder.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[tokio::test]
async fn update_all_persists_then_get_all_returns_pairs() {
    let state = test_state().await;
    let (st, _) =
        send(&state, "PUT", "/api/settings", Some(r#"{"theme":"dark","provider":"openai"}"#)).await;
    assert_eq!(st, StatusCode::OK);

    let (st, body) = send(&state, "GET", "/api/settings", None).await;
    assert_eq!(st, StatusCode::OK);
    let v = json(&body);
    assert_eq!(v["theme"], "dark");
    assert_eq!(v["provider"], "openai");
}

#[tokio::test]
async fn update_all_rejects_a_non_object_body() {
    let state = test_state().await;
    let (st, _) = send(&state, "PUT", "/api/settings", Some(r#"["not","an","object"]"#)).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}
