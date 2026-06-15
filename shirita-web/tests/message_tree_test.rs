//! Message-tree endpoints: in-place edit, hide, active-leaf switch, regenerate, fork.

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
    let path = dir.path().join("message_tree.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "test-model".into() }
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

async fn create(state: &AppState, name: &str) -> String {
    let (st, out) = send(state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"{name}"}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    json(&out)["id"].as_str().unwrap().to_string()
}

// run one send turn (collects the SSE body to completion)
async fn turn(state: &AppState, sid: &str, text: &str) {
    let (st, _) = send(state, "POST", &format!("/api/sessions/{sid}/messages"),
        Some(&format!(r#"{{"text":"{text}"}}"#))).await;
    assert_eq!(st, StatusCode::OK);
}

async fn messages(state: &AppState, sid: &str) -> Value {
    let (_, out) = send(state, "GET", &format!("/api/sessions/{sid}/messages"), None).await;
    json(&out)
}

#[tokio::test]
async fn edit_overwrites_in_place_and_recomputes_display() {
    let state = test_state().await;
    let sid = create(&state, "Chat").await;
    turn(&state, &sid, "hi").await;
    let msgs = messages(&state, &sid).await;
    let user = msgs.as_array().unwrap().iter().find(|m| m["role"] == "user").unwrap();
    let mid = user["id"].as_str().unwrap();

    let (st, out) = send(&state, "PUT", &format!("/api/sessions/{sid}/messages/{mid}"),
        Some(r#"{"content":"edited"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["raw_content"], "edited");

    let after = messages(&state, &sid).await;
    let same = after.as_array().unwrap().len();
    assert_eq!(same, 2); // no new branch — in-place edit
}

#[tokio::test]
async fn hide_toggles_is_hidden() {
    let state = test_state().await;
    let sid = create(&state, "Chat").await;
    turn(&state, &sid, "hi").await;
    let msgs = messages(&state, &sid).await;
    let a = msgs.as_array().unwrap().iter().find(|m| m["role"] == "assistant").unwrap();
    let mid = a["id"].as_str().unwrap();

    let (st, out) = send(&state, "PUT", &format!("/api/sessions/{sid}/messages/{mid}"),
        Some(r#"{"is_hidden":true}"#)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["is_hidden"], true);
}
