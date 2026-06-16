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

#[tokio::test]
async fn embedded_server_binds_serves_and_shuts_down_gracefully() {
    use tokio_util::sync::CancellationToken;

    // 单独构造 storage 以保留 pool 句柄（模拟桌面 bin 的优雅关闭）。
    let dir = tempfile::tempdir().unwrap();
    let storage = SqliteStorage::connect(dir.path().join("smoke.db").to_str().unwrap())
        .await
        .unwrap();
    storage.run_migrations().await.unwrap();
    let pool = storage.pool().clone();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let assets = dir.path().join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let config = Arc::new(Config::new("ignored", assets.to_str().unwrap(), "secret-token").unwrap());
    let state = AppState {
        storage,
        config,
        provider: Arc::new(EchoProvider),
        token_counter: Arc::new(TiktokenCounter::new()),
        model: "m".into(),
        generations: Arc::new(Generations::new()),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    assert!(port > 0, "OS 应分配一个真实端口");

    let token = CancellationToken::new();
    let child = token.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, app_with_cors(state))
            .with_graceful_shutdown(async move { child.cancelled().await })
            .await
    });

    // 让 server 起来，然后广播关闭。
    tokio::task::yield_now().await;
    token.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .expect("server 应在取消后及时退出")
        .expect("server task join")
        .expect("axum::serve 返回 Ok");

    // 显式关池（桌面 RunEvent::Exit 的行为）。
    pool.close().await;
    assert!(pool.is_closed());
}
