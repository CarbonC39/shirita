//! POST /api/import — SillyTavern chat-completion presets (prompts + prompt_order).

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

async fn import_named(state: &AppState, query: &str, filename: &str, data: &[u8]) -> (StatusCode, Value) {
    let boundary = "BND";
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/import{query}"))
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
async fn import_real_st_preset_creates_template_and_prompt_defs() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/示例预设.json")).unwrap();
    let (st, summary) = import_named(&state, "", "示例预设.json", &data).await;
    assert_eq!(st, StatusCode::OK);
    // template created, named after the filename stem
    let created = summary["created"].as_array().unwrap();
    assert!(created.iter().any(|c| c["kind"] == "template" && c["name"] == "示例预设"));
    // the three enabled authored prompts (main + nsfw + jailbreak) became prompt defs
    let defs = state.storage.list_definitions().await.unwrap();
    let prompts: Vec<_> = defs.iter().filter(|d| d.def_type == "prompt").collect();
    assert_eq!(prompts.len(), 3, "main + nsfw + jailbreak");
    assert!(prompts.iter().any(|d| d.name == "➡️扩写/转述输入"));
}

#[tokio::test]
async fn import_preset_with_empty_order_is_400() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let preset = serde_json::json!({
        "prompts": [],
        "prompt_order": [ { "character_id": 100000, "order": [] } ]
    });
    let (st, _) = import_named(&state, "", "empty.json", &serde_json::to_vec(&preset).unwrap()).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn two_presets_with_colliding_prompt_name_stay_independent() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let mk = |content: &str| {
        serde_json::json!({
            "prompts": [ { "identifier": "main", "name": "main", "content": content } ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "main", "enabled": true }
            ]}]
        })
    };
    // Distinct filenames -> distinct template names -> neither short-circuits under skip.
    let (s1, _) = import_named(&state, "", "preset-a.json", &serde_json::to_vec(&mk("AAA")).unwrap()).await;
    let (s2, _) = import_named(&state, "", "preset-b.json", &serde_json::to_vec(&mk("BBB")).unwrap()).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    let defs = state.storage.list_definitions().await.unwrap();
    let mains: Vec<_> = defs.iter().filter(|d| d.def_type == "prompt" && d.name == "main").collect();
    assert_eq!(mains.len(), 2, "fresh def per import — no dedup, no overwrite");
    assert!(mains.iter().any(|d| d.content == "AAA"));
    assert!(mains.iter().any(|d| d.content == "BBB"));
}
