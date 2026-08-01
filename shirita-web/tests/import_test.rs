//! 统一 /api/import：原创单定义/模板/包 bundle + on_conflict + 原生格式拒绝。

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

async fn test_state() -> (AppState, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_path_buf();
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(base.join("import.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    shirita_web::seed_test_session(&storage).await;
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let assets = base.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let config = Arc::new(Config::new("ignored", assets.to_str().unwrap()).unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    let state = AppState { storage, config, provider, token_counter, model: "m".into(), generations: Arc::new(shirita_web::Generations::new()), http_client: shirita_web::new_http_client() };
    (state, assets)
}

/// 用 multipart 提交一段字节作为 `file` 字段，返回 (status, 解析后的 JSON 摘要)。
async fn import_bytes(state: &AppState, query: &str, filename: &str, data: &[u8]) -> (StatusCode, Value) {
    let boundary = "BNDRY";
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    ).as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/import{query}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, v)
}

/// Count `created` summary items of kind "definition" (cards now also create a template).
fn created_defs(v: &Value) -> usize {
    v["created"].as_array().unwrap().iter().filter(|c| c["kind"] == "definition").count()
}

#[tokio::test]
async fn imports_portable_definition() {
    let (state, _) = test_state().await;
    let doc = r#"{"format":"shirita.definition","version":1,"definition":{"type":"persona","name":"Me","content":"a user","meta":{}}}"#;
    let (st, v) = import_bytes(&state, "", "me.json", doc.as_bytes()).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["created"].as_array().unwrap().len(), 1);
    assert!(state.storage.list_definitions().await.unwrap().iter().any(|d| d.def_type == "persona" && d.name == "Me"));
}

#[tokio::test]
async fn conflict_skip_then_overwrite_then_duplicate() {
    // Native definition import: the conflict unit is name+def_type in
    // `persist_defs` — skip keeps the existing row, overwrite updates it in
    // place (preserving its id), duplicate creates a same-named sibling.
    let (state, _) = test_state().await;
    let doc = |v: &str| format!(
        r#"{{"format":"shirita.definition","version":1,"definition":{{"type":"persona","name":"Dup","content":"{v}","meta":{{}}}}}}"#
    );
    // 首次：created 1
    let (_, v1) = import_bytes(&state, "", "d.json", doc("v1").as_bytes()).await;
    assert_eq!(created_defs(&v1), 1);

    // skip：同名同类型跳过，不产生新定义
    let (_, v2) = import_bytes(&state, "?on_conflict=skip", "d.json", doc("v2").as_bytes()).await;
    assert_eq!(v2["skipped"].as_array().unwrap().len(), 1);
    assert_eq!(created_defs(&v2), 0);
    assert_eq!(state.storage.list_definitions().await.unwrap().iter().filter(|d| d.name == "Dup").count(), 1);

    // overwrite：就地更新（id 保留），不新增
    let (_, v3) = import_bytes(&state, "?on_conflict=overwrite", "d.json", doc("v2").as_bytes()).await;
    assert_eq!(v3["overwritten"].as_array().unwrap().len(), 1);
    assert_eq!(created_defs(&v3), 0);
    let after = state.storage.list_definitions().await.unwrap();
    let dups: Vec<_> = after.iter().filter(|d| d.name == "Dup").collect();
    assert_eq!(dups.len(), 1, "overwrite 应更新而非新增");
    assert_eq!(dups[0].content, "v2");

    // duplicate：同名再建新 id
    let (_, v4) = import_bytes(&state, "?on_conflict=duplicate", "d.json", doc("v1").as_bytes()).await;
    assert_eq!(created_defs(&v4), 1);
    let dups: Vec<_> = state.storage.list_definitions().await.unwrap().into_iter().filter(|d| d.name == "Dup").collect();
    assert_eq!(dups.len(), 2, "duplicate 应产生同名共存");
}

#[tokio::test]
async fn rejects_malformed_json() {
    let (state, _) = test_state().await;
    let (st, _) = import_bytes(&state, "", "bad.json", br#"{not json"#).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_arbitrary_png_bytes() {
    // A PNG with no embedded character-card JSON is not a native bundle and
    // must be rejected without creating rows or asset files.
    let (state, assets) = test_state().await;
    let png = [0x89u8, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0, 1, 2, 3];
    let (st, _) = import_bytes(&state, "", "img.png", &png).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert!(state.storage.list_assets(None).await.unwrap().is_empty(), "no asset rows for rejected PNG");
    assert!(std::fs::read_dir(&assets).map(|mut d| d.next().is_none()).unwrap_or(true), "no asset files for rejected PNG");
}

#[tokio::test]
async fn rejects_unknown_json() {
    let (state, _) = test_state().await;
    let (st, _) = import_bytes(&state, "", "x.json", br#"{"random":"blob"}"#).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}
