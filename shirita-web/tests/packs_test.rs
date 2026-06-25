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
async fn session_pack_mounts_roundtrip() {
    let state = test_state().await;
    let (_, b) = send(&state, "POST", "/api/sessions", Some(json!({ "name": "Chat" }))).await;
    let sid = body_json(&b)["id"].as_str().unwrap().to_string();
    let (_, b) = send(&state, "POST", "/api/packs", Some(json!({ "name": "Alice" }))).await;
    let pid = body_json(&b)["id"].as_str().unwrap().to_string();

    let (st, _) = send(&state, "PUT", &format!("/api/sessions/{sid}/packs"), Some(json!({ "pack_ids": [pid] }))).await;
    assert_eq!(st, StatusCode::OK);
    let (st, b) = send(&state, "GET", &format!("/api/sessions/{sid}/packs"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body_json(&b), json!([pid]));
}

#[tokio::test]
async fn create_session_mounts_packs_and_seeds_pack_variables() {
    let state = test_state().await;
    // a pack declaring a variable (via a `variables` brick) + carrying a greeting
    let (_, b) = send(&state, "POST", "/api/packs", Some(json!({ "name": "Alice" }))).await;
    let pid = body_json(&b)["id"].as_str().unwrap().to_string();
    let (_, b) = send(&state, "POST", "/api/definitions", Some(json!({
        "type": "first_message", "name": "hello", "content": "Hi, I'm Alice."
    }))).await;
    let gid = body_json(&b)["id"].as_str().unwrap().to_string();
    send(&state, "POST", &format!("/api/packs/{pid}/nodes?owner_kind=pack"),
        Some(json!({ "kind": "ref", "definition_id": gid }))).await;
    let (_, b) = send(&state, "POST", "/api/definitions", Some(json!({
        "type": "variables", "name": "Vars", "content": "",
        "meta": { "decls": [ { "name": "affection", "type": "number", "initial": "5" } ] }
    }))).await;
    let vid = body_json(&b)["id"].as_str().unwrap().to_string();
    send(&state, "POST", &format!("/api/packs/{pid}/nodes?owner_kind=pack"),
        Some(json!({ "kind": "ref", "definition_id": vid }))).await;
    // a template (gets content+history)
    let (_, b) = send(&state, "POST", "/api/templates", Some(json!({ "name": "T" }))).await;
    let tid = body_json(&b)["id"].as_str().unwrap().to_string();

    let (st, b) = send(&state, "POST", "/api/sessions", Some(json!({
        "name": "Chat", "template_id": tid, "pack_ids": [pid]
    }))).await;
    assert_eq!(st, StatusCode::OK);
    let s = body_json(&b);
    assert_eq!(s["mounted_packs"], json!([pid]));
    assert_eq!(s["current_state"]["affection"], "5", "pack variable initial seeded");
    let sid = s["id"].as_str().unwrap().to_string();

    // greeting from the pack's first_message was seeded
    let (st, b) = send(&state, "GET", &format!("/api/sessions/{sid}/messages"), None).await;
    assert_eq!(st, StatusCode::OK);
    let msgs = body_json(&b);
    assert!(msgs.as_array().unwrap().iter().any(|m| m["raw_content"].as_str().unwrap_or("").contains("I'm Alice")),
        "pack greeting seeded as a message");
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

async fn make_avatar_asset(state: &AppState, path: &str) {
    let mut a = shirita_core::Asset::new(path, path);
    a.kind = "avatar".into();
    state.storage.create_asset(&a).await.unwrap();
}

#[tokio::test]
async fn deleting_a_pack_garbage_collects_its_now_unreferenced_avatar() {
    // Bug: unreferenced avatars (e.g. one a charcard import saved) were never
    // automatically cleaned up — only the pack itself was removed, leaving
    // the Asset row (and file) behind forever with nothing pointing at it.
    let state = test_state().await;
    make_avatar_asset(&state, "orphan.png").await;
    let (_, b) = send(&state, "POST", "/api/packs", Some(json!({
        "name": "Solo", "identity": { "avatar": "orphan.png" }
    }))).await;
    let id = body_json(&b)["id"].as_str().unwrap().to_string();

    let (st, _) = send(&state, "DELETE", &format!("/api/packs/{id}"), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    let (_, assets) = send(&state, "GET", "/api/assets?kind=avatar", None).await;
    assert!(body_json(&assets).as_array().unwrap().is_empty(), "the now-unreferenced avatar is cleaned up");
}

#[tokio::test]
async fn deleting_a_pack_keeps_avatar_still_used_by_another_pack() {
    let state = test_state().await;
    make_avatar_asset(&state, "shared.png").await;
    let (_, b1) = send(&state, "POST", "/api/packs", Some(json!({
        "name": "One", "identity": { "avatar": "shared.png" }
    }))).await;
    let id1 = body_json(&b1)["id"].as_str().unwrap().to_string();
    send(&state, "POST", "/api/packs", Some(json!({
        "name": "Two", "identity": { "avatar": "shared.png" }
    }))).await;

    let (st, _) = send(&state, "DELETE", &format!("/api/packs/{id1}"), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    let (_, assets) = send(&state, "GET", "/api/assets?kind=avatar", None).await;
    assert_eq!(body_json(&assets).as_array().unwrap().len(), 1, "still referenced by the other pack");
}

#[tokio::test]
async fn changing_a_packs_avatar_garbage_collects_the_old_one() {
    let state = test_state().await;
    make_avatar_asset(&state, "old.png").await;
    let (_, b) = send(&state, "POST", "/api/packs", Some(json!({
        "name": "Swap", "identity": { "avatar": "old.png" }
    }))).await;
    let id = body_json(&b)["id"].as_str().unwrap().to_string();

    let (st, _) = send(&state, "PUT", &format!("/api/packs/{id}"), Some(json!({
        "name": "Swap", "identity": { "avatar": "new.png" }
    }))).await;
    assert_eq!(st, StatusCode::OK);

    let (_, assets) = send(&state, "GET", "/api/assets?kind=avatar", None).await;
    assert!(body_json(&assets).as_array().unwrap().is_empty(), "old.png is now unreferenced and gets cleaned up");
}
