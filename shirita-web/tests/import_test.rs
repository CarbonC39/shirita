//! 统一 /api/import：ST 角色卡(PNG/JSON)/世界书 + 原创单定义 + on_conflict。

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use base64::Engine;
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
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let assets = base.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let config = Arc::new(Config::new("ignored", assets.to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    let state = AppState { storage, config, provider, token_counter, model: "m".into(), generations: Arc::new(shirita_web::Generations::new()) };
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

fn png_card(json: &str) -> Vec<u8> {
    let sig = [0x89u8, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
    let b64 = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());
    let mut data = Vec::new();
    data.extend_from_slice(b"chara");
    data.push(0);
    data.extend_from_slice(b64.as_bytes());
    let mut out = sig.to_vec();
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(b"tEXt");
    out.extend_from_slice(&data);
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(b"IEND");
    out.extend_from_slice(&[0, 0, 0, 0]);
    out
}

#[tokio::test]
async fn imports_st_card_json() {
    let (state, _) = test_state().await;
    let card = r#"{"spec":"chara_card_v2","data":{"name":"Neo","description":"The One"}}"#;
    let (st, v) = import_bytes(&state, "", "neo.json", card.as_bytes()).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["created"].as_array().unwrap().len(), 1);
    let defs = state.storage.list_definitions().await.unwrap();
    assert!(defs.iter().any(|d| d.def_type == "char" && d.name == "Neo"));
}

#[tokio::test]
async fn imports_png_card_and_saves_avatar() {
    let (state, assets) = test_state().await;
    let png = png_card(r#"{"spec":"chara_card_v2","data":{"name":"Trinity"}}"#);
    let (st, _v) = import_bytes(&state, "", "trinity.png", &png).await;
    assert_eq!(st, StatusCode::OK);
    let defs = state.storage.list_definitions().await.unwrap();
    let ch = defs.iter().find(|d| d.name == "Trinity").unwrap();
    let avatar = ch.meta.get("avatar").and_then(|v| v.as_str()).unwrap();
    assert!(avatar.ends_with(".png"));
    assert!(assets.join(avatar).exists(), "PNG 整图应存进 assets");
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
    let (state, _) = test_state().await;
    let card = r#"{"data":{"name":"Dup","description":"v1"}}"#;
    // 首次：created
    let (_, v1) = import_bytes(&state, "", "d.json", card.as_bytes()).await;
    assert_eq!(v1["created"].as_array().unwrap().len(), 1);
    // skip：同名跳过
    let (_, v2) = import_bytes(&state, "?on_conflict=skip", "d.json", card.as_bytes()).await;
    assert_eq!(v2["skipped"].as_array().unwrap().len(), 1);
    // overwrite：原地更新，id 不变、数量不增
    let before = state.storage.list_definitions().await.unwrap();
    let id_before = before.iter().find(|d| d.name == "Dup").unwrap().id.clone();
    let card2 = r#"{"data":{"name":"Dup","description":"v2"}}"#;
    let (_, v3) = import_bytes(&state, "?on_conflict=overwrite", "d.json", card2.as_bytes()).await;
    assert_eq!(v3["overwritten"].as_array().unwrap().len(), 1);
    let after = state.storage.list_definitions().await.unwrap();
    let dup = after.iter().find(|d| d.name == "Dup").unwrap();
    assert_eq!(dup.id, id_before, "overwrite 必须保留原 id（不删不换）");
    assert_eq!(dup.content, "v2");
    // duplicate：同名再建新 id
    let (_, v4) = import_bytes(&state, "?on_conflict=duplicate", "d.json", card.as_bytes()).await;
    assert_eq!(v4["created"].as_array().unwrap().len(), 1);
    let dups: Vec<_> = state.storage.list_definitions().await.unwrap().into_iter().filter(|d| d.name == "Dup").collect();
    assert_eq!(dups.len(), 2, "duplicate 应产生同名共存");
}

#[tokio::test]
async fn rejects_unknown_json() {
    let (state, _) = test_state().await;
    let (st, _) = import_bytes(&state, "", "x.json", br#"{"random":"blob"}"#).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}
