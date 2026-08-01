//! Session-auth integration tests: login, session middleware, /assets gating,
//! logout, change-password, bootstrap provisioning.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use chrono::Duration;
use shirita_core::{
    ensure_bootstrap_user, hash_password, random_token, AuthSessionRecord, BootstrapCreds, Config,
    SqliteStorage, Storage, TiktokenCounter, User,
};
use shirita_web::{app, AppState};
use tower::ServiceExt;

async fn make_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    shirita_web::seed_test_session(&storage).await;
    let config = Config::new("ignored", "./assets").unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    AppState {
        storage,
        config: Arc::new(config),
        provider: Arc::new(shirita_core::EchoProvider),
        token_counter: Arc::new(TiktokenCounter::new()),
        model: "m".into(),
        generations: Arc::new(shirita_web::Generations::new()),
        http_client: shirita_web::new_http_client(),
    }
}

async fn mem_storage() -> SqliteStorage {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth_seed.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    storage
}

/// Seed a known user `alice` / `s3cret`.
async fn seed_alice(state: &AppState) {
    let hash = hash_password("s3cret").unwrap();
    state.storage.create_user(&User::new("alice", hash)).await.unwrap();
}

fn get(path: &str) -> Request<Body> {
    Request::builder().method("GET").uri(path).body(Body::empty()).unwrap()
}

fn authed_get(path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn post_json(path: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn authed_post_json(path: &str, token: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Log in and return the issued token (asserts 200).
async fn login(state: &AppState, user: &str, pass: &str) -> String {
    let res = app(state.clone())
        .oneshot(post_json(
            "/api/auth/login",
            &format!(r#"{{"username":"{user}","password":"{pass}"}}"#),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "login {user}/{pass} should succeed");
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    v["token"].as_str().expect("token in response").to_string()
}

async fn body_json(res: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn health_is_public() {
    let state = make_state().await;
    let res = app(state).oneshot(get("/health")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn login_route_is_reachable_without_credentials() {
    // GET on the POST-only login route → 405 (route exists, wrong method),
    // proving it is mounted publicly (not hidden behind the session gate → 401).
    let state = make_state().await;
    let res = app(state).oneshot(get("/api/auth/login")).await.unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_401_without_token() {
    let state = make_state().await;
    let res = app(state).oneshot(get("/api/ping")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_success_then_protected_route_ok() {
    let state = make_state().await;
    seed_alice(&state).await;
    let token = login(&state, "alice", "s3cret").await;
    let res = app(state.clone()).oneshot(authed_get("/api/ping", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn login_rejects_wrong_password() {
    let state = make_state().await;
    seed_alice(&state).await;
    let res = app(state)
        .oneshot(post_json(
            "/api/auth/login",
            r#"{"username":"alice","password":"wrong"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_rejects_unknown_user() {
    let state = make_state().await;
    let res = app(state)
        .oneshot(post_json(
            "/api/auth/login",
            r#"{"username":"nobody","password":"x"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn expired_session_is_rejected() {
    let state = make_state().await;
    seed_alice(&state).await;
    let alice = state.storage.get_user_by_username("alice").await.unwrap().unwrap();
    let token = random_token();
    let past = (chrono::Utc::now() - Duration::seconds(1)).to_rfc3339();
    state
        .storage
        .create_auth_session(&AuthSessionRecord::new(&token, &alice.id, &past))
        .await
        .unwrap();
    let res = app(state).oneshot(authed_get("/api/ping", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_returns_user_and_has_password() {
    let state = make_state().await;
    seed_alice(&state).await;
    let token = login(&state, "alice", "s3cret").await;
    let res = app(state.clone()).oneshot(authed_get("/api/auth/me", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["username"], "alice");
    assert_eq!(v["has_password"], true);
}

#[tokio::test]
async fn logout_invalidates_session() {
    let state = make_state().await;
    seed_alice(&state).await;
    let token = login(&state, "alice", "s3cret").await;
    let res = app(state.clone())
        .oneshot(authed_post_json("/api/auth/logout", &token, ""))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // The token no longer works.
    let res = app(state).oneshot(authed_get("/api/ping", &token)).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn change_password_rejects_wrong_current() {
    let state = make_state().await;
    seed_alice(&state).await;
    let token = login(&state, "alice", "s3cret").await;
    let res = app(state.clone())
        .oneshot(authed_post_json(
            "/api/auth/password",
            &token,
            r#"{"current":"wrong","new":"newpass1"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn change_password_invalidates_other_sessions_but_not_current() {
    let state = make_state().await;
    seed_alice(&state).await;
    // Two independent sessions (e.g. two devices).
    let token_a = login(&state, "alice", "s3cret").await;
    let token_b = login(&state, "alice", "s3cret").await;

    // Change password from device A; current is correct.
    let res = app(state.clone())
        .oneshot(authed_post_json(
            "/api/auth/password",
            &token_a,
            r#"{"current":"s3cret","new":"newpass1"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Device A (current session) still works; device B is invalidated.
    let res_a = app(state.clone()).oneshot(authed_get("/api/ping", &token_a)).await.unwrap();
    assert_eq!(res_a.status(), StatusCode::OK);
    let res_b = app(state.clone()).oneshot(authed_get("/api/ping", &token_b)).await.unwrap();
    assert_eq!(res_b.status(), StatusCode::UNAUTHORIZED);

    // Old password no longer logs in; new one does.
    let res = app(state.clone())
        .oneshot(post_json("/api/auth/login", r#"{"username":"alice","password":"s3cret"}"#))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let res = app(state)
        .oneshot(post_json("/api/auth/login", r#"{"username":"alice","password":"newpass1"}"#))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn assets_reject_without_token_but_accept_query_token() {
    let state = make_state().await;
    seed_alice(&state).await;
    let token = login(&state, "alice", "s3cret").await;

    // No token at all → 401 (gated).
    let res = app(state.clone()).oneshot(get("/assets/missing.png")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Valid ?t= passes the gate; the file doesn't exist so ServeDir 404s — but
    // crucially NOT 401, proving the gate accepted the token.
    let res = app(state.clone())
        .oneshot(get(&format!("/assets/missing.png?t={token}")))
        .await
        .unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bootstrap_env_creates_user_and_is_idempotent() {
    let storage = mem_storage().await;
    assert_eq!(storage.count_users().await.unwrap(), 0);

    let creds = BootstrapCreds {
        user: Some("alice".into()),
        password: Some("s3cret".into()),
        interactive: true,
    };
    let first = ensure_bootstrap_user(&storage, &creds).await.unwrap();
    assert!(first.is_none(), "env-provided creds are not 'generated'");
    assert_eq!(storage.count_users().await.unwrap(), 1);
    let alice = storage.get_user_by_username("alice").await.unwrap().unwrap();
    assert!(shirita_core::verify_password("s3cret", &alice.password_hash));

    // Idempotent: a second call must not clobber.
    ensure_bootstrap_user(&storage, &creds).await.unwrap();
    assert_eq!(storage.count_users().await.unwrap(), 1);
}

#[tokio::test]
async fn bootstrap_web_autogen_returns_password() {
    let storage = mem_storage().await;
    let creds = ensure_bootstrap_user(
        &storage,
        &BootstrapCreds { user: None, password: None, interactive: true },
    )
    .await
    .unwrap()
    .expect("web autogen returns generated creds");
    assert_eq!(creds.username, "admin");
    let admin = storage.get_user_by_username("admin").await.unwrap().unwrap();
    assert!(shirita_core::verify_password(&creds.password, &admin.password_hash));
}
