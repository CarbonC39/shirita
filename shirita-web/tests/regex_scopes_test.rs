//! GET /api/regex-rules/scopes: per-rule scope (global vs template), the source
//! template names, and the fancy-regex compile error for invalid patterns.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use shirita_core::{
    Config, Definition, EchoProvider, ModelProvider, OwnerKind, PromptNode, SqliteStorage, Storage,
    Template, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("regex_scopes.db");
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

fn rule(name: &str, pattern: &str) -> Definition {
    let mut d = Definition::new("regex_rule", name, "");
    d.meta = serde_json::json!({ "pattern": pattern, "replacement": "" });
    d
}

#[tokio::test]
async fn regex_scopes_reports_scope_sources_and_errors() {
    let state = test_state().await;

    // Orphan rule (referenced by no node) → global.
    let g = rule("Global", r"\d+");
    state.storage.create_definition(&g).await.unwrap();

    // Rule referenced by a template → template-scoped, with the template name.
    let s = rule("Scoped", "foo");
    state.storage.create_definition(&s).await.unwrap();
    let tmpl = Template::new("My Card");
    state.storage.create_template(&tmpl).await.unwrap();
    state
        .storage
        .create_node(&PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, 0, &s.id))
        .await
        .unwrap();

    // Orphan rule with an invalid pattern → global + pattern_error.
    let bad = rule("Bad", "foo(");
    state.storage.create_definition(&bad).await.unwrap();

    let res = app(state.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/regex-rules/scopes")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let out: Value = serde_json::from_slice(&bytes).unwrap();
    let by_id = |id: &str| {
        out.as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()
            .clone()
    };

    let gj = by_id(&g.id);
    assert_eq!(gj["scope"], "global");
    assert_eq!(gj["template_names"].as_array().unwrap().len(), 0);
    assert!(gj["pattern_error"].is_null());

    let sj = by_id(&s.id);
    assert_eq!(sj["scope"], "template");
    assert_eq!(sj["template_names"], serde_json::json!(["My Card"]));

    let bj = by_id(&bad.id);
    assert_eq!(bj["scope"], "global");
    assert!(bj["pattern_error"].is_string(), "invalid pattern reports an error");
}
