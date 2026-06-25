//! Home-management endpoints: delete / duplicate / export / import sessions.

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
    let path = dir.path().join("sessions_mgmt.db");
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

async fn create(state: &AppState, name: &str) -> String {
    let (st, out) = send(state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"{name}"}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    json(&out)["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn delete_removes_the_session() {
    let state = test_state().await;
    let id = create(&state, "Doomed").await;
    let (st, _) = send(&state, "DELETE", &format!("/api/sessions/{id}"), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (_, list) = send(&state, "GET", "/api/sessions", None).await;
    assert!(!json(&list).as_array().unwrap().iter().any(|s| s["id"] == id.as_str()));
}

#[tokio::test]
async fn duplicate_makes_a_copy() {
    let state = test_state().await;
    let id = create(&state, "Original").await;
    let (st, out) = send(&state, "POST", &format!("/api/sessions/{id}/duplicate"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["name"], "Original copy");
    let (_, list) = send(&state, "GET", "/api/sessions", None).await;
    assert_eq!(json(&list).as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn export_then_import_round_trips() {
    let state = test_state().await;
    let id = create(&state, "Exported").await;
    let (st, exported) = send(&state, "GET", &format!("/api/sessions/{id}/export"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&exported)["session"]["name"], "Exported");
    assert!(json(&exported)["messages"].as_array().unwrap().is_empty());

    let (st2, out2) = send(&state, "POST", "/api/sessions/import", Some(&exported)).await;
    assert_eq!(st2, StatusCode::OK);
    assert_eq!(json(&out2)["name"], "Exported");
    // a brand-new id, not the original
    assert_ne!(json(&out2)["id"].as_str().unwrap(), id);
}

/// Advance a session's active leaf onto a fresh state-carrier node and return it.
async fn seed_active_leaf(state: &AppState, id: &str) -> String {
    let (st, _) =
        send(state, "POST", &format!("/api/sessions/{id}/state-updates"), Some(r#"{"updates":[]}"#)).await;
    assert_eq!(st, StatusCode::OK);
    let (_, s) = send(state, "GET", &format!("/api/sessions/{id}"), None).await;
    json(&s)["active_leaf_id"].as_str().expect("seeded session has an active leaf").to_string()
}

async fn active_leaf_points_to_own_message(state: &AppState, id: &str) -> String {
    let (_, s) = send(state, "GET", &format!("/api/sessions/{id}"), None).await;
    let leaf = json(&s)["active_leaf_id"].as_str().unwrap_or("").to_string();
    assert!(!leaf.is_empty(), "session {id} must carry an active leaf");
    let (_, msgs) = send(state, "GET", &format!("/api/sessions/{id}/messages"), None).await;
    assert!(
        json(&msgs).as_array().unwrap().iter().any(|m| m["id"] == leaf.as_str()),
        "active leaf must reference a message that exists in session {id}",
    );
    leaf
}

#[tokio::test]
async fn duplicate_preserves_active_leaf() {
    let state = test_state().await;
    let id = create(&state, "Branched").await;
    let src_leaf = seed_active_leaf(&state, &id).await;

    let (st, out) = send(&state, "POST", &format!("/api/sessions/{id}/duplicate"), None).await;
    assert_eq!(st, StatusCode::OK);
    let dup_id = json(&out)["id"].as_str().unwrap().to_string();

    let dup_leaf = active_leaf_points_to_own_message(&state, &dup_id).await;
    assert_ne!(dup_leaf, src_leaf, "the copy's leaf must be a remapped id, not the source's");
}

#[tokio::test]
async fn import_preserves_active_leaf() {
    let state = test_state().await;
    let id = create(&state, "Branched").await;
    seed_active_leaf(&state, &id).await;

    let (st, exported) = send(&state, "GET", &format!("/api/sessions/{id}/export"), None).await;
    assert_eq!(st, StatusCode::OK);

    let (st2, out2) = send(&state, "POST", "/api/sessions/import", Some(&exported)).await;
    assert_eq!(st2, StatusCode::OK);
    let imp_id = json(&out2)["id"].as_str().unwrap().to_string();

    active_leaf_points_to_own_message(&state, &imp_id).await;
}

#[tokio::test]
async fn patch_renames_session() {
    let state = test_state().await;
    let id = create(&state, "Old").await;
    let (st, out) = send(&state, "PATCH", &format!("/api/sessions/{id}"), Some(r#"{"name":"New"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["name"], "New");
    let (_, got) = send(&state, "GET", &format!("/api/sessions/{id}"), None).await;
    assert_eq!(json(&got)["name"], "New");
}

