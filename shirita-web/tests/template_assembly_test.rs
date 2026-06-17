//! 端到端：带模板的会话经节点树驱动组装，并把本次 user 转发给 provider。
//! 同时覆盖：模板创建自动生成 history 魔法节点；会话引用模板（不深拷贝节点）。

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
    let path = dir.path().join("tmpl_asm_test.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "test-model".into(), generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client() }
}

/// 发一个带 Bearer 的请求，返回 (status, 文本 body)。
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
async fn templated_session_tree_drives_assembly_end_to_end() {
    let state = test_state().await;

    // 1) 建模板（应自动生成一个 history 魔法节点）。
    let (st, body) = send(&state, "POST", "/api/templates", Some(r#"{"name":"T"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    let tid = json(&body)["id"].as_str().unwrap().to_string();

    // 2) 模板节点里应已含一个 kind=history 的节点。
    let (st, body) =
        send(&state, "GET", &format!("/api/templates/{tid}/nodes?owner_kind=template"), None).await;
    assert_eq!(st, StatusCode::OK);
    let nodes = json(&body);
    assert!(
        nodes.as_array().unwrap().iter().any(|n| n["kind"] == "history"),
        "template should auto-create a history node: {body}"
    );

    // 3) 建一个 char 定义。
    let (st, body) = send(
        &state,
        "POST",
        "/api/definitions",
        Some(r#"{"type":"char","name":"Neo","content":"Neo body"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let did = json(&body)["id"].as_str().unwrap().to_string();

    // 4) char 容器 + 指向该定义的 ref。
    let (st, body) = send(
        &state,
        "POST",
        &format!("/api/templates/{tid}/nodes?owner_kind=template"),
        Some(r#"{"kind":"folder","tag":"char"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let fid = json(&body)["id"].as_str().unwrap().to_string();

    let (st, _) = send(
        &state,
        "POST",
        &format!("/api/templates/{tid}/nodes?owner_kind=template"),
        Some(&format!(r#"{{"kind":"ref","parent_id":"{fid}","definition_id":"{did}"}}"#)),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // 5) 建会话，引用模板。
    let (st, body) =
        send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"s","template_id":"{tid}"}}"#)))
            .await;
    assert_eq!(st, StatusCode::OK);
    let sid = json(&body)["id"].as_str().unwrap().to_string();

    // 6) 会话自身无节点（引用而非深拷贝）。
    let (st, body) =
        send(&state, "GET", &format!("/api/templates/{sid}/nodes?owner_kind=session"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(
        json(&body).as_array().unwrap().is_empty(),
        "session should reference template, not own copied nodes: {body}"
    );

    // 7) 发消息：SSE 正常流回，且本次 user 经 history 切分后转发给 provider（echo）。
    let (st, sse) =
        send(&state, "POST", &format!("/api/sessions/{sid}/messages"), Some(r#"{"text":"hi"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    assert!(sse.contains(r#""type":"delta""#), "expected delta events: {sse}");
    assert!(sse.contains(r#""type":"done""#), "expected done event: {sse}");
    // echo 按空格分片（"echo: " / "hi"）；逐块到达即可，完整串在落库消息里断言。
    assert!(sse.contains("echo: "), "expected echo prefix delta: {sse}");
    assert!(sse.contains(r#""text":"hi""#), "live user turn should reach the provider: {sse}");

    // 8) 落库：assistant 回复确为 echo: hi（证明 user 未被树管线丢弃）。
    let (st, body) =
        send(&state, "GET", &format!("/api/sessions/{sid}/messages"), None).await;
    assert_eq!(st, StatusCode::OK);
    let msgs = json(&body);
    let arr = msgs.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["role"], "user");
    assert_eq!(arr[1]["role"], "assistant");
    assert_eq!(arr[1]["raw_content"], "echo: hi");
}
