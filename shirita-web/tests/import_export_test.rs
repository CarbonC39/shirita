//! /api/import/{worldinfo,charcard}：把 ST JSON 落库为 world/char 定义。

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
    let path = dir.path().join("import_test.db");
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
async fn import_worldinfo_creates_world_defs() {
    let state = test_state().await;
    let body = r#"{"entries":{"0":{"key":["zion"],"comment":"Zion","content":"Last city","constant":false}}}"#;
    let (st, out) = send(&state, "POST", "/api/import/worldinfo", Some(body)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["created"].as_array().unwrap().len(), 1);
    // it is now listed as a world definition
    let (_, defs) = send(&state, "GET", "/api/definitions?type=world", None).await;
    assert!(json(&defs).as_array().unwrap().iter().any(|d| d["name"] == "Zion"));
}

#[tokio::test]
async fn import_charcard_creates_char_and_book() {
    let state = test_state().await;
    let card = r#"{"spec":"chara_card_v2","data":{"name":"Neo","description":"The One","character_book":{"entries":[{"keys":["zion"],"comment":"Zion","content":"x"}]}}}"#;
    let (st, out) = send(&state, "POST", "/api/import/charcard", Some(card)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["created"].as_array().unwrap().len(), 2); // char + 1 world
}
