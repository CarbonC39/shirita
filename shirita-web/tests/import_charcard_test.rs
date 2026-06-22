use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use shirita_core::{Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("import_card.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "m".into(), generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client() }
}

async fn send(state: &AppState, method: &str, uri: &str, body: Option<&str>) -> (StatusCode, String) {
    let mut b = Request::builder().method(method).uri(uri).header(header::AUTHORIZATION, "Bearer secret-token");
    let body = match body {
        Some(j) => {
            b = b.header(header::CONTENT_TYPE, "application/json");
            Body::from(j.to_string())
        }
        None => Body::empty(),
    };
    let res = app(state.clone()).oneshot(b.body(body).unwrap()).await.unwrap();
    let st = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (st, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn import_charcard_creates_pack() {
    let state = test_state().await;
    let card = r#"{"spec":"chara_card_v3","data":{"name":"Neo","description":"d","first_mes":"hi","character_book":{"entries":[{"keys":["zion"],"comment":"Zion","content":"c"}]},"extensions":{"regex_scripts":[{"scriptName":"r","findRegex":"a","replaceString":"b","markdownOnly":true}]}}}"#;
    let (st, _) = send(&state, "POST", "/api/import/charcard", Some(card)).await;
    assert_eq!(st, StatusCode::OK);

    let (_, defs) = send(&state, "GET", "/api/definitions", None).await;
    let defs: Value = serde_json::from_str(&defs).unwrap();
    let arr = defs.as_array().unwrap();
    assert!(arr.iter().any(|d| d["type"] == "char" && d["name"] == "Neo"));
    assert!(arr.iter().any(|d| d["type"] == "first_message"));
    assert!(arr.iter().any(|d| d["type"] == "world"));
    assert!(arr.iter().any(|d| d["type"] == "regex_rule"));

    // ST character cards import as a Pack — the format designed to hold one
    // self-contained piece of imported content — not a bare Template.
    let (_, tmpls) = send(&state, "GET", "/api/templates", None).await;
    let tmpls: Value = serde_json::from_str(&tmpls).unwrap();
    assert!(
        !tmpls.as_array().unwrap().iter().any(|t| t["name"] == "Neo"),
        "charcard import must not create a Template"
    );

    let (_, packs) = send(&state, "GET", "/api/packs", None).await;
    let packs: Value = serde_json::from_str(&packs).unwrap();
    let p = packs.as_array().unwrap().iter().find(|p| p["name"] == "Neo").unwrap();
    assert_eq!(p["identity"]["display_name"], "Neo");
    let pid = p["id"].as_str().unwrap();
    let (_, nodes) = send(&state, "GET", &format!("/api/packs/{pid}/nodes?owner_kind=pack"), None).await;
    let nodes: Value = serde_json::from_str(&nodes).unwrap();
    assert!(nodes.as_array().unwrap().iter().any(|n| n["kind"] == "history"));
}
