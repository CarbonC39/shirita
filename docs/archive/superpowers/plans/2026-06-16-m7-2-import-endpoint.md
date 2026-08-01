# M7 Plan 2 — 统一导入端点（定义级来源）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现统一导入端点 `POST /api/import`（multipart，按内容 sniff 来源），覆盖**定义级**来源：ST 角色卡 PNG（+头像存 asset）/ JSON、ST 世界书 JSON、Shirita 原创单定义；全局 `on_conflict` 策略（定义判重 name+def_type，overwrite 原地更新不删）+ 结果摘要。

**Architecture:** web 层在 Plan 1 的 `read_card_json`/`parse_portable` + 既有 `charcard_to_defs`/`worldinfo_to_defs` 之上编排：读 multipart 字节 → sniff → 转成 `Vec<Definition>`（PNG 额外存头像 asset 并写 `meta.avatar`）→ 按 `on_conflict` 落库 → 返回 `ImportSummary`。模板 bundle 分支留待 Plan 3 插入（本 plan 中 `shirita.template` 暂落入 400）。

**Tech Stack:** Rust、axum 0.8（Multipart、Query、DefaultBodyLimit）、serde。

**Upstream spec:** `docs/superpowers/specs/2026-06-16-m7-import-export-design.md`（§6 导入端点、§2 头像、§12 不删原则）。

---

## File Structure

- `shirita-web/src/routes/import_export.rs` — **rewrite**：`OnConflict`/`ImportSummary`/`persist_defs`/`save_png_asset`/`import` 统一处理器；旧 `import_charcard`/`import_worldinfo` 改为薄包装转调。
- `shirita-web/src/lib.rs` — **modify**：注册 `POST /api/import`（带 `DefaultBodyLimit`）。
- `shirita-web/tests/import_test.rs` — **create**：multipart 集成测试。

---

## Task 1: 导入处理器骨架 —— `OnConflict` + `ImportSummary` + `persist_defs`

**Files:**
- Modify: `shirita-web/src/routes/import_export.rs`

- [ ] **Step 1: 重写 `import_export.rs` 顶部（类型 + 落库 helper）**

把 `shirita-web/src/routes/import_export.rs` 整体替换为下述内容的**第一部分**（后续 Task 追加 `import` 处理器与薄包装）：

```rust
use axum::extract::{Multipart, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use shirita_core::Definition;

use crate::AppState;

/// 同名冲突的全局策略。
#[derive(Debug, Clone, Copy)]
pub enum OnConflict {
    Skip,
    Overwrite,
    Duplicate,
}

impl OnConflict {
    fn parse(s: Option<&str>) -> Self {
        match s {
            Some("overwrite") => OnConflict::Overwrite,
            Some("duplicate") => OnConflict::Duplicate,
            _ => OnConflict::Skip, // 默认 + 未知
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ImportQuery {
    pub on_conflict: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ImportSummary {
    pub created: Vec<ImportItem>,
    pub skipped: Vec<ImportItem>,
    pub overwritten: Vec<ImportItem>,
}

#[derive(Debug, Serialize)]
pub struct ImportItem {
    pub kind: String,
    pub id: String,
    pub name: String,
}

fn item(kind: &str, id: &str, name: &str) -> ImportItem {
    ImportItem { kind: kind.into(), id: id.into(), name: name.into() }
}

/// 按 name+def_type 判重，依 `on_conflict` 落库定义；累加进 summary。
async fn persist_defs(
    state: &AppState,
    defs: Vec<Definition>,
    oc: OnConflict,
    summary: &mut ImportSummary,
) -> Result<(), StatusCode> {
    let existing = state.storage.list_definitions().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for mut d in defs {
        let dup = existing.iter().find(|e| e.name == d.name && e.def_type == d.def_type).cloned();
        match (dup, oc) {
            (Some(ex), OnConflict::Skip) => summary.skipped.push(item("definition", &ex.id, &ex.name)),
            (Some(ex), OnConflict::Overwrite) => {
                // 原地更新：保留 ex.id，绝不删除（护 ON DELETE SET NULL 引用）。
                d.id = ex.id.clone();
                state.storage.update_definition(&d).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                summary.overwritten.push(item("definition", &d.id, &d.name));
            }
            (_, OnConflict::Duplicate) | (None, _) => {
                state.storage.create_definition(&d).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                summary.created.push(item("definition", &d.id, &d.name));
            }
        }
    }
    Ok(())
}
```

> 说明：`(_, Duplicate) | (None, _)` 覆盖"无冲突一律新建"与"duplicate 强制新建"；`Some(ex)` 的 Skip/Overwrite 单列。`Definition` 实现了 `Clone`，`existing.iter().find(...).cloned()` 可用。

- [ ] **Step 2: 编译占位（暂不接路由，确保片段类型正确）**

Run: `cargo build -p shirita-web`
Expected: 因旧 `import_charcard`/`import_worldinfo` 被删而 `lib.rs` 引用失效 → **会报错**。先不修复，下一 Task 加回处理器与薄包装后再编译。

> 若希望每步可编译：可临时保留旧两个函数在文件末尾；但 Task 2 会重写它们，故此处允许短暂不编译，Task 2 结束时统一编译通过。

---

## Task 2: `import` 处理器（sniff + 头像）+ 薄包装 + 路由

**Files:**
- Modify: `shirita-web/src/routes/import_export.rs`
- Modify: `shirita-web/src/lib.rs`

- [ ] **Step 1: 追加 `save_png_asset` + `import` + 薄包装**

在 `import_export.rs` 末尾追加：

```rust
/// 把整张 PNG 存进 assets 目录并登记 Asset，返回存储文件名（写入定义 meta.avatar）。
async fn save_png_asset(state: &AppState, bytes: &[u8], display: &str) -> Result<String, StatusCode> {
    use std::path::Path as FsPath;
    let stored = format!("{}.png", uuid::Uuid::new_v4());
    let path = FsPath::new(&state.config.assets_dir).join(&stored);
    tokio::fs::create_dir_all(&state.config.assets_dir).await.ok();
    tokio::fs::write(&path, bytes).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let asset = shirita_core::Asset::new(display, stored.clone());
    state.storage.create_asset(&asset).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(stored)
}

/// 读取首个 multipart 字段的字节。
async fn first_field_bytes(mut mp: Multipart) -> Result<Vec<u8>, StatusCode> {
    let field = mp.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)?.ok_or(StatusCode::BAD_REQUEST)?;
    let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(bytes.to_vec())
}

const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// 把一张 ST 角色卡 JSON（可带头像文件名）转成定义列表（char + 内嵌世界书）。
fn card_to_defs(card: &Value, avatar: Option<&str>) -> Vec<Definition> {
    let (mut ch, book) = shirita_core::charcard_to_defs(card);
    if let (Some(av), Some(obj)) = (avatar, ch.meta.as_object_mut()) {
        obj.insert("avatar".into(), json!(av));
    }
    let mut all = vec![ch];
    all.extend(book);
    all
}

/// POST /api/import — multipart 单 `file`。按内容 sniff 来源并落库。
pub async fn import(
    State(state): State<AppState>,
    Query(q): Query<ImportQuery>,
    mp: Multipart,
) -> Result<Json<ImportSummary>, StatusCode> {
    let oc = OnConflict::parse(q.on_conflict.as_deref());
    let bytes = first_field_bytes(mp).await?;
    let mut summary = ImportSummary::default();

    // 1) PNG → ST 角色卡 + 头像。
    if bytes.len() >= 8 && bytes[..8] == PNG_SIG {
        let card = shirita_core::read_card_json(&bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
        let name = card.get("data").and_then(|d| d.get("name")).and_then(|v| v.as_str()).unwrap_or("character");
        let avatar = save_png_asset(&state, &bytes, name).await?;
        persist_defs(&state, card_to_defs(&card, Some(&avatar)), oc, &mut summary).await?;
        return Ok(Json(summary));
    }

    // 2) 否则按 JSON sniff。
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
    match v.get("format").and_then(|f| f.as_str()) {
        Some("shirita.definition") => {
            match shirita_core::parse_portable(&v).map_err(|_| StatusCode::BAD_REQUEST)? {
                shirita_core::PortableDoc::Definition(d) => persist_defs(&state, vec![d], oc, &mut summary).await?,
                _ => return Err(StatusCode::BAD_REQUEST),
            }
        }
        // shirita.template 留待 Plan 3 处理。
        _ => {
            let is_card = v.get("spec").and_then(|s| s.as_str()).map(|s| s.contains("chara_card")).unwrap_or(false)
                || v.get("data").and_then(|d| d.get("name")).is_some()
                || (v.get("name").is_some() && v.get("description").is_some());
            if is_card {
                persist_defs(&state, card_to_defs(&v, None), oc, &mut summary).await?;
            } else if v.get("entries").is_some() {
                persist_defs(&state, shirita_core::worldinfo_to_defs(&v), oc, &mut summary).await?;
            } else {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }
    Ok(Json(summary))
}

/// 兼容薄包装：固定 ST 角色卡 JSON 来源，转调统一落库逻辑（默认 skip）。
pub async fn import_charcard(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<ImportSummary>, StatusCode> {
    let mut summary = ImportSummary::default();
    persist_defs(&state, card_to_defs(&body, None), OnConflict::Skip, &mut summary).await?;
    Ok(Json(summary))
}

/// 兼容薄包装：固定 ST 世界书 JSON 来源。
pub async fn import_worldinfo(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<ImportSummary>, StatusCode> {
    let mut summary = ImportSummary::default();
    persist_defs(&state, shirita_core::worldinfo_to_defs(&body), OnConflict::Skip, &mut summary).await?;
    Ok(Json(summary))
}
```

> `read_card_json`/`parse_portable`/`PortableDoc`/`worldinfo_to_defs`/`charcard_to_defs`/`Asset` 均由 Plan 1 + 既有 core re-export 提供。旧两个端点返回类型由 `Json<Value>` 改为 `Json<ImportSummary>`（更一致）。

- [ ] **Step 2: 注册路由 + body limit**

`shirita-web/src/lib.rs`：
1. 顶部 use 区加：`use axum::extract::DefaultBodyLimit;`（若已有 axum 导入，合并）。
2. 在 `/import/worldinfo`、`/import/charcard` 两行旁，加统一入口：

```rust
        .route("/import", post(routes::import_export::import).layer(DefaultBodyLimit::max(16 * 1024 * 1024)))
```

> 保留既有 `/import/worldinfo`、`/import/charcard` 路由（现指向薄包装）。`post` 已在 use 列表中（既有路由在用）。

- [ ] **Step 3: 编译**

Run: `cargo build -p shirita-web`
Expected: 通过、零警告。

> 若 `ch` 变量出现 `unused_mut`/`unused_variables` 警告：`card_to_defs` 内 `let (mut ch, book)` 的 `mut` 是必须的（`as_object_mut`）。确认无遗留警告。

- [ ] **Step 4: 提交**

```bash
git add shirita-web/src/routes/import_export.rs shirita-web/src/lib.rs
git commit -m "feat(web): unified POST /api/import — sniff ST card(PNG/JSON)/worldinfo/portable definition + conflict policy"
```

---

## Task 3: 导入集成测试

**Files:**
- Create: `shirita-web/tests/import_test.rs`

- [ ] **Step 1: 写测试 harness + PNG 构造 + 用例**

创建 `shirita-web/tests/import_test.rs`：

```rust
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
```

> `base64` 是 core 依赖但测试在 web crate——需在 `shirita-web/Cargo.toml` 的 `[dev-dependencies]` 加 `base64 = "0.22"`（若尚无）。

- [ ] **Step 2: 加 web dev 依赖（如缺）**

检查 `shirita-web/Cargo.toml` 的 `[dev-dependencies]` 是否有 `base64`；无则加 `base64 = "0.22"`。

- [ ] **Step 3: 跑测试**

Run: `cargo test -p shirita-web --test import_test`
Expected: PASS（5 tests）。

- [ ] **Step 4: 回归 + 提交**

Run: `cargo test -p shirita-web`（确认 `import_export_test.rs` 仍绿——其断言旧端点返回值；若旧测试断言 `{"created":N}` 的形状变了需同步更新为 `created` 数组长度）。

> 旧 `import_export_test.rs` 若断言 `v["created"]` 为数字，现改为数组——一并更新该测试断言为 `v["created"].as_array().unwrap().len()`。

```bash
git add shirita-web/tests/import_test.rs shirita-web/Cargo.toml shirita-web/tests/import_export_test.rs
git commit -m "test(web): import endpoint integration — ST card PNG/JSON, portable def, conflict policy"
```

---

## Self-Review Checklist

- **Spec 覆盖**：§6 统一 `POST /api/import` multipart + sniff（PNG/ST JSON/世界书/原创单定义）✓、`on_conflict` 三态 + 定义判重 name+def_type + overwrite 原地不删（Task1 `persist_defs`）✓、body limit（Task2 路由 `DefaultBodyLimit`）✓；§2 PNG 整图存 asset + `meta.avatar`（Task2 `save_png_asset`/`card_to_defs`）✓；旧端点薄包装（Task2）✓。`shirita.template` 明确留待 Plan 3（本 plan 落 400）。
- **Placeholder 扫描**：无 TBD；处理器、helper、测试均完整代码。Task1 Step2 的"短暂不编译"是刻意顺序（Task2 收口编译通过），非占位。
- **类型一致**：`OnConflict`/`ImportQuery`/`ImportSummary`/`ImportItem`/`persist_defs(&AppState,Vec<Definition>,OnConflict,&mut ImportSummary)`/`card_to_defs(&Value,Option<&str>)->Vec<Definition>`/`save_png_asset(&AppState,&[u8],&str)->Result<String>` 全程一致；复用 Plan 1 的 `read_card_json`/`parse_portable`/`PortableDoc`。
- **依赖前置**：依赖 Plan 1（`read_card_json`/`parse_portable`）。模板 bundle 导入在 Plan 3 接入（替换 `shirita.template` 的 400 分支）。
```
