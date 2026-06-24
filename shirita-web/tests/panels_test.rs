use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt; // oneshot

use shirita_core::models::definition::Definition;
use shirita_core::models::pack::Pack;
use shirita_core::models::prompt_node::{OwnerKind, PromptNode};
use shirita_core::models::session::Session;
use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("panels_test.db");
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
async fn get_panels_returns_mounted_pack_panel() {
    let state = test_state().await;

    // A pack with a panel folder: html + css children.
    let pack = Pack::new("HUD pack");
    state.storage.create_pack(&pack).await.unwrap();

    let html = Definition::new("html", "markup", "<div id=\"a\"></div>");
    let css = Definition::new("css", "theme", ".a{color:red}");
    state.storage.create_definition(&html).await.unwrap();
    state.storage.create_definition(&css).await.unwrap();

    let mut folder = PromptNode::new_folder(OwnerKind::Pack, &pack.id, None, 0, "panel");
    folder.meta = serde_json::json!({ "name": "Status", "caps": { "write": true } });
    let h = PromptNode::new_ref(OwnerKind::Pack, &pack.id, Some(folder.id.clone()), 0, &html.id);
    let c = PromptNode::new_ref(OwnerKind::Pack, &pack.id, Some(folder.id.clone()), 1, &css.id);
    state.storage.create_node(&folder).await.unwrap();
    state.storage.create_node(&h).await.unwrap();
    state.storage.create_node(&c).await.unwrap();

    // A session that mounts the pack.
    let session = Session::new("chat");
    state.storage.create_session(&session).await.unwrap();
    state.storage.set_mounted_packs(&session.id, &[pack.id.clone()]).await.unwrap();

    let (status, body) = send(&state, "GET", &format!("/api/sessions/{}/panels", session.id), None).await;
    assert_eq!(status, StatusCode::OK);
    let panels = body_json(&body);
    assert_eq!(panels.as_array().unwrap().len(), 1);
    assert_eq!(panels[0]["name"], "Status");
    assert_eq!(panels[0]["html"], "<div id=\"a\"></div>");
    assert_eq!(panels[0]["css"], ".a{color:red}");
    assert_eq!(panels[0]["caps"]["write"], true);
}

#[tokio::test]
async fn get_panels_missing_session_is_404() {
    let state = test_state().await;
    let (status, _) = send(&state, "GET", "/api/sessions/nope/panels", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
