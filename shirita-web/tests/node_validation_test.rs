//! 校验提示节点树的「2 层」业务约束在 API 层被强制执行：
//! folder/history 必须挂根；ref 的 parent 只能是 None 或同 owner 的 folder。

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
    let path = dir.path().join("node_validation_test.db");
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

/// 建一个模板，返回其 id。
async fn new_template(state: &AppState) -> String {
    let (st, body) = send(state, "POST", "/api/templates", Some(r#"{"name":"T"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    json(&body)["id"].as_str().unwrap().to_string()
}

/// 建一个 def，返回其 id（用于 ref 节点指向）。
async fn new_def(state: &AppState) -> String {
    let (st, body) = send(
        state,
        "POST",
        "/api/definitions",
        Some(r#"{"type":"char","name":"Neo","content":"x"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    json(&body)["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn folder_with_parent_is_rejected() {
    let state = test_state().await;
    let tid = new_template(&state).await;
    let nodes = format!("/api/templates/{tid}/nodes?owner_kind=template");

    // 先建一个根 folder 当作非法 parent。
    let (st, body) = send(&state, "POST", &nodes, Some(r#"{"kind":"folder","tag":"a"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    let root = json(&body)["id"].as_str().unwrap().to_string();

    // folder 不允许有 parent → 400。
    let (st, _) = send(
        &state,
        "POST",
        &nodes,
        Some(&format!(r#"{{"kind":"folder","tag":"b","parent_id":"{root}"}}"#)),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ref_parent_must_be_a_folder() {
    let state = test_state().await;
    let tid = new_template(&state).await;
    let did = new_def(&state).await;
    let nodes = format!("/api/templates/{tid}/nodes?owner_kind=template");

    // 根 folder + 指向它的 ref（合法）。
    let (_, body) = send(&state, "POST", &nodes, Some(r#"{"kind":"folder","tag":"char"}"#)).await;
    let folder = json(&body)["id"].as_str().unwrap().to_string();
    let (st, body) = send(
        &state,
        "POST",
        &nodes,
        Some(&format!(r#"{{"kind":"ref","parent_id":"{folder}","definition_id":"{did}"}}"#)),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "ref under a root folder is valid: {body}");
    let ref_id = json(&body)["id"].as_str().unwrap().to_string();

    // ref 的 parent 指向另一个 ref（非 folder）→ 400。
    let (st, _) = send(
        &state,
        "POST",
        &nodes,
        Some(&format!(r#"{{"kind":"ref","parent_id":"{ref_id}","definition_id":"{did}"}}"#)),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);

    // ref 的 parent 指向不存在的节点 → 400。
    let (st, _) = send(
        &state,
        "POST",
        &nodes,
        Some(&format!(r#"{{"kind":"ref","parent_id":"ghost","definition_id":"{did}"}}"#)),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ref_at_root_is_allowed() {
    let state = test_state().await;
    let tid = new_template(&state).await;
    let did = new_def(&state).await;
    let nodes = format!("/api/templates/{tid}/nodes?owner_kind=template");

    // parent 为 None 的 ref 合法（挂在根）。
    let (st, _) = send(
        &state,
        "POST",
        &nodes,
        Some(&format!(r#"{{"kind":"ref","definition_id":"{did}"}}"#)),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
}

#[tokio::test]
async fn update_cannot_move_folder_under_a_parent() {
    let state = test_state().await;
    let tid = new_template(&state).await;
    let nodes = format!("/api/templates/{tid}/nodes?owner_kind=template");

    let (_, body) = send(&state, "POST", &nodes, Some(r#"{"kind":"folder","tag":"a"}"#)).await;
    let a = json(&body)["id"].as_str().unwrap().to_string();
    let (_, body) = send(&state, "POST", &nodes, Some(r#"{"kind":"folder","tag":"b"}"#)).await;
    let b = json(&body)["id"].as_str().unwrap().to_string();

    // 把 folder b 移到 folder a 下 → 400（folder 必须挂根）。
    let (st, _) = send(
        &state,
        "PUT",
        &format!("/api/nodes/{b}"),
        Some(&format!(r#"{{"parent_id":"{a}"}}"#)),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_can_explicitly_clear_parent_id_back_to_root() {
    let state = test_state().await;
    let tid = new_template(&state).await;
    let did = new_def(&state).await;
    let nodes = format!("/api/templates/{tid}/nodes?owner_kind=template");

    let (_, body) = send(&state, "POST", &nodes, Some(r#"{"kind":"folder","tag":"char"}"#)).await;
    let folder = json(&body)["id"].as_str().unwrap().to_string();
    let (_, body) = send(
        &state,
        "POST",
        &nodes,
        Some(&format!(r#"{{"kind":"ref","parent_id":"{folder}","definition_id":"{did}"}}"#)),
    )
    .await;
    let ref_id = json(&body)["id"].as_str().unwrap().to_string();
    assert_eq!(json(&body)["parent_id"], serde_json::json!(folder));

    // 显式传 parent_id: null → 清空到根，而不是被 omitted-field 的回退逻辑保留旧值。
    let (st, body) = send(
        &state,
        "PUT",
        &format!("/api/nodes/{ref_id}"),
        Some(r#"{"parent_id":null}"#),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(json(&body)["parent_id"], Value::Null);

    // 不传 parent_id 字段（只改 enabled）→ 保留现有值不变。
    let (_, body) = send(
        &state,
        "PUT",
        &format!("/api/nodes/{ref_id}"),
        Some(r#"{"enabled":false}"#),
    )
    .await;
    assert_eq!(json(&body)["parent_id"], Value::Null);
    assert_eq!(json(&body)["enabled"], serde_json::json!(false));
}
