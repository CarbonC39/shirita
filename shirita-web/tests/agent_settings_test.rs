use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, Session, SqliteStorage, Storage, TiktokenCounter,
    TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent_settings.db");
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
        http_client: shirita_web::new_http_client(),
    }
}

async fn request(
    state: &AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
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

fn settings(enabled: bool, rounds: u64) -> Value {
    json!({
        "enabled": enabled,
        "transport": "auto",
        "enabled_tools": ["shirita.random.choose"],
        "max_rounds": rounds,
        "max_tool_calls": 8,
        "tool_timeout_ms": 5000,
        "show_activity": true,
        "show_user_status": true,
        "max_identical_call_rounds": rounds.min(3),
        "system_prompt": "agent prompt",
        "unfinished_prompt": "continue prompt"
    })
}

#[tokio::test]
async fn global_settings_are_typed_validated_and_resettable() {
    let state = test_state().await;
    let (status, initial) = request(&state, "GET", "/api/agent-settings", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initial["global"]["enabled"], false);
    assert!(initial["limits"]["hard_max_rounds"].as_u64().unwrap() >= 4);
    assert!(initial["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "shirita.run.finish"));

    let (status, updated) = request(
        &state,
        "PUT",
        "/api/agent-settings",
        Some(settings(true, 2)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["effective"]["enabled"], true);
    assert_eq!(updated["global"]["max_rounds"], 2);

    let (status, _) = request(
        &state,
        "PUT",
        "/api/agent-settings",
        Some(settings(true, 999)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, unchanged) = request(&state, "GET", "/api/agent-settings", None).await;
    assert_eq!(unchanged["global"]["max_rounds"], 2);

    let (status, reset) = request(&state, "DELETE", "/api/agent-settings", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reset["global"]["enabled"], false);
    assert_eq!(reset["override"], Value::Null);
}

#[tokio::test]
async fn session_override_inherits_and_does_not_replace_siblings() {
    let state = test_state().await;
    let mut session = Session::new("agent session");
    session.override_config = json!({"local_variables": [{"name": "hp"}]});
    state.storage.create_session(&session).await.unwrap();

    request(
        &state,
        "PUT",
        "/api/agent-settings",
        Some(settings(true, 4)),
    )
    .await;
    let uri = format!("/api/sessions/{}/agent-settings", session.id);
    let (_, inherited) = request(&state, "GET", &uri, None).await;
    assert_eq!(inherited["override"], Value::Null);
    assert_eq!(inherited["effective"]["enabled"], true);

    let (status, custom) = request(&state, "PUT", &uri, Some(settings(false, 2))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(custom["override"]["max_rounds"], 2);
    assert_eq!(custom["effective"]["enabled"], false);
    let stored = state
        .storage
        .get_session(&session.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.override_config["local_variables"][0]["name"], "hp");

    let (status, reset) = request(&state, "DELETE", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reset["override"], Value::Null);
    assert_eq!(reset["effective"]["enabled"], true);
    let stored = state
        .storage
        .get_session(&session.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.override_config["local_variables"][0]["name"], "hp");
}
