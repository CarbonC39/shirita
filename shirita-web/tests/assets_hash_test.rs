//! Uploading an asset records its content hash.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use shirita_core::{
    sha256_hex, Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter,
    TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state(dir: &std::path::Path) -> AppState {
    let storage = SqliteStorage::connect(dir.join("a.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", dir.join("assets").to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "m".into(), generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client() }
}

#[tokio::test]
async fn upload_records_content_hash() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    let state = test_state(dir.path()).await;

    let bytes = b"fake-image-bytes";
    let boundary = "BOUNDARY";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.png\"\r\n\
         Content-Type: image/png\r\n\r\n{data}\r\n--{b}--\r\n",
        b = boundary,
        data = std::str::from_utf8(bytes).unwrap(),
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/assets?kind=avatar")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let assets = state.storage.list_assets(None).await.unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].hash.as_deref(), Some(sha256_hex(bytes).as_str()));
}
