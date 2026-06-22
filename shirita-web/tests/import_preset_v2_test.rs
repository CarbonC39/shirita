//! POST /api/import — fidelity v2 against the real example preset.

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

async fn test_state(dir: &std::path::Path) -> AppState {
    let storage = SqliteStorage::connect(dir.join("p.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", dir.join("assets").to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState {
        storage,
        config,
        provider,
        token_counter,
        model: "m".into(),
        generations: Arc::new(shirita_web::Generations::new()),
        http_client: shirita_web::new_http_client(),
    }
}

async fn import_named(state: &AppState, filename: &str, data: &[u8]) -> (StatusCode, Value) {
    let boundary = "BND";
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method("POST")
        .uri("/api/import")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn imports_real_preset_with_variables_and_inactive_folder() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/示例预设.json")).unwrap();
    let (st, summary) = import_named(&state, "示例预设.json", &data).await;
    assert_eq!(st, StatusCode::OK);
    let tmpl_id = summary["created"].as_array().unwrap().iter()
        .find(|c| c["kind"] == "template").expect("template created")["id"].as_str().unwrap().to_string();

    // Variables from the jailbreak setvar block landed on the template.
    let tmpl = state.storage.get_template(&tmpl_id).await.unwrap().unwrap();
    let vars = tmpl.meta["variables"].as_array().expect("variables registered");
    let has = |n: &str| vars.iter().any(|v| v["name"] == n);
    assert!(has("wordsCloud") && has("JailbreakPrompt"), "setvar variables registered");

    // The all-setvar jailbreak prompt had its variables registered (asserted above);
    // any remaining non-macro content may or may not produce a def, but variables are set.

    // An inactive folder exists for the out-of-order library prompts.
    let nodes = state.storage.list_nodes(&shirita_core::OwnerKind::Template, &tmpl_id).await.unwrap();
    assert!(nodes.iter().any(|n| n.tag.as_deref() == Some("inactive") && !n.enabled), "inactive folder");
}
