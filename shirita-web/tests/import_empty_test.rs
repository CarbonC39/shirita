//! Import should ignore empty content-bearing entries (but keep identity
//! anchors and meta-only types).

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("import_empty.db");
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
        model: "m".into(),
        generations: Arc::new(shirita_web::Generations::new()),
        http_client: shirita_web::new_http_client(),
    }
}

#[tokio::test]
async fn worldinfo_import_skips_empty_entries() {
    let state = test_state().await;
    let storage = state.storage.clone();
    // a world book with one real entry and one empty-content entry
    let body = serde_json::json!({
        "entries": [
            { "keys": ["zion"], "comment": "Zion", "content": "Last city" },
            { "keys": ["void"], "comment": "Void", "content": "" }
        ]
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/import/worldinfo")
        .header("authorization", "Bearer secret-token")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app(state).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let defs = storage.list_definitions().await.unwrap();
    assert!(defs.iter().any(|d| d.content == "Last city"));
    assert!(!defs.iter().any(|d| d.content.trim().is_empty()));
}
