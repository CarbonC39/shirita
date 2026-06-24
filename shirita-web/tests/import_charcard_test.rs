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
    assert!(nodes.as_array().unwrap().iter().any(|n| n["kind"] == "folder" && n["tag"] == "char"));
    // A pack carries no history/content mount of its own — those belong solely
    // to the template that eventually mounts it (see loreset_to_pack).
    assert!(!nodes.as_array().unwrap().iter().any(|n| n["kind"] == "history" || n["kind"] == "content"));
}

#[tokio::test]
async fn import_charcard_with_status_bar_emits_panel_bricks() {
    // A status-bar regex now imports as a `panel` folder holding html + css
    // brick definitions (named `<card>·panel·html` / `·css`), not a meta.panel
    // blob — so the import summary reports them as plain definitions.
    let state = test_state().await;
    let card = r#"{"data":{"name":"Neo","description":"d","extensions":{"regex_scripts":[
        {"scriptName":"status","findRegex":"<hp>(\\d+)</hp>","replaceString":"HP: $1","disabled":false,"markdownOnly":true}
    ]}}}"#;
    let (st, body) = send(&state, "POST", "/api/import/charcard", Some(card)).await;
    assert_eq!(st, StatusCode::OK);
    let summary: Value = serde_json::from_str(&body).unwrap();
    let created = summary["created"].as_array().unwrap();
    let has = |name: &str| created.iter().any(|c| c["name"] == name);
    assert!(has("Neo·panel·html"), "expected a panel html brick in created: {created:?}");
    assert!(has("Neo·panel·css"), "expected a panel css brick in created: {created:?}");
}

#[tokio::test]
async fn unrelated_cards_dont_share_a_same_named_regex_rule() {
    // Bug: regex_rule defs were deduped by name+def_type across charcard
    // imports, so two unrelated cards whose ST regex script happened to share
    // a generic `scriptName` (e.g. "Remove asterisks") ended up pointing at
    // the very same Definition row — toggling/editing one card's rule would
    // silently affect the other card's pack too.
    let state = test_state().await;
    let card_a = r#"{"data":{"name":"CardA","extensions":{"regex_scripts":[{"scriptName":"Clean","findRegex":"a","replaceString":"A"}]}}}"#;
    let card_b = r#"{"data":{"name":"CardB","extensions":{"regex_scripts":[{"scriptName":"Clean","findRegex":"b","replaceString":"B"}]}}}"#;
    let (st_a, _) = send(&state, "POST", "/api/import/charcard", Some(card_a)).await;
    assert_eq!(st_a, StatusCode::OK);
    let (st_b, _) = send(&state, "POST", "/api/import/charcard", Some(card_b)).await;
    assert_eq!(st_b, StatusCode::OK);

    let (_, defs) = send(&state, "GET", "/api/definitions", None).await;
    let defs: Value = serde_json::from_str(&defs).unwrap();
    let rules: Vec<_> = defs.as_array().unwrap().iter().filter(|d| d["type"] == "regex_rule" && d["name"] == "Clean").collect();
    assert_eq!(rules.len(), 2, "each card keeps its own regex_rule definition, not a shared one");
    let patterns: std::collections::HashSet<_> = rules.iter().map(|r| r["meta"]["pattern"].as_str().unwrap()).collect();
    assert_eq!(patterns, std::collections::HashSet::from(["a", "b"]), "neither card's pattern was clobbered by the other's");
}
