//! Rolling context summaries: fork copies + remaps the side-band summaries.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("summaries.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "test-model".into(), generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client() }
}

async fn send(state: &AppState, method: &str, uri: &str, body: Option<&str>) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secret-token");
    let body = match body {
        Some(json) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(json.to_string())
        }
        None => Body::empty(),
    };
    let res = app(state.clone()).oneshot(builder.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[tokio::test]
async fn fork_copies_and_remaps_summaries() {
    let state = test_state().await;
    // 建会话
    let sid = {
        let (_, out) = send(&state, "POST", "/api/sessions", Some(r#"{"name":"S"}"#)).await;
        json(&out)["id"].as_str().unwrap().to_string()
    };
    // 发一条消息产生 user+assistant（EchoProvider）
    send(&state, "POST", &format!("/api/sessions/{sid}/messages"), Some(r#"{"text":"hi"}"#)).await;
    let msgs = state.storage.list_messages(&sid).await.unwrap();
    let leaf = msgs.last().unwrap().id.clone();
    // 以该叶子为 cutoff 的摘要
    let sum = shirita_core::models::summary::Summary::new(&sid, &leaf, "[sum] earlier");
    state.storage.create_summary(&sum).await.unwrap();

    // fork
    let (st, out) = send(&state, "POST", &format!("/api/sessions/{sid}/fork"),
        Some(&format!(r#"{{"message_id":"{leaf}"}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    let new_sid = json(&out)["id"].as_str().unwrap().to_string();

    // 新会话应有一条摘要，cutoff 指向新会话里对应的消息
    let copied = state.storage.list_summaries(&new_sid).await.unwrap();
    assert_eq!(copied.len(), 1);
    assert_eq!(copied[0].content, "[sum] earlier");
    let new_msgs = state.storage.list_messages(&new_sid).await.unwrap();
    assert!(new_msgs.iter().any(|m| m.id == copied[0].cutoff_message_id),
        "cutoff 必须重映射到新会话内的消息 id");
}
