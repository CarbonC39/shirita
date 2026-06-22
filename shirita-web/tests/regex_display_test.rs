//! Read-time Display-side regex: `list_messages` applies regex per request to
//! `display_content`, leaving `raw_content` (and the stored row) untouched, so
//! editing a rule refreshes history with no message re-write.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use shirita_core::{
    Config, Definition, EchoProvider, Message, ModelProvider, Role, Session, SqliteStorage,
    Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("regex_display.db");
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

async fn messages(state: &AppState, sid: &str) -> Value {
    let res = app(state.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/sessions/{sid}/messages"))
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn list_messages_applies_display_regex_at_read_time() {
    let state = test_state().await;
    let session = Session::new("Chat");
    state.storage.create_session(&session).await.unwrap();

    // Global orphan rule: strip "SECRET" from AI output for display only.
    let mut rule = Definition::new("regex_rule", "redact", "");
    rule.meta = serde_json::json!({
        "pattern": "SECRET", "replacement": "", "scope": "display", "targets": ["ai_output"]
    });
    state.storage.create_definition(&rule).await.unwrap();

    // One assistant message whose raw content holds the secret.
    let msg = Message::new(&session.id, None, Role::Assistant, "a SECRET b");
    state.storage.create_message(&msg).await.unwrap();

    // Read-time: display_content is stripped, raw_content keeps the secret.
    let out = messages(&state, &session.id).await;
    let m = out.as_array().unwrap().iter().find(|m| m["role"] == "assistant").unwrap();
    assert_eq!(m["raw_content"], "a SECRET b");
    assert_eq!(m["display_content"], "a  b");

    // Editing the rule reflects immediately on the next fetch — no re-write.
    rule.meta = serde_json::json!({
        "pattern": "SECRET", "replacement": "[redacted]", "scope": "display", "targets": ["ai_output"]
    });
    state.storage.update_definition(&rule).await.unwrap();
    let out = messages(&state, &session.id).await;
    let m = out.as_array().unwrap().iter().find(|m| m["role"] == "assistant").unwrap();
    assert_eq!(m["display_content"], "a [redacted] b");
    assert_eq!(m["raw_content"], "a SECRET b", "raw row never mutated");
}

#[tokio::test]
async fn list_messages_skips_rp_regex_on_a_fenced_html_card_document() {
    // Bug: a real-world ST "HTML card" greeting is commonly the whole
    // document wrapped in a ```/```html fence (sometimes with CRLF line
    // endings), not starting with the doctype literally. The skip-check in
    // list_messages used a doctype-prefix test that missed the fence, so an
    // RP regex meant for prose (e.g. swapping "SECRET") could mangle markup
    // inside the card instead of being skipped.
    let state = test_state().await;
    let session = Session::new("Chat");
    state.storage.create_session(&session).await.unwrap();

    let mut rule = Definition::new("regex_rule", "redact", "");
    rule.meta = serde_json::json!({
        "pattern": "SECRET", "replacement": "", "scope": "display", "targets": ["ai_output"]
    });
    state.storage.create_definition(&rule).await.unwrap();

    let html = "```\r\n<!DOCTYPE html>\r\n<html><body>SECRET</body></html>\r\n```";
    let msg = Message::new(&session.id, None, Role::Assistant, html);
    state.storage.create_message(&msg).await.unwrap();

    let out = messages(&state, &session.id).await;
    let m = out.as_array().unwrap().iter().find(|m| m["role"] == "assistant").unwrap();
    assert_eq!(m["raw_content"], html);
    assert!(m["display_content"].is_null(), "skipped — left untouched rather than regex-mangled");
}
