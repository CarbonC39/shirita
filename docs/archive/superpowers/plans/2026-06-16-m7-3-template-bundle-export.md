# M7 Plan 3 — 模板 bundle 导入 + 导出端点 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 补齐 Shirita 原创格式的两端：导入 `shirita.template` bundle（原子还原模板 + 启用节点树 + 定义，local_id 重映射为新 UUID），以及导出端点 `GET /api/definitions/{id}/export`、`GET /api/templates/{id}/export`（产原创 JSON、附下载头）。

**Architecture:** 导入侧在 Plan 2 的 `import` 处理器 `match format` 里补 `shirita.template` 分支，调 `import_template_bundle`（按模板名 skip/否则全新建，绝不删旧模板——护惰性 Fork）。导出侧新建 `routes/export.rs` 调 Plan 1 的 `export_definition`/`export_template`。

**Tech Stack:** Rust、axum 0.8、chrono、uuid。无新迁移。

**Upstream spec:** `docs/superpowers/specs/2026-06-16-m7-import-export-design.md`（§7 模板还原、§8 导出端点、§6 模板冲突）。

---

## File Structure

- `shirita-web/src/routes/import_export.rs` — **modify**：加 `import_template_bundle` + 在 `import` 的 `match` 补 `shirita.template` 分支。
- `shirita-web/src/routes/export.rs` — **create**：`export_definition`/`export_template` 处理器。
- `shirita-web/src/routes/mod.rs` — **modify**：`pub mod export;`。
- `shirita-web/src/lib.rs` — **modify**：注册两个导出路由。
- `shirita-web/tests/export_test.rs` — **create**：导出 + round-trip + 模板还原测试。

---

## Task 1: 模板 bundle 导入还原

**Files:**
- Modify: `shirita-web/src/routes/import_export.rs`

- [ ] **Step 1: 补 imports**

`import_export.rs` 顶部 use 区补：

```rust
use std::collections::HashMap;

use shirita_core::{NodeKind, OwnerKind, PromptNode, Template};
```

（与既有 `use shirita_core::Definition;` 合并为 `use shirita_core::{Definition, NodeKind, OwnerKind, PromptNode, Template};`。）

- [ ] **Step 2: 加 `import_template_bundle`**

在 `import_export.rs` 末尾追加：

```rust
/// 还原 shirita.template bundle：bundle 为原子单位，按模板名决策。
/// skip（存在且 Skip）→ 整 bundle 跳过；否则全新建（模板+定义+节点，local_id 重映射为新 UUID）。
/// **绝不删除现有模板**（护 M4 惰性 Fork：未 materialize 会话直接引用模板节点）。
async fn import_template_bundle(
    state: &AppState,
    v: &Value,
    oc: OnConflict,
    summary: &mut ImportSummary,
) -> Result<(), StatusCode> {
    let doc = shirita_core::parse_portable(v).map_err(|_| StatusCode::BAD_REQUEST)?;
    let (name, meta, nodes, defs) = match doc {
        shirita_core::PortableDoc::Template { name, meta, nodes, defs } => (name, meta, nodes, defs),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    // 模板冲突：仅 Skip 时跳过同名；overwrite 对模板等同 duplicate（绝不删旧模板）。
    if matches!(oc, OnConflict::Skip) {
        let templates = state.storage.list_templates().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(ex) = templates.iter().find(|t| t.name == name) {
            summary.skipped.push(item("template", &ex.id, &ex.name));
            return Ok(());
        }
    }

    // 1) 新建模板。
    let mut tmpl = Template::new(&name);
    tmpl.meta = meta;
    state.storage.create_template(&tmpl).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 2) 新建定义，建 local_id -> 新定义 id 映射（bundle 内定义随模板原子新建，不按 name+type 去重）。
    let mut def_map: HashMap<String, String> = HashMap::new();
    for pd in &defs {
        let mut d = Definition::new(&pd.def_type, &pd.name, &pd.content);
        d.meta = pd.meta.clone();
        state.storage.create_definition(&d).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        def_map.insert(pd.local_id.clone(), d.id.clone());
    }

    // 3) 预分配节点新 UUID（供 parent 重指）。
    let node_map: HashMap<String, String> =
        nodes.iter().map(|n| (n.local_id.clone(), uuid::Uuid::new_v4().to_string())).collect();

    for pn in &nodes {
        // ref 的 definition_id 经 def_map 重指；缺失则跳过该节点 + warn。
        let definition_id = match (&pn.kind, &pn.def_local_id) {
            (NodeKind::Ref, Some(dl)) => match def_map.get(dl) {
                Some(real) => Some(real.clone()),
                None => {
                    tracing::warn!(local_id = %pn.local_id, "template import: ref def_local_id missing, skipping node");
                    continue;
                }
            },
            _ => None,
        };
        let node = PromptNode {
            id: node_map[&pn.local_id].clone(),
            owner_kind: OwnerKind::Template,
            owner_id: tmpl.id.clone(),
            parent_id: pn.parent_local_id.as_ref().and_then(|p| node_map.get(p)).cloned(),
            sort_order: pn.sort_order,
            kind: pn.kind.clone(),
            tag: pn.tag.clone(),
            definition_id,
            enabled: pn.enabled,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        state.storage.create_node(&node).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    summary.created.push(item("template", &tmpl.id, &tmpl.name));
    Ok(())
}
```

- [ ] **Step 3: 在 `import` 的 `match` 接入分支**

在 `import` 处理器里 `match v.get("format").and_then(|f| f.as_str()) {` 的 `Some("shirita.definition") => {...}` 之后、`_ =>` 之前，插入：

```rust
        Some("shirita.template") => import_template_bundle(&state, &v, oc, &mut summary).await?,
```

- [ ] **Step 4: 编译**

Run: `cargo build -p shirita-web`
Expected: 通过、零警告。

- [ ] **Step 5: 提交**

```bash
git add shirita-web/src/routes/import_export.rs
git commit -m "feat(web): import shirita.template bundle — atomic restore, never deletes existing template"
```

---

## Task 2: 导出端点

**Files:**
- Create: `shirita-web/src/routes/export.rs`
- Modify: `shirita-web/src/routes/mod.rs`
- Modify: `shirita-web/src/lib.rs`

- [ ] **Step 1: 写 `export.rs`**

创建 `shirita-web/src/routes/export.rs`：

```rust
use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;

use shirita_core::{Definition, OwnerKind};

use crate::AppState;

/// 文件名安全化：仅保留字母数字/`-`/`_`，其余转 `_`。
fn safe_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if s.is_empty() { "export".into() } else { s }
}

/// GET /api/definitions/{id}/export — 单定义原创 JSON（附下载头）。
pub async fn export_definition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let def = state
        .storage
        .get_definition(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let v = shirita_core::export_definition(&def);
    let cd = format!("attachment; filename=\"{}.json\"", safe_filename(&def.name));
    Ok(([(header::CONTENT_DISPOSITION, cd)], Json(v)))
}

/// GET /api/templates/{id}/export — 模板「启用部分」原创 JSON（附下载头）。
pub async fn export_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let tmpl = state
        .storage
        .get_template(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let nodes = state
        .storage
        .list_nodes(&OwnerKind::Template, &id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let all = state.storage.list_definitions().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let defs: HashMap<String, Definition> = all.into_iter().map(|d| (d.id.clone(), d)).collect();
    let v = shirita_core::export_template(&tmpl, &nodes, &defs);
    let cd = format!("attachment; filename=\"{}.json\"", safe_filename(&tmpl.name));
    Ok(([(header::CONTENT_DISPOSITION, cd)], Json(v)))
}
```

- [ ] **Step 2: 注册模块 + 路由**

1. `shirita-web/src/routes/mod.rs` 加 `pub mod export;`。
2. `shirita-web/src/lib.rs` 在 definitions / templates 路由附近加：

```rust
        .route("/definitions/{id}/export", get(routes::export::export_definition))
        .route("/templates/{id}/export", get(routes::export::export_template))
```

> `get` 已在 use 列表（既有路由在用）。

- [ ] **Step 3: 编译 + 提交**

Run: `cargo build -p shirita-web`
Expected: 通过、零警告。

```bash
git add shirita-web/src/routes/export.rs shirita-web/src/routes/mod.rs shirita-web/src/lib.rs
git commit -m "feat(web): export endpoints — definition + template(enabled) as portable JSON"
```

---

## Task 3: 导出 / round-trip / 模板还原 集成测试

**Files:**
- Create: `shirita-web/tests/export_test.rs`

- [ ] **Step 1: 写测试**

创建 `shirita-web/tests/export_test.rs`：

```rust
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
```

- [ ] **Step 2: 跑测试**

Run: `cargo test -p shirita-web --test export_test`
Expected: PASS（3 tests）。

- [ ] **Step 3: 全量回归 + 提交**

Run: `cargo test --workspace`
Expected: 全绿、零警告。

```bash
git add shirita-web/tests/export_test.rs
git commit -m "test(web): export endpoints + portable round-trip + template bundle restore"
```

---

## Self-Review Checklist

- **Spec 覆盖**：§7 模板 bundle 原子还原（skip/duplicate、local_id→新 UUID 重映射、悬空 ref 跳过+warn、绝不删旧模板）（Task1）✓；§8 `GET …/definitions/{id}/export`、`GET …/templates/{id}/export` + 下载头（Task2）✓；§6 模板冲突 skip 保留原模板（Task3 测试守）✓。
- **Placeholder 扫描**：无 TBD；处理器、helper、测试均完整代码。
- **类型一致**：`import_template_bundle(&AppState,&Value,OnConflict,&mut ImportSummary)`、`PortableDoc::Template{name,meta,nodes,defs}`（Plan 1 定义）、`PromptNode` 结构体字面量字段与模型一致、`export_definition`/`export_template`（Plan 1）签名一致。
- **依赖前置**：依赖 Plan 1（`export_*`/`parse_portable`/`PortableDoc`）与 Plan 2（`import` 处理器 + `OnConflict`/`ImportSummary`/`item`/`Definition` use）。
```
