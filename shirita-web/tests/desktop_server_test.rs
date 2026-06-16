//! 桌面内嵌 server：CORS（preflight 绕过鉴权）+ 优雅关闭 smoke。

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app_with_cors, AppState, Generations};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_path_buf();
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(base.join("desk.db").to_str().unwrap())
        .await
        .unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let assets = base.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let config = Arc::new(Config::new("ignored", assets.to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState {
        storage,
        config,
        provider,
        token_counter,
        model: "m".into(),
        generations: Arc::new(Generations::new()),
    }
}

#[tokio::test]
async fn cors_preflight_bypasses_auth_and_allows_tauri_origin() {
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/ping")
        .header(header::ORIGIN, "tauri://localhost")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "authorization")
        .body(Body::empty())
        .unwrap();
    let res = app_with_cors(test_state().await).oneshot(req).await.unwrap();
    // preflight 不带 Authorization，却不能 401——必须被 CorsLayer 短路应答。
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "tauri://localhost"
    );
}

#[tokio::test]
async fn real_request_carries_cors_header() {
    let req = Request::builder()
        .method("GET")
        .uri("/api/ping")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::ORIGIN, "tauri://localhost")
        .body(Body::empty())
        .unwrap();
    let res = app_with_cors(test_state().await).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "tauri://localhost"
    );
}
