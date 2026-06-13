# M2 — 定义体系与上下文组装 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把"万物皆定义"落地：全类型 definitions REST CRUD、资源上传/静态服务、真正的 prompt 组装流水线（挂载→局部覆盖→`{{var}}`→XML 封包），以及 `regex_rule` 清洗生成 `display_content`。完成后：给会话挂载角色+世界书，发消息时这些定义真实出现在请求里；上传的资源能按 URL 取回。

**Architecture:** 挂载用"有序 ID 列表"——`chat_sessions` 新增 `mounted_definitions`(JSON 数组)。新增 `shirita-core/src/assembly.rs`（纯函数：变量渲染、局部覆盖、XML 封包、regex 清洗）。`send_message` 在调用 provider 前组装 system 消息、在落库 assistant 时写 `display_content`。Web 新增 definitions/assets/mounts 路由 + `ServeDir` 静态服务。

**Tech Stack:** 在 M0/M1 基础上新增：`regex`（core）、axum `multipart` 特性、tower-http `fs` 特性、uuid（web）。沿用 sqlx 运行时 API。

---

## 前置说明（实现者必读）

- 挂载模型 = **会话持有有序 definition ID 列表**（`mounted_definitions`）。改挂载 = 整体替换该数组（`PUT /api/sessions/{id}/mounts`）。
- 组装只纳入"内容型"定义（`char/persona/world/item/prompt`），**排除** `regex_rule`（用于输出清洗）与 `tool`（M5 工具调用）。
- 局部覆盖只读：组装时若 `override_config.local_definitions[<id>]` 存在则用其替换全局 content。写侧（编辑落入局部覆盖、"同步至全局"）属 M4，本里程碑不做。
- 全程 `main` 分支，每 Task 末提交；先测后实现；构建开销大时每 Task 末统一 `cargo test`。
- 向后兼容：M1 的会话默认 `mounted_definitions=[]` → 组装出空 system → 不加 system 消息 → EchoProvider 行为不变，M1 测试仍绿。

## 文件结构（创建/修改）

```
shirita-core/
├── Cargo.toml                                # +regex
├── migrations/0003_session_mounts.sql        # 创建
└── src/
    ├── lib.rs                                 # 导出 assembly
    ├── models/session.rs                      # +mounted_definitions
    ├── storage/mod.rs                         # +set_mounted_definitions
    ├── storage/sqlite.rs                      # 列映射 + INSERT + set + 测试
    ├── assembly.rs                            # 创建：render_vars/封包/regex 清洗
    └── conversation.rs                        # send_message 接入组装 + display_content
shirita-web/
├── Cargo.toml                                # axum multipart, tower-http fs, +uuid
└── src/
    ├── lib.rs                                 # 挂载新路由 + /assets ServeDir
    ├── main.rs                               # 启动时确保 assets 目录存在
    └── routes/
        ├── mod.rs                             # 挂载 definitions/assets
        ├── definitions.rs                     # 创建：CRUD
        ├── assets.rs                          # 创建：resolve_asset_url + upload
        └── sessions.rs                        # +set_mounts 处理器
shirita-web/tests/
├── definitions_test.rs                        # 创建
└── assets_mounts_test.rs                      # 创建
```

---

## Task 1: 会话挂载（migration 0003 + 模型 + 存储）（TDD）

**Files:** `shirita-core/migrations/0003_session_mounts.sql`(create), `shirita-core/src/models/session.rs`, `shirita-core/src/storage/mod.rs`, `shirita-core/src/storage/sqlite.rs`

- [ ] **Step 1: 迁移 `0003_session_mounts.sql`**
```sql
ALTER TABLE chat_sessions ADD COLUMN mounted_definitions TEXT NOT NULL DEFAULT '[]';
```

- [ ] **Step 2: Session 加字段（`models/session.rs`）**
struct 增加（放在 `current_state` 之后）：
```rust
    #[serde(default)]
    pub mounted_definitions: Vec<String>,
```
`Session::new` 的返回体增加：
```rust
            mounted_definitions: Vec::new(),
```

- [ ] **Step 3: Storage trait 增方法（`storage/mod.rs`，在 messages 区前/后）**
```rust
    /// 整体替换会话的挂载定义 ID 列表。
    async fn set_mounted_definitions(&self, session_id: &str, ids: &[String]) -> Result<()>;
```

- [ ] **Step 4: 写失败测试（`sqlite.rs` tests，追加）**
```rust
    #[tokio::test]
    async fn session_mounts_roundtrip() {
        let storage = temp_storage().await;
        let mut s = Sess::new("m");
        s.mounted_definitions = vec!["a".into(), "b".into()];
        storage.create_session(&s).await.unwrap();
        assert_eq!(storage.get_session(&s.id).await.unwrap().unwrap().mounted_definitions, vec!["a", "b"]);

        storage.set_mounted_definitions(&s.id, &["x".into()]).await.unwrap();
        assert_eq!(storage.get_session(&s.id).await.unwrap().unwrap().mounted_definitions, vec!["x"]);
    }
```

- [ ] **Step 5: 实现（`sqlite.rs`）**
`row_to_session` 增加读取（在构造 Session 前）：
```rust
    let mounted: String = row.try_get("mounted_definitions")?;
```
并在 `Ok(Session { ... })` 内增加：
```rust
        mounted_definitions: serde_json::from_str(&mounted)?,
```
`create_session`：INSERT 列与值都加 `mounted_definitions`：
```rust
        sqlx::query(
            "INSERT INTO chat_sessions (id, name, avatar, override_config, current_state, mounted_definitions) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        // ...原有 5 个 bind 之后追加：
        .bind(serde_json::to_string(&session.mounted_definitions)?)
```
`get_session`/`list_sessions` 的 SELECT 增加 `, mounted_definitions` 列。
在 `impl Storage` 内新增：
```rust
    async fn set_mounted_definitions(&self, session_id: &str, ids: &[String]) -> Result<()> {
        let json = serde_json::to_string(ids)?;
        sqlx::query("UPDATE chat_sessions SET mounted_definitions = ? WHERE id = ?")
            .bind(json)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```

- [ ] **Step 6: 测试通过** — Run: `cargo test -p shirita-core sqlite::` → 全绿（含 session_mounts_roundtrip）。
- [ ] **Step 7: 提交** — `git commit -am "feat(m2): session mounted_definitions (migration + storage)"`

---

## Task 2: 组装模块 assembly.rs（TDD，纯函数）

**Files:** `shirita-core/Cargo.toml`(+regex), `shirita-core/src/assembly.rs`(create), `shirita-core/src/lib.rs`

- [ ] **Step 1: core Cargo.toml `[dependencies]` 加** `regex = "1"`

- [ ] **Step 2: 创建 `assembly.rs`（含测试）**
```rust
//! Prompt 组装：局部覆盖 → 变量渲染 → XML 封包；以及 regex_rule 输出清洗。

use crate::models::definition::{Definition, DefinitionType};

/// 取定义的"有效内容"：若 local_overrides 含该 id 则用覆盖文本，否则用全局 content。
fn effective_content(def: &Definition, local_overrides: &serde_json::Value) -> String {
    local_overrides
        .get(&def.id)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| def.content.clone())
}

/// 用 state 渲染 `{{var}}`；未知键保留原占位符。
pub fn render_vars(content: &str, state: &serde_json::Value) -> String {
    let re = regex::Regex::new(r"\{\{\s*([A-Za-z0-9_]+)\s*\}\}").unwrap();
    re.replace_all(content, |caps: &regex::Captures| {
        let key = &caps[1];
        match state.get(key) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Number(n)) => n.to_string(),
            Some(serde_json::Value::Bool(b)) => b.to_string(),
            _ => caps[0].to_string(),
        }
    })
    .into_owned()
}

/// type → 封包标签；返回 None 表示不进 system（regex_rule/tool）。
fn wrap_tag(t: &DefinitionType) -> Option<&'static str> {
    match t {
        DefinitionType::Persona => Some("personas"),
        DefinitionType::Char => Some("characters"),
        DefinitionType::World => Some("world_rules"),
        DefinitionType::Item => Some("items"),
        DefinitionType::Prompt => Some("prompts"),
        DefinitionType::RegexRule | DefinitionType::Tool => None,
    }
}

/// 组装 system 文本：按固定 type 顺序分组，每组用 `<tag>…</tag>` 包裹，组内按挂载顺序拼接。
pub fn assemble_system_prompt(
    mounted: &[Definition],
    local_overrides: &serde_json::Value,
    state: &serde_json::Value,
) -> String {
    // 固定分组顺序。
    let order = [
        DefinitionType::Persona,
        DefinitionType::Char,
        DefinitionType::World,
        DefinitionType::Item,
        DefinitionType::Prompt,
    ];
    let mut blocks: Vec<String> = Vec::new();
    for group in order {
        let tag = wrap_tag(&group).unwrap();
        let bodies: Vec<String> = mounted
            .iter()
            .filter(|d| d.def_type == group)
            .map(|d| render_vars(&effective_content(d, local_overrides), state))
            .collect();
        if !bodies.is_empty() {
            blocks.push(format!("<{tag}>\n{}\n</{tag}>", bodies.join("\n")));
        }
    }
    blocks.join("\n")
}

/// 依挂载顺序对文本应用 regex_rule（meta: {pattern, replacement}）。无规则返回 None。
pub fn apply_regex_rules(text: &str, rules: &[Definition]) -> Option<String> {
    if rules.is_empty() {
        return None;
    }
    let mut out = text.to_string();
    for rule in rules {
        let pattern = rule.meta.get("pattern").and_then(|v| v.as_str());
        let replacement = rule.meta.get("replacement").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(p) = pattern {
            if let Ok(re) = regex::Regex::new(p) {
                out = re.replace_all(&out, replacement).into_owned();
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::definition::{Definition, DefinitionType};
    use serde_json::json;

    fn def(t: DefinitionType, name: &str, content: &str) -> Definition {
        Definition::new(t, name, content)
    }

    #[test]
    fn render_vars_known_and_unknown() {
        let s = json!({ "name": "Alice", "hp": 80 });
        assert_eq!(render_vars("Hi {{name}}, hp={{hp}} {{missing}}", &s), "Hi Alice, hp=80 {{missing}}");
    }

    #[test]
    fn assemble_groups_in_order_with_tags() {
        let mounted = vec![
            def(DefinitionType::World, "w", "rule1"),
            def(DefinitionType::Char, "c", "I am {{name}}"),
            def(DefinitionType::RegexRule, "r", "ignored"),
        ];
        let out = assemble_system_prompt(&mounted, &json!({}), &json!({ "name": "Bob" }));
        // characters 在 world_rules 之前，且 regex_rule 被排除。
        assert!(out.contains("<characters>\nI am Bob\n</characters>"));
        assert!(out.contains("<world_rules>\nrule1\n</world_rules>"));
        assert!(out.find("<characters>").unwrap() < out.find("<world_rules>").unwrap());
        assert!(!out.contains("ignored"));
    }

    #[test]
    fn local_override_replaces_content() {
        let d = def(DefinitionType::Char, "c", "global");
        let overrides = json!({ d.id.clone(): "overridden" });
        let out = assemble_system_prompt(std::slice::from_ref(&d), &overrides, &json!({}));
        assert!(out.contains("overridden"));
        assert!(!out.contains("global"));
    }

    #[test]
    fn regex_rules_clean_text() {
        let mut r = def(DefinitionType::RegexRule, "r", "");
        r.meta = json!({ "pattern": "<think>.*?</think>", "replacement": "" });
        assert_eq!(apply_regex_rules("a<think>x</think>b", &[r]).as_deref(), Some("ab"));
        assert_eq!(apply_regex_rules("abc", &[]), None);
    }
}
```

- [ ] **Step 3: `lib.rs` 导出** —— 加 `pub mod assembly;` 与 `pub use assembly::{assemble_system_prompt, apply_regex_rules, render_vars};`
- [ ] **Step 4: 测试** — Run: `cargo test -p shirita-core assembly::` → 4 passed。
- [ ] **Step 5: 提交** — `git commit -am "feat(m2): assembly pipeline (vars, xml wrap, regex clean)"`

---

## Task 3: send_message 接入组装与 display_content（TDD）

**Files:** `shirita-core/src/conversation.rs`

- [ ] **Step 1: 修改 `send_message`（在组装请求处接入）**
在 `let history = ...` 之后、构造 `chat_messages` 处，改为：加载 session、组装 system、收集 regex 规则。把"组装请求消息"段替换为：
```rust
        // 载入会话以取挂载/覆盖/状态。
        let session = match storage.get_session(&session_id).await {
            Ok(Some(s)) => s,
            Ok(None) => { yield SendEvent::Error("session not found".into()); return; }
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        // 依挂载顺序载入定义。
        let mut mounted = Vec::new();
        for id in &session.mounted_definitions {
            match storage.get_definition(id).await {
                Ok(Some(d)) => mounted.push(d),
                Ok(None) => {}
                Err(e) => { yield SendEvent::Error(e.to_string()); return; }
            }
        }
        let local = session
            .override_config
            .get("local_definitions")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let system = crate::assembly::assemble_system_prompt(&mounted, &local, &session.current_state);
        let regex_rules: Vec<_> = mounted
            .iter()
            .filter(|d| d.def_type == crate::models::definition::DefinitionType::RegexRule)
            .cloned()
            .collect();

        // 组装请求消息：system（若非空） + 历史（过滤隐藏） + 新 user。
        let mut chat_messages: Vec<ChatMessage> = Vec::new();
        if !system.is_empty() {
            chat_messages.push(ChatMessage { role: Role::System, content: system });
        }
        chat_messages.extend(history.iter().filter(|m| !m.is_hidden).map(|m| ChatMessage {
            role: m.role,
            content: m.raw_content.clone(),
        }));
        chat_messages.push(ChatMessage { role: Role::User, content: user_text.clone() });
```
> 注意：原先 `let mut chat_messages: Vec<ChatMessage> = history.iter()...collect();` 整段被上面替换。`prompt_text`/`tracing::debug!`/`req` 之后照旧。

- [ ] **Step 2: 落库 assistant 时写 display_content**
把 `let assistant = Message::new(...)` 段替换为：
```rust
        let mut assistant = Message::new(&session_id, Some(user_msg.id.clone()), Role::Assistant, &full);
        assistant.display_content = crate::assembly::apply_regex_rules(&full, &regex_rules);
        if let Err(e) = storage.create_message(&assistant).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
        yield SendEvent::Done { message_id: assistant.id };
```

- [ ] **Step 3: 追加测试（验证组装真的喂给 provider + regex 清洗）**
在 `conversation.rs` tests 内追加一个记录型 provider 与两个测试：
```rust
    use crate::model::{ChatRequest, ModelProvider};
    use futures::stream::{self, BoxStream};
    use std::sync::Mutex;

    struct RecordingProvider {
        seen: Arc<Mutex<Option<ChatRequest>>>,
        reply: String,
    }
    #[async_trait::async_trait]
    impl ModelProvider for RecordingProvider {
        async fn stream_chat(&self, req: ChatRequest) -> crate::Result<BoxStream<'static, crate::Result<String>>> {
            *self.seen.lock().unwrap() = Some(req);
            let reply = self.reply.clone();
            Ok(Box::pin(stream::iter(vec![Ok(reply)])))
        }
    }

    #[tokio::test]
    async fn assembled_system_is_sent() {
        let storage = Arc::new(temp_storage().await);
        let mut session = Session::new("t");
        // 挂一个角色定义。
        let mut ch = crate::models::definition::Definition::new(
            crate::models::definition::DefinitionType::Char, "C", "I am {{who}}");
        ch.meta = serde_json::json!({});
        storage.create_definition(&ch).await.unwrap();
        session.mounted_definitions = vec![ch.id.clone()];
        session.current_state = serde_json::json!({ "who": "Neo" });
        storage.create_session(&session).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hi".into());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        assert_eq!(req.messages[0].role, Role::System);
        assert!(req.messages[0].content.contains("<characters>"));
        assert!(req.messages[0].content.contains("I am Neo")); // 变量渲染生效
    }

    #[tokio::test]
    async fn regex_rule_sets_display_content() {
        let storage = Arc::new(temp_storage().await);
        let mut session = Session::new("t");
        let mut rule = crate::models::definition::Definition::new(
            crate::models::definition::DefinitionType::RegexRule, "R", "");
        rule.meta = serde_json::json!({ "pattern": "STOP", "replacement": "" });
        storage.create_definition(&rule).await.unwrap();
        session.mounted_definitions = vec![rule.id.clone()];
        storage.create_session(&session).await.unwrap();

        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: Arc::new(Mutex::new(None)), reply: "helloSTOP".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hi".into());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let assistant = msgs.iter().find(|m| m.role == Role::Assistant).unwrap();
        assert_eq!(assistant.raw_content, "helloSTOP");
        assert_eq!(assistant.display_content.as_deref(), Some("hello"));
    }
```

- [ ] **Step 4: 测试** — Run: `cargo test -p shirita-core` → 全绿（含两新测试；M1 echo 测试不受影响）。
- [ ] **Step 5: 提交** — `git commit -am "feat(m2): wire assembly + regex display_content into send_message"`

---

## Task 4: definitions REST CRUD（web）（TDD）

**Files:** `shirita-web/src/routes/definitions.rs`(create), `shirita-web/src/routes/mod.rs`, `shirita-web/src/lib.rs`, `shirita-web/tests/definitions_test.rs`(create)

- [ ] **Step 1: 创建 `routes/definitions.rs`**
```rust
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use shirita_core::models::definition::{Definition, DefinitionType};

use crate::AppState;

#[derive(Deserialize)]
pub struct DefinitionBody {
    pub r#type: String,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub meta: Value,
}

fn build(id: String, body: DefinitionBody) -> Result<Definition, StatusCode> {
    let def_type = DefinitionType::from_db(&body.r#type).map_err(|_| StatusCode::BAD_REQUEST)?;
    let meta = if body.meta.is_null() { serde_json::json!({}) } else { body.meta };
    Ok(Definition { id, def_type, name: body.name, content: body.content, meta })
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub r#type: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Definition>>, StatusCode> {
    let mut defs = state.storage.list_definitions().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(t) = q.r#type {
        defs.retain(|d| d.def_type.as_str() == t);
    }
    Ok(Json(defs))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<DefinitionBody>,
) -> Result<Json<Definition>, StatusCode> {
    let def = build(uuid::Uuid::new_v4().to_string(), body)?;
    state.storage.create_definition(&def).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(def))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Definition>, StatusCode> {
    match state.storage.get_definition(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        Some(d) => Ok(Json(d)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<DefinitionBody>,
) -> Result<Json<Definition>, StatusCode> {
    let def = build(id, body)?;
    state.storage.update_definition(&def).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(def))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    state.storage.delete_definition(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}
```
> 需要 web 依赖 uuid（Task 5 Step 1 一并加）。本 Task 编译前先加 `uuid.workspace = true` 到 `shirita-web/Cargo.toml`。

- [ ] **Step 2: `routes/mod.rs` 加** `pub mod definitions;`

- [ ] **Step 3: `lib.rs` 在受保护 `/api` 路由组加**
```rust
        .route("/definitions", get(routes::definitions::list).post(routes::definitions::create))
        .route(
            "/definitions/{id}",
            get(routes::definitions::get)
                .put(routes::definitions::update)
                .delete(routes::definitions::delete),
        )
```

- [ ] **Step 4: 创建 `tests/definitions_test.rs`**（复用 M1 的 `test_state`/`auth` 范式；用 `Bearer secret-token`）
```rust
use std::sync::Arc;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;
use shirita_core::{Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("def_test.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "m".into() }
}
fn req(method: &str, uri: &str, body: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri).header(header::AUTHORIZATION, "Bearer secret-token");
    if body.is_some() { b = b.header(header::CONTENT_TYPE, "application/json"); }
    b.body(body.map(|s| Body::from(s.to_string())).unwrap_or(Body::empty())).unwrap()
}

#[tokio::test]
async fn definition_crud_over_http() {
    let state = test_state().await;

    // create
    let res = app(state.clone()).oneshot(req("POST", "/api/definitions",
        Some(r#"{"type":"char","name":"Alice","content":"<c/>"}"#))).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["type"], "char");

    // get
    let res = app(state.clone()).oneshot(req("GET", &format!("/api/definitions/{id}"), None)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // list with type filter
    let res = app(state.clone()).oneshot(req("GET", "/api/definitions?type=char", None)).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    let res = app(state.clone()).oneshot(req("GET", "/api/definitions?type=world", None)).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 0);

    // update
    let res = app(state.clone()).oneshot(req("PUT", &format!("/api/definitions/{id}"),
        Some(r#"{"type":"persona","name":"Al","content":"x"}"#))).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let updated: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated["type"], "persona");

    // delete
    let res = app(state.clone()).oneshot(req("DELETE", &format!("/api/definitions/{id}"), None)).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = app(state).oneshot(req("GET", &format!("/api/definitions/{id}"), None)).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bad_type_is_rejected() {
    let res = app(test_state().await).oneshot(req("POST", "/api/definitions",
        Some(r#"{"type":"nope","name":"x","content":"y"}"#))).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 5: 测试** — Run: `cargo test -p shirita-web definition` → 2 passed。
- [ ] **Step 6: 提交** — `git commit -am "feat(m2): definitions REST CRUD"`

---

## Task 5: 资源上传、静态服务与 resolve_asset_url（TDD）

**Files:** `shirita-web/Cargo.toml`, `shirita-web/src/routes/assets.rs`(create), `shirita-web/src/routes/mod.rs`, `shirita-web/src/lib.rs`, `shirita-web/src/main.rs`, `shirita-web/tests/assets_mounts_test.rs`(create)

- [ ] **Step 1: `shirita-web/Cargo.toml`**
- `axum = "0.8"` → `axum = { version = "0.8", features = ["multipart"] }`
- `tower-http = { version = "0.6", features = ["trace"] }` → 加 `"fs"`
- `[dependencies]` 加 `uuid.workspace = true`（若 Task 4 已加则跳过）

- [ ] **Step 2: 创建 `routes/assets.rs`**
```rust
use std::path::Path as FsPath;

use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

/// Web 下的资源 URL 解析：相对路径 → `/assets/<rel>`。
/// （Tauri 入口在 M8 返回 `asset://localhost/<rel>`。）
pub fn resolve_asset_url(relative: &str) -> String {
    format!("/assets/{}", relative.trim_start_matches('/'))
}

pub async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Value>, StatusCode> {
    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        let filename = field.file_name().map(|s| s.to_string());
        let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
        let ext = filename
            .as_deref()
            .and_then(|f| f.rsplit('.').next())
            .filter(|e| !e.is_empty() && e.len() <= 8)
            .unwrap_or("bin");
        let name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
        let path = FsPath::new(&state.config.assets_dir).join(&name);
        tokio::fs::write(&path, &data).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(Json(json!({ "path": name, "url": resolve_asset_url(&name) })));
    }
    Err(StatusCode::BAD_REQUEST)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolve_prefixes_assets() {
        assert_eq!(resolve_asset_url("a.png"), "/assets/a.png");
        assert_eq!(resolve_asset_url("/a.png"), "/assets/a.png");
    }
}
```

- [ ] **Step 3: `routes/mod.rs` 加** `pub mod assets;`

- [ ] **Step 4: `lib.rs`** —— 顶部加 `use tower_http::services::ServeDir;`；在受保护组加上传路由：
```rust
        .route("/assets", axum::routing::post(routes::assets::upload))
```
并在最外层 Router（`with_state` 之前）加静态服务（公开，便于浏览器加载图片）：
```rust
        .nest_service("/assets", ServeDir::new(state.config.assets_dir.clone()))
```
> 注意：`/assets` 既有受保护的 `POST /api/assets`（上传）也有公开的 `GET /assets/*`（静态）。两者路径不同（`/api/assets` vs `/assets`），不冲突。`nest_service` 放在 `Router::new()...` 链上、`with_state(state)` 之前；`state.config.assets_dir` 在 move 进 with_state 前先 clone。

- [ ] **Step 5: `main.rs`** —— 在 `run_migrations` 之后加：
```rust
    tokio::fs::create_dir_all(&config.assets_dir).await.ok();
```

- [ ] **Step 6: 创建 `tests/assets_mounts_test.rs`**
```rust
use std::sync::Arc;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;
use shirita_core::{Config, EchoProvider, ModelProvider, Session, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app, AppState};

async fn state_with_assets() -> (AppState, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_path_buf();
    std::mem::forget(dir);
    let assets = base.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let storage = SqliteStorage::connect(base.join("am.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new(base.join("am.db").to_str().unwrap(), assets.to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    (AppState { storage, config, provider, token_counter, model: "m".into() }, assets)
}

#[tokio::test]
async fn upload_writes_file_and_returns_url() {
    let (state, assets) = state_with_assets().await;
    let boundary = "BOUNDARY";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"pic.png\"\r\nContent-Type: application/octet-stream\r\n\r\nHELLO\r\n--{b}--\r\n",
        b = boundary
    );
    let req = Request::builder()
        .method("POST").uri("/api/assets")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body)).unwrap();
    let res = app(state).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let name = v["path"].as_str().unwrap();
    assert!(v["url"].as_str().unwrap().starts_with("/assets/"));
    assert!(name.ends_with(".png"));
    assert_eq!(std::fs::read(assets.join(name)).unwrap(), b"HELLO");
}

#[tokio::test]
async fn set_and_read_mounts() {
    let (state, _) = state_with_assets().await;
    // 建会话。
    let s = Session::new("c");
    state.storage.create_session(&s).await.unwrap();
    // PUT mounts
    let req = Request::builder()
        .method("PUT").uri(format!("/api/sessions/{}/mounts", s.id))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"definition_ids":["d1","d2"]}"#)).unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // 读取会话确认。
    let got = state.storage.get_session(&s.id).await.unwrap().unwrap();
    assert_eq!(got.mounted_definitions, vec!["d1", "d2"]);
}
```
> `set_and_read_mounts` 依赖 Task 6 的 mount 端点；若按顺序执行，本测试会随 Task 6 实现后转绿。可先写、Task 6 后再跑。

- [ ] **Step 7: 测试（仅 upload + resolve 单测）** — Run: `cargo test -p shirita-web assets` → resolve 单测 + upload 测试通过（mount 测试待 Task 6）。
- [ ] **Step 8: 提交** — `git commit -am "feat(m2): asset upload, static serving, resolve_asset_url"`

---

## Task 6: 会话挂载 REST 端点 + 全量校验

**Files:** `shirita-web/src/routes/sessions.rs`, `shirita-web/src/lib.rs`

- [ ] **Step 1: `routes/sessions.rs` 加 mount 处理器**
```rust
#[derive(Deserialize)]
pub struct SetMounts {
    pub definition_ids: Vec<String>,
}

pub async fn set_mounts(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<SetMounts>,
) -> Result<StatusCode, StatusCode> {
    state
        .storage
        .set_mounted_definitions(&session_id, &body.definition_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}
```
（`Deserialize` 已在文件顶部 import；如未，则 `use serde::Deserialize;`。）

- [ ] **Step 2: `lib.rs` 受保护组加**
```rust
        .route("/sessions/{id}/mounts", axum::routing::put(routes::sessions::set_mounts))
```

- [ ] **Step 3: 全量测试** — Run: `cargo test --workspace` → 全绿（core + web，含 assets_mounts 的两个测试）。

- [ ] **Step 4: 离线冒烟（端到端：定义→挂载→对话引用）**
后台启动：
```bash
TOKEN_SECRET=devtoken DATABASE_PATH=/home/cc/.claude/jobs/b73d9870/tmp/m2-smoke.db ASSETS_DIR=/home/cc/.claude/jobs/b73d9870/tmp/m2-assets BIND_ADDR=127.0.0.1:8789 cargo run -q -p shirita-web
```
脚本（另一终端）：
```bash
H='-H Authorization:Bearer devtoken'; J='-H Content-Type:application/json'
# 建角色定义
CID=$(curl -s $H $J -d '{"type":"char","name":"Neo","content":"I am {{who}}"}' http://127.0.0.1:8789/api/definitions | grep -o '"id":"[^"]*"' | head -1 | sed 's/.*:"//;s/"//')
# 建会话
SID=$(curl -s $H $J -d '{"name":"s"}' http://127.0.0.1:8789/api/sessions | grep -o '"id":"[^"]*"' | head -1 | sed 's/.*:"//;s/"//')
# 挂载角色
curl -s -o /dev/null -w "mount=%{http_code}\n" -X PUT $H $J -d "{\"definition_ids\":[\"$CID\"]}" http://127.0.0.1:8789/api/sessions/$SID/mounts
# 发消息（EchoProvider 回 echo: <user>，但请求里 system 已含 <characters>I am ...）
curl -N -s $H $J -d '{"text":"hi"}' http://127.0.0.1:8789/api/sessions/$SID/messages; echo
# 资源上传
echo PNGDATA > /tmp/x.png
curl -s $H -F file=@/tmp/x.png http://127.0.0.1:8789/api/assets
```
Expected: mount=200；SSE 正常 done；上传返回 `{"path":"<uuid>.png","url":"/assets/<uuid>.png"}`，且 `GET /assets/<uuid>.png` 可取回。停服并清理冒烟 db/assets。

- [ ] **Step 5: 提交** — `git commit -am "feat(m2): session mounts REST endpoint"`

---

## M2 完成判定（DoD）

- [ ] `cargo test --workspace` 全绿（core 约 21 + web 约 11）。
- [ ] definitions 全类型 REST CRUD 正常（含 type 过滤、坏 type → 400）。
- [ ] 资源上传写入 `ASSETS_DIR`，`GET /assets/<file>` 取回；`resolve_asset_url` 正确。
- [ ] 挂载角色+世界书后发消息：请求的 system 消息含 `<characters>`/`<world_rules>` 且 `{{var}}` 已渲染（RecordingProvider 测试证明）。
- [ ] `regex_rule` 挂载后助手消息写入清洗后的 `display_content`。
- [ ] 全部改动已在 `main` 提交。

## 自检备注（Self-Review）
- **Spec 覆盖**：M2 四块 —— definitions REST(T4)、资源(T5)、组装流水线(T2+T3)、regex display_content(T2+T3)；挂载模型(T1+T6)。
- **类型一致性**：`assemble_system_prompt(mounted,&local,&state)`、`apply_regex_rules(text,&rules)->Option<String>`、`render_vars(content,&state)`、`Storage::set_mounted_definitions(id,&[String])`、`Session.mounted_definitions:Vec<String>`、`resolve_asset_url(&str)->String`、definitions handler 签名、mount body `{definition_ids}` 一致。
- **向后兼容**：空挂载 → 无 system 消息 → M1 echo 测试不变。
- **无占位符**：硬逻辑（assembly/regex/send_message/upload）均给出完整代码；CRUD/测试给出完整代码。
```
