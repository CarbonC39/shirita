use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt; // oneshot

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("packs_test.db");
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
        generations: Arc::new(shirita_web::Generations::new()),
        http_client: shirita_web::new_http_client(),
    }
}

/// Send an authenticated request, returning (status, body-bytes).
async fn send(state: &AppState, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Vec<u8>) {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secret-token");
    let body = match body {
        Some(v) => {
            b = b.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    let res = app(state.clone()).oneshot(b.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, bytes)
}

fn body_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

#[tokio::test]
async fn new_template_has_content_before_history() {
    let state = test_state().await;
    let (_, b) = send(&state, "POST", "/api/templates", Some(json!({ "name": "T" }))).await;
    let tid = body_json(&b)["id"].as_str().unwrap().to_string();
    let (st, b) = send(&state, "GET", &format!("/api/templates/{tid}/nodes?owner_kind=template"), None).await;
    assert_eq!(st, StatusCode::OK);
    let nodes = body_json(&b);
    let arr = nodes.as_array().unwrap();
    let content = arr.iter().find(|n| n["kind"] == "content").expect("content node");
    let history = arr.iter().find(|n| n["kind"] == "history").expect("history node");
    assert!(content["sort_order"].as_i64() < history["sort_order"].as_i64());
}

#[tokio::test]
async fn pack_nodes_crud_via_reused_endpoints() {
    let state = test_state().await;
    let (_, b) = send(&state, "POST", "/api/packs", Some(json!({ "name": "Alice" }))).await;
    let pid = body_json(&b)["id"].as_str().unwrap().to_string();
    let (_, b) = send(&state, "POST", "/api/definitions", Some(json!({ "type": "char", "name": "Alice", "content": "hi" }))).await;
    let did = body_json(&b)["id"].as_str().unwrap().to_string();

    let (st, b) = send(&state, "POST", &format!("/api/packs/{pid}/nodes?owner_kind=pack"),
        Some(json!({ "kind": "ref", "definition_id": did }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b)["owner_kind"], "pack");

    let (st, b) = send(&state, "GET", &format!("/api/packs/{pid}/nodes?owner_kind=pack"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b).as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn pack_crud_roundtrip() {
    let state = test_state().await;
    let (st, b) = send(&state, "POST", "/api/packs", Some(json!({
        "name": "Alice", "identity": { "display_name": "Alice", "avatar": "a.png" }
    }))).await;
    assert_eq!(st, StatusCode::OK);
    let created = body_json(&b);
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["name"], "Alice");
    assert_eq!(created["identity"]["display_name"], "Alice");

    let (st, b) = send(&state, "GET", &format!("/api/packs/{id}"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b)["identity"]["avatar"], "a.png");

    let (st, b) = send(&state, "GET", "/api/packs", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b).as_array().unwrap().len(), 1);

    let (st, b) = send(&state, "PUT", &format!("/api/packs/{id}"), Some(json!({
        "name": "Alice 2", "identity": { "display_name": "Alice" }
    }))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b)["name"], "Alice 2");

    let (st, _) = send(&state, "DELETE", &format!("/api/packs/{id}"), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (st, _) = send(&state, "GET", &format!("/api/packs/{id}"), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}
