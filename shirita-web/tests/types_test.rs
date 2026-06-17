//! /api/types：列出（3 内置）/ 新建 / 删除（内置受保护）。

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
    let path = dir.path().join("types_test.db");
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
async fn types_crud_and_builtin_protected() {
    let state = test_state().await;
    // list seeds 3 builtin
    let (st, body) = send(&state, "GET", "/api/types", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&body).as_array().unwrap().len(), 3);

    // create custom
    let (st, _) = send(&state, "POST", "/api/types", Some(r#"{"id":"faction","label":"Faction"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&send(&state, "GET", "/api/types", None).await.1).as_array().unwrap().len(), 4);

    // delete custom OK
    let (st, _) = send(&state, "DELETE", "/api/types/faction", None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    // delete builtin rejected
    let (st, _) = send(&state, "DELETE", "/api/types/char", None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}
