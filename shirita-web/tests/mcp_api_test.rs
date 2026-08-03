use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mcp_api.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap())
        .await
        .unwrap();
    storage.run_migrations().await.unwrap();
    shirita_web::seed_test_session(&storage).await;
    let storage: Arc<dyn Storage> = Arc::new(storage);
    AppState {
        storage,
        config: Arc::new(Config::new("ignored", "./assets").unwrap()),
        provider: Arc::new(EchoProvider) as Arc<dyn ModelProvider>,
        token_counter: Arc::new(TiktokenCounter::new()) as Arc<dyn TokenCounter>,
        model: "test-model".into(),
        generations: Arc::new(shirita_web::Generations::new()),
        http_client: shirita_web::new_http_client(), authorization: Arc::new(shirita_core::mcp::authorization::AuthorizationBroker::new()),
    }
}

async fn request(state: &AppState, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secret-token");
    let body = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app(state.clone())
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

fn server(id: &str, url: &str, secret: &str) -> Value {
    json!({
        "id": id,
        "name": id,
        "enabled": true,
        "transport": {
            "transport": "streamable_http",
            "url": url,
            "headers": [["x-api-key", secret]]
        },
        "request_timeout_ms": 5000
    })
}

#[tokio::test]
async fn mcp_servers_crud_redacts_secrets_and_merges_updates() {
    let state = test_state().await;

    // Create: a secret header is stored.
    let (status, created) = request(
        &state,
        "POST",
        "/api/mcp/servers",
        Some(server("demo", "http://localhost:8080/mcp", "s3cret")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["has_secret"], true);
    assert_eq!(created["transport"]["headers"][0][1], "", "secret must be redacted");

    // A remote plain-http URL is rejected at save time.
    let (status, _) = request(
        &state,
        "POST",
        "/api/mcp/servers",
        Some(server("remote", "http://example.com/mcp", "x")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // GET returns the redacted view; the stored secret survives a blank update.
    let (_, got) = request(&state, "GET", "/api/mcp/servers/demo", None).await;
    assert_eq!(got["name"], "demo");
    assert_eq!(got["transport"]["headers"][0][1], "");

    let (status, updated) = request(
        &state,
        "PUT",
        "/api/mcp/servers/demo",
        Some(server("demo", "http://localhost:8080/mcp", "")), // blank keeps stored secret
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["has_secret"], true);

    // A non-blank value replaces the secret.
    let (status, updated) = request(
        &state,
        "PUT",
        "/api/mcp/servers/demo",
        Some(server("demo", "http://localhost:8080/mcp", "new-secret")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["has_secret"], true);

    let (_, all) = request(&state, "GET", "/api/mcp/servers", None).await;
    assert_eq!(all.as_array().unwrap().len(), 1);

    let (status, _) = request(&state, "DELETE", "/api/mcp/servers/demo", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = request(&state, "GET", "/api/mcp/servers/demo", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn mcp_server_creation_is_capped() {
    let state = test_state().await;
    for i in 0..16 {
        let (status, _) = request(
            &state,
            "POST",
            "/api/mcp/servers",
            Some(server(&format!("s{i}"), "http://localhost:8080/mcp", "")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "creating server s{i} should succeed");
    }
    let (status, _) = request(
        &state,
        "POST",
        "/api/mcp/servers",
        Some(server("s16", "http://localhost:8080/mcp", "")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "the 17th server must be rejected");
}
