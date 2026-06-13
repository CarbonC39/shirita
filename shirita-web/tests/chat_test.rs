use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, Session, SqliteStorage, Storage, TiktokenCounter,
    TokenCounter,
};
use shirita_web::{app, AppState};

async fn state_with_session() -> (AppState, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_test.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let session = Session::new("c");
    storage.create_session(&session).await.unwrap();

    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    let state = AppState {
        storage,
        config,
        provider,
        token_counter,
        model: "m".into(),
    };
    (state, session.id)
}

#[tokio::test]
async fn send_streams_echo_and_persists() {
    let (state, session_id) = state_with_session().await;

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{session_id}/messages"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .body(Body::from(r#"{"text":"hello"}"#))
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 收集整段 SSE body，断言含 echo 分片与 done。
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains(r#""type":"delta""#),
        "should contain delta events: {text}"
    );
    assert!(text.contains("echo:"), "should echo the input: {text}");
    assert!(
        text.contains(r#""type":"done""#),
        "should end with done: {text}"
    );

    // 落库校验。
    let req = Request::builder()
        .uri(format!("/api/sessions/{session_id}/messages"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .body(Body::empty())
        .unwrap();
    let res = app(state).oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let msgs: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = msgs.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["role"], "user");
    assert_eq!(arr[1]["role"], "assistant");
    assert_eq!(arr[1]["raw_content"], "echo: hello");
}
