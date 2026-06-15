//! Copy-on-write local definition override endpoints: set / clear / promote.

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
    let path = dir.path().join("local_overrides.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "test-model".into(), generations: Arc::new(shirita_web::Generations::new()) }
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

async fn create_def(state: &AppState, name: &str, content: &str) -> String {
    let (st, out) = send(state, "POST", "/api/definitions",
        Some(&format!(r#"{{"type":"prompt","name":"{name}","content":"{content}","meta":{{}}}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    json(&out)["id"].as_str().unwrap().to_string()
}

async fn get_session(state: &AppState, sid: &str) -> Value {
    let (_, out) = send(state, "GET", &format!("/api/sessions/{sid}"), None).await;
    json(&out)
}

async fn create_template(state: &AppState, name: &str) -> String {
    let (_, out) = send(state, "POST", "/api/templates", Some(&format!(r#"{{"name":"{name}"}}"#))).await;
    json(&out)["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn materialize_copies_template_nodes_once() {
    let state = test_state().await;
    // `create_template` seeds a single root `history` node, so the template
    // already owns one node we can deep-copy.
    let tid = create_template(&state, "T").await;
    // a session using that template
    let (_, sout) = send(&state, "POST", "/api/sessions",
        Some(&format!(r#"{{"name":"Chat","template_id":"{tid}"}}"#))).await;
    let sid = json(&sout)["id"].as_str().unwrap().to_string();

    // before: session has no own nodes
    let (_, before) = send(&state, "GET", &format!("/api/templates/{sid}/nodes?owner_kind=session"), None).await;
    assert_eq!(json(&before).as_array().unwrap().len(), 0);

    let (st, _) = send(&state, "POST", &format!("/api/sessions/{sid}/materialize-nodes"), Some("{}")).await;
    assert_eq!(st, StatusCode::OK);

    let (_, after) = send(&state, "GET", &format!("/api/templates/{sid}/nodes?owner_kind=session"), None).await;
    assert_eq!(json(&after).as_array().unwrap().len(), 1);

    // idempotent: a second call doesn't double the tree
    send(&state, "POST", &format!("/api/sessions/{sid}/materialize-nodes"), Some("{}")).await;
    let (_, after2) = send(&state, "GET", &format!("/api/templates/{sid}/nodes?owner_kind=session"), None).await;
    assert_eq!(json(&after2).as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn set_clear_and_promote_local_definition() {
    let state = test_state().await;
    let sid = create(&state, "Chat").await;
    let did = create_def(&state, "Lore", "global text").await;

    // set a local content override
    let (st, _) = send(&state, "PUT", &format!("/api/sessions/{sid}/local-definitions/{did}"),
        Some(r#"{"content":"local text"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    let s = get_session(&state, &sid).await;
    assert_eq!(s["override_config"]["local_definitions"][&did]["content"], "local text");
    // global is untouched
    let (_, gdef) = send(&state, "GET", &format!("/api/definitions/{did}"), None).await;
    assert_eq!(json(&gdef)["content"], "global text");

    // promote -> global takes the local content, override cleared
    let (st2, _) = send(&state, "POST",
        &format!("/api/sessions/{sid}/local-definitions/{did}/promote"), Some("{}")).await;
    assert_eq!(st2, StatusCode::OK);
    let (_, gdef2) = send(&state, "GET", &format!("/api/definitions/{did}"), None).await;
    assert_eq!(json(&gdef2)["content"], "local text");
    let s2 = get_session(&state, &sid).await;
    assert!(s2["override_config"]["local_definitions"].get(&did).is_none()
        || s2["override_config"]["local_definitions"][&did].is_null());

    // set again then clear (revert)
    send(&state, "PUT", &format!("/api/sessions/{sid}/local-definitions/{did}"),
        Some(r#"{"content":"temp"}"#)).await;
    let (st3, _) = send(&state, "DELETE",
        &format!("/api/sessions/{sid}/local-definitions/{did}"), None).await;
    assert_eq!(st3, StatusCode::OK);
    let s3 = get_session(&state, &sid).await;
    assert!(s3["override_config"]["local_definitions"].get(&did).is_none()
        || s3["override_config"]["local_definitions"][&did].is_null());
}
