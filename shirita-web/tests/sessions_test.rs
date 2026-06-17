use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sess_test.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState {
        storage,
        config,
        provider,
        token_counter,
        model: "test-model".into(),
        generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client(),
    }
}

fn auth(req: Request<Body>) -> Request<Body> {
    let (mut parts, body) = req.into_parts();
    parts
        .headers
        .insert(header::AUTHORIZATION, "Bearer secret-token".parse().unwrap());
    Request::from_parts(parts, body)
}

#[tokio::test]
async fn create_then_list_session() {
    let state = test_state().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"Chat A"}"#))
        .unwrap();
    let res = app(state.clone()).oneshot(auth(req)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(created["name"], "Chat A");
    assert!(created["id"].as_str().is_some());

    let req = Request::builder()
        .uri("/api/sessions")
        .body(Body::empty())
        .unwrap();
    let res = app(state).oneshot(auth(req)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
}
