#![cfg(feature = "embed-ui")]
//! Embedded-UI serving. Compiled/run only with `--features embed-ui`, which
//! needs `shirita-ui/dist` built (Step 1). The Docker/CI build exercises this.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
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
    AppState { storage, config, provider, token_counter, model: "m".into(), generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client() }
}

async fn get(state: &AppState, uri: &str) -> (StatusCode, Vec<u8>) {
    let req = Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, body)
}

#[tokio::test]
async fn index_serves_spa_with_injected_token() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let (st, body) = get(&state, "/").await;
    assert_eq!(st, StatusCode::OK);
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("window.__SHIRITA_RUNTIME__="), "runtime injected");
    assert!(html.contains(r#""token":"secret-token""#), "token present");
    assert!(html.contains("/static/"), "built index references /static chunks");
}

#[tokio::test]
async fn deep_link_serves_index_unknown_api_404s() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    // Vue Router history deep link → SPA shell.
    let (st, body) = get(&state, "/book").await;
    assert_eq!(st, StatusCode::OK);
    assert!(String::from_utf8_lossy(&body).contains("__SHIRITA_RUNTIME__"));
    // Unknown API path → 404, never HTML.
    let (st2, _b2) = get(&state, "/api/nope").await;
    assert_eq!(st2, StatusCode::NOT_FOUND);
}
