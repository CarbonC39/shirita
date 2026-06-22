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
async fn create_session_persists_avatar() {
    let state = test_state().await;
    let (st, s) = send(&state, "POST", "/api/sessions", Some(r#"{"name":"s","avatar":"face.png"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    let created: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(created["avatar"], "face.png");
    // and it survives a round-trip through the session read
    let sid = created["id"].as_str().unwrap();
    let (_, got) = send(&state, "GET", &format!("/api/sessions/{sid}"), None).await;
    assert_eq!(serde_json::from_str::<serde_json::Value>(&got).unwrap()["avatar"], "face.png");
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

#[tokio::test]
async fn identity_resolves_char_name_and_session_avatar() {
    let state = test_state().await;
    let (_, c) = send(&state, "POST", "/api/definitions", Some(r#"{"type":"char","name":"Neo","content":"desc"}"#)).await;
    let cid = serde_json::from_str::<serde_json::Value>(&c).unwrap()["id"].as_str().unwrap().to_string();
    let (_, p) = send(&state, "POST", "/api/definitions", Some(r#"{"type":"persona","name":"Me","content":"","meta":{"avatar":"u.png"}}"#)).await;
    let pid = serde_json::from_str::<serde_json::Value>(&p).unwrap()["id"].as_str().unwrap().to_string();
    let (_, t) = send(&state, "POST", "/api/templates", Some(r#"{"name":"Neo"}"#)).await;
    let tid = serde_json::from_str::<serde_json::Value>(&t).unwrap()["id"].as_str().unwrap().to_string();
    for did in [&cid, &pid] {
        let body = format!(r#"{{"kind":"ref","definition_id":"{did}"}}"#);
        send(&state, "POST", &format!("/api/templates/{tid}/nodes?owner_kind=template"), Some(&body)).await;
    }
    let (_, s) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"chat","template_id":"{tid}","avatar":"face.png"}}"#))).await;
    let sid = serde_json::from_str::<serde_json::Value>(&s).unwrap()["id"].as_str().unwrap().to_string();

    let (st, out) = send(&state, "GET", &format!("/api/sessions/{sid}/identity"), None).await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["assistant"]["name"], "Neo");
    assert_eq!(v["assistant"]["avatar"], "face.png");
    assert_eq!(v["user"]["name"], "Me");
    assert_eq!(v["user"]["avatar"], "u.png");
}

#[tokio::test]
async fn identity_is_null_without_a_template() {
    let state = test_state().await;
    let (_, s) = send(&state, "POST", "/api/sessions", Some(r#"{"name":"free"}"#)).await;
    let sid = serde_json::from_str::<serde_json::Value>(&s).unwrap()["id"].as_str().unwrap().to_string();
    let (st, out) = send(&state, "GET", &format!("/api/sessions/{sid}/identity"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(serde_json::from_str::<serde_json::Value>(&out).unwrap()["assistant"]["name"].is_null());
}

#[tokio::test]
async fn identity_prefers_mounted_pack_binding() {
    let state = test_state().await;
    // A char definition lives inside a pack; the pack binds a display name + avatar.
    let (_, c) = send(&state, "POST", "/api/definitions", Some(r#"{"type":"char","name":"Alice","content":"desc"}"#)).await;
    let cid = serde_json::from_str::<serde_json::Value>(&c).unwrap()["id"].as_str().unwrap().to_string();
    let (_, p) = send(&state, "POST", "/api/packs",
        Some(r#"{"name":"AlicePack","identity":{"display_name":"Alice the Bound","avatar":"pack.png"}}"#)).await;
    let pid = serde_json::from_str::<serde_json::Value>(&p).unwrap()["id"].as_str().unwrap().to_string();
    let body = format!(r#"{{"kind":"ref","definition_id":"{cid}"}}"#);
    send(&state, "POST", &format!("/api/packs/{pid}/nodes?owner_kind=pack"), Some(&body)).await;
    // Session with no template-bound char — only the mounted pack + a session avatar.
    let (_, s) = send(&state, "POST", "/api/sessions",
        Some(&format!(r#"{{"name":"chat","avatar":"face.png","pack_ids":["{pid}"]}}"#))).await;
    let sid = serde_json::from_str::<serde_json::Value>(&s).unwrap()["id"].as_str().unwrap().to_string();

    let (st, out) = send(&state, "GET", &format!("/api/sessions/{sid}/identity"), None).await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["assistant"]["name"], "Alice the Bound"); // pack display_name wins
    assert_eq!(v["assistant"]["avatar"], "pack.png");       // pack avatar over session avatar
}

#[tokio::test]
async fn identity_pack_without_display_name_falls_back_to_char_def() {
    let state = test_state().await;
    let (_, c) = send(&state, "POST", "/api/definitions", Some(r#"{"type":"char","name":"Bob","content":"d"}"#)).await;
    let cid = serde_json::from_str::<serde_json::Value>(&c).unwrap()["id"].as_str().unwrap().to_string();
    let (_, p) = send(&state, "POST", "/api/packs", Some(r#"{"name":"BobPack"}"#)).await; // no identity
    let pid = serde_json::from_str::<serde_json::Value>(&p).unwrap()["id"].as_str().unwrap().to_string();
    let body = format!(r#"{{"kind":"ref","definition_id":"{cid}"}}"#);
    send(&state, "POST", &format!("/api/packs/{pid}/nodes?owner_kind=pack"), Some(&body)).await;
    let (_, s) = send(&state, "POST", "/api/sessions",
        Some(&format!(r#"{{"name":"chat","avatar":"face.png","pack_ids":["{pid}"]}}"#))).await;
    let sid = serde_json::from_str::<serde_json::Value>(&s).unwrap()["id"].as_str().unwrap().to_string();

    let (st, out) = send(&state, "GET", &format!("/api/sessions/{sid}/identity"), None).await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["assistant"]["name"], "Bob");          // falls back to the pack's char def name
    assert_eq!(v["assistant"]["avatar"], "face.png");    // falls back to the session avatar
}

#[tokio::test]
async fn create_session_seeds_avatar_from_mounted_pack_when_not_given() {
    // Bug: ChatCard.vue (the chat-list row) reads session.avatar directly,
    // not the live /identity resolution that ChatView uses — so a session
    // mounting a character pack (e.g. one just brought in by a charcard
    // import) showed no avatar at all in the list unless the user also
    // manually picked an avatar override in NewChatView.
    let state = test_state().await;
    let (_, p) = send(&state, "POST", "/api/packs",
        Some(r#"{"name":"AvatarPack","identity":{"display_name":"Aria","avatar":"aria.png"}}"#)).await;
    let pid = serde_json::from_str::<serde_json::Value>(&p).unwrap()["id"].as_str().unwrap().to_string();

    // No `avatar` field in the request body at all.
    let (st, s) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"chat","pack_ids":["{pid}"]}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    let created: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(created["avatar"], "aria.png", "session.avatar is backfilled from the mounted pack's identity");
}

#[tokio::test]
async fn create_session_explicit_avatar_wins_over_mounted_pack() {
    let state = test_state().await;
    let (_, p) = send(&state, "POST", "/api/packs",
        Some(r#"{"name":"AvatarPack","identity":{"avatar":"aria.png"}}"#)).await;
    let pid = serde_json::from_str::<serde_json::Value>(&p).unwrap()["id"].as_str().unwrap().to_string();

    let (_, s) = send(&state, "POST", "/api/sessions",
        Some(&format!(r#"{{"name":"chat","avatar":"face.png","pack_ids":["{pid}"]}}"#))).await;
    let created: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(created["avatar"], "face.png", "an explicit avatar override is never clobbered by pack seeding");
}
