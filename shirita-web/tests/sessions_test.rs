use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sess_test.db");
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
        model: "test-model".into(),
        generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client(),
    }
}

fn auth(req: Request<Body>) -> Request<Body> {
    let (mut parts, body) = req.into_parts();
    parts
        .headers
        .insert(header::AUTHORIZATION, "Bearer secret-token".parse().unwrap());
    Request::from_parts(parts, body)
}

async fn send(state: &AppState, method: &str, uri: &str, body: Option<&str>) -> (StatusCode, String) {
    let mut b = Request::builder().method(method).uri(uri).header(header::AUTHORIZATION, "Bearer secret-token");
    let body = match body {
        Some(j) => {
            b = b.header(header::CONTENT_TYPE, "application/json");
            Body::from(j.to_string())
        }
        None => Body::empty(),
    };
    let res = app(state.clone()).oneshot(b.body(body).unwrap()).await.unwrap();
    let st = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (st, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn create_session_seeds_first_message_with_anchor() {
    let state = test_state().await;
    // a first_message definition + a template that references it
    let fm = r#"{"type":"first_message","name":"g","content":"wake up","meta":{"alternate_greetings":["again"]}}"#;
    let (_, d) = send(&state, "POST", "/api/definitions", Some(fm)).await;
    let did = serde_json::from_str::<serde_json::Value>(&d).unwrap()["id"].as_str().unwrap().to_string();
    let (_, t) = send(&state, "POST", "/api/templates", Some(r#"{"name":"T"}"#)).await;
    let tid = serde_json::from_str::<serde_json::Value>(&t).unwrap()["id"].as_str().unwrap().to_string();
    let body = format!(r#"{{"kind":"ref","definition_id":"{did}"}}"#);
    send(&state, "POST", &format!("/api/templates/{tid}/nodes?owner_kind=template"), Some(&body)).await;

    let (st, s) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"s","template_id":"{tid}"}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    let sid = serde_json::from_str::<serde_json::Value>(&s).unwrap()["id"].as_str().unwrap().to_string();

    let (_, msgs) = send(&state, "GET", &format!("/api/sessions/{sid}/messages"), None).await;
    let msgs: serde_json::Value = serde_json::from_str(&msgs).unwrap();
    let arr = msgs.as_array().unwrap();
    // anchor user + 2 assistants (main + alternate)
    let anchor = arr.iter().find(|m| m["role"] == "user" && m["is_anchor"] == true).unwrap();
    let assistants: Vec<_> = arr.iter().filter(|m| m["role"] == "assistant").collect();
    assert_eq!(assistants.len(), 2);
    // both assistants hang off the anchor (they are swipes)
    for a in &assistants {
        assert_eq!(a["parent_id"], anchor["id"]);
    }
    assert!(assistants.iter().any(|a| a["raw_content"] == "wake up"));
    assert!(assistants.iter().any(|a| a["raw_content"] == "again"));
}

#[tokio::test]
async fn create_then_list_session() {
    let state = test_state().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/sessions")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"Chat A"}"#))
        .unwrap();
    let res = app(state.clone()).oneshot(auth(req)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(created["name"], "Chat A");
    assert!(created["id"].as_str().is_some());

    let req = Request::builder()
        .uri("/api/sessions")
        .body(Body::empty())
        .unwrap();
    let res = app(state).oneshot(auth(req)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
}
