//! 导出端点 + 原创格式 round-trip + 模板 bundle 还原。

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use shirita_core::{
    Config, Definition, EchoProvider, ModelProvider, OwnerKind, PromptNode, SqliteStorage, Storage,
    Template, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_path_buf();
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(base.join("export.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let assets = base.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let config = Arc::new(Config::new("ignored", assets.to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "m".into(), generations: Arc::new(shirita_web::Generations::new()) }
}

async fn get(state: &AppState, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .body(Body::empty())
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// 把 JSON 文本作为 multipart `file` 提交到 /api/import。
async fn import_json(state: &AppState, query: &str, json: &Value) -> (StatusCode, Value) {
    let boundary = "BNDRY";
    let payload = serde_json::to_vec(json).unwrap();
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"b.json\"\r\nContent-Type: application/json\r\n\r\n"
    ).as_bytes());
    body.extend_from_slice(&payload);
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
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn definition_export_then_reimport_roundtrip() {
    let state = test_state().await;
    let mut d = Definition::new("char", "Neo", "The One");
    d.meta = serde_json::json!({ "wrap_in_tag": true });
    state.storage.create_definition(&d).await.unwrap();

    let (st, bundle) = get(&state, &format!("/api/definitions/{}/export", d.id)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(bundle["format"], "shirita.definition");

    // 删掉原定义再导入，验证还原。
    state.storage.delete_definition(&d.id).await.unwrap();
    let (st2, summary) = import_json(&state, "?on_conflict=duplicate", &bundle).await;
    assert_eq!(st2, StatusCode::OK);
    assert_eq!(summary["created"].as_array().unwrap().len(), 1);
    let got = state.storage.list_definitions().await.unwrap();
    let neo = got.iter().find(|x| x.name == "Neo").unwrap();
    assert_eq!(neo.content, "The One");
    assert_eq!(neo.meta["wrap_in_tag"], true);
}

#[tokio::test]
async fn template_export_enabled_part_then_restore() {
    let state = test_state().await;
    // 模板：folder(char) > ref A(enabled)；disabled folder > ref B
    let t = Template::new("Preset");
    state.storage.create_template(&t).await.unwrap();
    let a = Definition::new("char", "A", "aa");
    let b = Definition::new("world", "B", "bb");
    state.storage.create_definition(&a).await.unwrap();
    state.storage.create_definition(&b).await.unwrap();
    let fa = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "char");
    let ra = PromptNode::new_ref(OwnerKind::Template, &t.id, Some(fa.id.clone()), 0, &a.id);
    let mut fb = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 1, "world");
    fb.enabled = false;
    let rb = PromptNode::new_ref(OwnerKind::Template, &t.id, Some(fb.id.clone()), 0, &b.id);
    for n in [&fa, &ra, &fb, &rb] {
        state.storage.create_node(n).await.unwrap();
    }

    let (st, bundle) = get(&state, &format!("/api/templates/{}/export", t.id)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(bundle["format"], "shirita.template");
    // 只含启用部分：2 节点（fa+ra）、1 定义（A）。
    assert_eq!(bundle["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(bundle["definitions"].as_array().unwrap().len(), 1);

    // 还原（duplicate）：新模板 + 节点 + 定义。
    let tmpl_before = state.storage.list_templates().await.unwrap().len();
    let (st2, summary) = import_json(&state, "?on_conflict=duplicate", &bundle).await;
    assert_eq!(st2, StatusCode::OK);
    assert_eq!(summary["created"][0]["kind"], "template");
    let templates = state.storage.list_templates().await.unwrap();
    assert_eq!(templates.len(), tmpl_before + 1, "应新建一个模板");
    let new_t = templates.iter().find(|x| x.id != t.id && x.name == "Preset").unwrap();
    let new_nodes = state.storage.list_nodes(&OwnerKind::Template, &new_t.id).await.unwrap();
    assert_eq!(new_nodes.len(), 2, "还原启用部分的 2 个节点");
    // ref 节点的 definition_id 应指向新建的定义（非原 a.id）。
    let new_ref = new_nodes.iter().find(|n| n.definition_id.is_some()).unwrap();
    assert_ne!(new_ref.definition_id.as_deref(), Some(a.id.as_str()), "definition_id 应重映射到新定义");
}

#[tokio::test]
async fn template_import_skip_keeps_existing_untouched() {
    let state = test_state().await;
    let t = Template::new("Same");
    state.storage.create_template(&t).await.unwrap();
    let bundle = serde_json::json!({
        "format": "shirita.template", "version": 1,
        "template": { "name": "Same", "meta": {} },
        "nodes": [], "definitions": []
    });
    let (st, summary) = import_json(&state, "?on_conflict=skip", &bundle).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(summary["skipped"].as_array().unwrap().len(), 1);
    // 原模板仍在、未新增。
    let templates = state.storage.list_templates().await.unwrap();
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].id, t.id);
}
