use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use base64::Engine;
use tower::ServiceExt;

use shirita_core::{Config, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app, AppState};

async fn make_state(user: Option<&str>, pass: Option<&str>) -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("basic_auth.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let mut config = Config::new("ignored", "./assets", "secret-token").unwrap();
    config.http_auth_user = user.map(str::to_string);
    config.http_auth_pass = pass.map(str::to_string);
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let provider: Arc<dyn ModelProvider> = Arc::new(shirita_core::EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState {
        storage,
        config: Arc::new(config),
        provider,
        token_counter,
        model: "m".into(),
        generations: Arc::new(shirita_web::Generations::new()),
        http_client: shirita_web::new_http_client(),
    }
}

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn basic_auth_disabled_when_creds_unset() {
    // No HTTP_AUTH_* configured → middleware is a no-op; /health must pass
    // without any Authorization header.
    let state = make_state(None, None).await;
    let res = app(state).oneshot(get("/health")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn basic_auth_challenges_when_no_credentials_provided() {
    let state = make_state(Some("alice"), Some("s3cret")).await;
    let res = app(state).oneshot(get("/health")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    // Browser pops the native login dialog only when WWW-Authenticate is set.
    let challenge = res
        .headers()
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        challenge.starts_with("Basic "),
        "expected 'Basic ...' challenge, got {challenge:?}"
    );
}

#[tokio::test]
async fn basic_auth_accepts_correct_credentials() {
    let state = make_state(Some("alice"), Some("s3cret")).await;
    let encoded = base64::engine::general_purpose::STANDARD.encode("alice:s3cret");
    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .header(header::AUTHORIZATION, format!("Basic {encoded}"))
        .body(Body::empty())
        .unwrap();
    let res = app(state).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn basic_auth_rejects_wrong_credentials() {
    let state = make_state(Some("alice"), Some("s3cret")).await;
    let encoded = base64::engine::general_purpose::STANDARD.encode("alice:wrong");
    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .header(header::AUTHORIZATION, format!("Basic {encoded}"))
        .body(Body::empty())
        .unwrap();
    let res = app(state).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert!(res.headers().contains_key("www-authenticate"));
}

#[tokio::test]
async fn basic_auth_gates_index_page_too() {
    // The whole point of this layer: an anonymous visitor must not pull the UI
    // shell even though /api/* would 401 via Bearer. / is the embedded index.
    let state = make_state(Some("alice"), Some("s3cret")).await;
    let res = app(state).oneshot(get("/")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
