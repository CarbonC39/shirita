# M1 — 最小端到端对话 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 打通第一条垂直切片：在浏览器里创建会话、发一条消息，看到助手回复**流式**返回（SSE）并持久化到 SQLite。无 API key 时用离线 `EchoProvider` 即可演示/测试；有 key 时走真实 OpenAI 兼容接口。

**Architecture:** `shirita-core` 新增 `ModelProvider` trait（OpenAI 兼容流式适配器 + 离线 Echo 适配器）、`TokenCounter` trait（tiktoken 默认实现）、`Session`/`Message`/`Role` 模型、Storage 的 sessions/messages CRUD，以及对话服务 `send_message`（返回 `Stream<SendEvent>`，边流式边累积，结束时落库助手消息）。`shirita-web` 新增 sessions/messages REST 与 `POST /api/sessions/{id}/messages` 的 SSE 端点，外加一个极薄静态 HTML 验证页。

**Tech Stack:** 在 M0 基础上新增：reqwest 0.12（rustls，流式）· futures 0.3 · async-stream 0.3 · tiktoken-rs · chrono · axum SSE。沿用 sqlx 运行时 API（无 `DATABASE_URL`）。

---

## 前置说明（实现者必读）

- **离线优先**：`EchoProvider` 让整条链路在无网络/无 key 时可跑通。`main.rs` 据 `OPENAI_API_KEY` 是否为空选择 `EchoProvider`（空）或 `OpenAiProvider`（非空）。测试一律用 `EchoProvider`，确定性强、不触网。
- `ModelProvider::stream_chat` 返回 `futures::stream::BoxStream<'static, Result<String>>`（每个元素是一段文本增量 delta；流自然结束即代表 done）。
- `send_message` 用 `async_stream::stream!` 写命令式流：先落库 user 消息 → 组装历史 → 调 provider 流 → 逐 delta 累积并 yield → 结束落库 assistant 消息 → yield Done。
- OpenAI SSE 解析抽出**纯函数** `parse_delta` 做单测；HTTP 整链不做单测（靠 `EchoProvider` 覆盖编排逻辑）。
- 全程在 `main` 分支，每个 Task 末尾提交。先建测试再实现；为省构建开销，可在每个 Task 末尾一次性 `cargo test` 验证（不强制每步重复编译）。

## 文件结构（本里程碑创建/修改）

```
shirita-core/
├── Cargo.toml                         # 修改：新增依赖
├── migrations/0002_messages_created_at.sql   # 创建
└── src/
    ├── lib.rs                         # 修改：挂载/re-export 新模块
    ├── config.rs                      # 修改：新增 openai_* 字段
    ├── models/
    │   ├── mod.rs                     # 修改：挂载 session/message
    │   ├── session.rs                 # 创建：Session
    │   └── message.rs                 # 创建：Message / Role
    ├── storage/
    │   ├── mod.rs                     # 修改：Storage 增 sessions/messages 方法
    │   └── sqlite.rs                  # 修改：实现新方法 + row 映射
    ├── tokenizer/
    │   ├── mod.rs                     # 创建：TokenCounter trait
    │   └── tiktoken.rs                # 创建：TiktokenCounter
    ├── model/
    │   ├── mod.rs                     # 创建：ModelProvider trait + 类型 + parse_delta
    │   ├── echo.rs                    # 创建：EchoProvider（离线）
    │   └── openai.rs                  # 创建：OpenAiProvider（真实）
    └── conversation.rs                # 创建：send_message + SendEvent
shirita-web/
├── Cargo.toml                         # 修改：新增 futures
├── static/index.html                  # 创建：极薄验证页
└── src/
    ├── lib.rs                         # 修改：挂载新路由
    ├── state.rs                       # 修改：AppState 增 provider/counter/model
    ├── main.rs                        # 修改：构建 provider + 提供静态页
    └── routes/
        ├── mod.rs                     # 修改：挂载 sessions/chat/index
        ├── sessions.rs                # 创建：sessions/messages REST
        ├── chat.rs                    # 创建：SSE send 端点
        └── index.rs                   # 创建：GET / 返回 index.html
shirita-web/tests/
├── api_test.rs                        # 沿用（M0）
├── sessions_test.rs                   # 创建：sessions/messages REST 测试
└── chat_test.rs                       # 创建：SSE 流式 + 落库测试
```

---

## Task 1: 新增依赖与 Session/Message/Role 模型（TDD）

**Files:**
- Modify: `shirita-core/Cargo.toml`
- Modify: `shirita-core/src/config.rs`
- Create: `shirita-core/src/models/session.rs`
- Create: `shirita-core/src/models/message.rs`
- Modify: `shirita-core/src/models/mod.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 在 `shirita-core/Cargo.toml` 的 `[dependencies]` 末尾追加**

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls"] }
futures = "0.3"
async-stream = "0.3"
tiktoken-rs = "0.6"
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 2: 扩展 Config（`shirita-core/src/config.rs`）**

把 `struct Config { ... }` 替换为（新增三个 openai 字段）：
```rust
pub struct Config {
    pub database_path: String,
    pub assets_dir: String,
    pub token_secret: String,
    pub openai_base_url: String,
    pub openai_api_key: String,
    pub openai_model: String,
}
```

把 `new` 的 `Ok(Self { ... })` 部分替换为（带 openai 默认值）：
```rust
        Ok(Self {
            database_path: database_path.into(),
            assets_dir: assets_dir.into(),
            token_secret,
            openai_base_url: "https://api.openai.com/v1".into(),
            openai_api_key: String::new(),
            openai_model: "gpt-4o-mini".into(),
        })
```

把 `from_env` 替换为（先 new 再用 env 覆盖 openai_*）：
```rust
    pub fn from_env() -> Result<Self> {
        let database_path =
            std::env::var("DATABASE_PATH").unwrap_or_else(|_| "shirita.db".into());
        let assets_dir = std::env::var("ASSETS_DIR").unwrap_or_else(|_| "./assets".into());
        let token_secret = std::env::var("TOKEN_SECRET")
            .map_err(|_| Error::Config("TOKEN_SECRET env var is required".into()))?;

        let mut cfg = Self::new(database_path, assets_dir, token_secret)?;
        if let Ok(v) = std::env::var("OPENAI_BASE_URL") {
            cfg.openai_base_url = v;
        }
        cfg.openai_api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        if let Ok(v) = std::env::var("OPENAI_MODEL") {
            cfg.openai_model = v;
        }
        Ok(cfg)
    }
```
（M0 既有的两个 config 测试只检查前三个字段，保持通过。）

- [ ] **Step 3: 写失败测试 —— 创建 `shirita-core/src/models/message.rs`（含类型骨架 + 测试）**

```rust
//! 消息模型与角色标签。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub role: Role,
    pub raw_content: String,
    pub display_content: Option<String>,
    pub is_hidden: bool,
    #[serde(default)]
    pub snapshot_state: serde_json::Value,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_db_roundtrip() {
        for (variant, s) in [
            (Role::System, "system"),
            (Role::User, "user"),
            (Role::Assistant, "assistant"),
        ] {
            assert_eq!(variant.as_str(), s);
            assert_eq!(Role::from_db(s).unwrap(), variant);
        }
        assert!(Role::from_db("nope").is_err());
    }

    #[test]
    fn new_message_defaults() {
        let m = Message::new("sess-1", Some("parent-1".into()), Role::User, "hi");
        assert_eq!(m.session_id, "sess-1");
        assert_eq!(m.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(m.role, Role::User);
        assert_eq!(m.raw_content, "hi");
        assert_eq!(m.display_content, None);
        assert!(!m.is_hidden);
        assert_eq!(m.snapshot_state, serde_json::json!({}));
        assert_eq!(m.id.len(), 36);
        assert!(!m.created_at.is_empty());
    }
}
```

- [ ] **Step 4: 创建 `shirita-core/src/models/session.rs`**

```rust
//! 会话模型。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    #[serde(default)]
    pub override_config: serde_json::Value,
    #[serde(default)]
    pub current_state: serde_json::Value,
}

impl Session {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            avatar: None,
            override_config: serde_json::json!({}),
            current_state: serde_json::json!({}),
        }
    }
}
```

- [ ] **Step 5: 挂载模块（`models/mod.rs` 与 `lib.rs`）**

`shirita-core/src/models/mod.rs`（替换全文）:
```rust
pub mod definition;
pub mod message;
pub mod session;
```

`shirita-core/src/lib.rs` 的 `pub use models::...` 行替换为：
```rust
pub use models::definition::{Definition, DefinitionType};
pub use models::message::{Message, Role};
pub use models::session::Session;
```

- [ ] **Step 6: 实现 `Role::{as_str,from_db}` 与 `Message::new`**

在 `message.rs` 的 `enum Role { ... }` 之后插入：
```rust
impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }

    pub fn from_db(s: &str) -> crate::Result<Self> {
        Ok(match s {
            "system" => Role::System,
            "user" => Role::User,
            "assistant" => Role::Assistant,
            other => return Err(crate::Error::InvalidDefinitionType(other.to_string())),
        })
    }
}
```

在 `struct Message { ... }` 之后插入：
```rust
impl Message {
    pub fn new(
        session_id: impl Into<String>,
        parent_id: Option<String>,
        role: Role,
        raw_content: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            parent_id,
            role,
            raw_content: raw_content.into(),
            display_content: None,
            is_hidden: false,
            snapshot_state: serde_json::json!({}),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}
```
> 备注：`from_db` 复用了 `Error::InvalidDefinitionType` 以避免新增错误变体；M1 范围内可接受。若希望更准确，可在 `error.rs` 增 `InvalidRole(String)` 并改用之（可选，不阻塞）。

- [ ] **Step 7: 运行测试，确认通过**

Run: `cargo test -p shirita-core message:: config::`
Expected: message 2 个 + config 2 个测试通过（首次会拉取新依赖并编译）。

- [ ] **Step 8: 提交**

```bash
git add shirita-core/Cargo.toml shirita-core/src/config.rs shirita-core/src/models shirita-core/src/lib.rs
git commit -m "feat(m1): add Session/Message/Role models and openai config fields"
```

---

## Task 2: 迁移 0002 与 Storage 的 sessions/messages CRUD（TDD）

**Files:**
- Create: `shirita-core/migrations/0002_messages_created_at.sql`
- Modify: `shirita-core/src/storage/mod.rs`
- Modify: `shirita-core/src/storage/sqlite.rs`

- [ ] **Step 1: 创建迁移 `shirita-core/migrations/0002_messages_created_at.sql`**

```sql
ALTER TABLE messages ADD COLUMN created_at TEXT NOT NULL DEFAULT '';
```

- [ ] **Step 2: 在 `sqlite.rs` 的 `#[cfg(test)] mod tests` 内追加失败测试**

在 `definition_crud_roundtrip` 之后追加：
```rust
    use crate::models::message::{Message, Role};
    use crate::models::session::Session;

    #[tokio::test]
    async fn session_and_message_roundtrip() {
        let storage = temp_storage().await;

        let session = Session::new("Chat 1");
        storage.create_session(&session).await.unwrap();

        let got = storage.get_session(&session.id).await.unwrap().unwrap();
        assert_eq!(got, session);
        assert_eq!(storage.list_sessions().await.unwrap().len(), 1);

        let m1 = Message::new(&session.id, None, Role::User, "hello");
        storage.create_message(&m1).await.unwrap();
        let m2 = Message::new(&session.id, Some(m1.id.clone()), Role::Assistant, "hi there");
        storage.create_message(&m2).await.unwrap();

        let msgs = storage.list_messages(&session.id).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].id, m1.id);
        assert_eq!(msgs[1].id, m2.id);
        assert_eq!(msgs[1].parent_id.as_deref(), Some(m1.id.as_str()));
        assert_eq!(msgs[1].role, Role::Assistant);
    }
```

- [ ] **Step 3: 扩展 Storage trait（`shirita-core/src/storage/mod.rs`）**

把 `use crate::models::definition::Definition;` 行替换为：
```rust
use crate::models::definition::Definition;
use crate::models::message::Message;
use crate::models::session::Session;
```

在 trait 内（`delete_definition` 之后）追加：
```rust
    // --- sessions ---
    async fn create_session(&self, session: &Session) -> Result<()>;
    async fn get_session(&self, id: &str) -> Result<Option<Session>>;
    async fn list_sessions(&self) -> Result<Vec<Session>>;

    // --- messages ---
    async fn create_message(&self, message: &Message) -> Result<()>;
    /// 按 created_at（再以 id 为 tiebreak）升序返回某会话的全部消息。
    async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>>;
```

- [ ] **Step 4: 运行测试，确认失败**

Run: `cargo test -p shirita-core session_and_message`
Expected: 编译失败 —— `SqliteStorage` 未实现新 trait 方法。

- [ ] **Step 5: 在 `sqlite.rs` 实现新方法 + 行映射**

把顶部 `use crate::models::definition::{Definition, DefinitionType};` 替换为：
```rust
use crate::models::definition::{Definition, DefinitionType};
use crate::models::message::{Message, Role};
use crate::models::session::Session;
```

在 `row_to_definition` 之后插入两个映射函数：
```rust
fn row_to_session(row: &SqliteRow) -> Result<Session> {
    let override_config: String = row.try_get("override_config")?;
    let current_state: String = row.try_get("current_state")?;
    Ok(Session {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        avatar: row.try_get("avatar")?,
        override_config: serde_json::from_str(&override_config)?,
        current_state: serde_json::from_str(&current_state)?,
    })
}

fn row_to_message(row: &SqliteRow) -> Result<Message> {
    let role_str: String = row.try_get("role")?;
    let snapshot: String = row.try_get("snapshot_state")?;
    let is_hidden: i64 = row.try_get("is_hidden")?;
    Ok(Message {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        parent_id: row.try_get("parent_id")?,
        role: Role::from_db(&role_str)?,
        raw_content: row.try_get("raw_content")?,
        display_content: row.try_get("display_content")?,
        is_hidden: is_hidden != 0,
        snapshot_state: serde_json::from_str(&snapshot)?,
        created_at: row.try_get("created_at")?,
    })
}
```

在 `#[async_trait] impl Storage for SqliteStorage {` 块内、`delete_definition` 之后追加：
```rust
    async fn create_session(&self, session: &Session) -> Result<()> {
        let override_config = serde_json::to_string(&session.override_config)?;
        let current_state = serde_json::to_string(&session.current_state)?;
        sqlx::query(
            "INSERT INTO chat_sessions (id, name, avatar, override_config, current_state) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&session.id)
        .bind(&session.name)
        .bind(&session.avatar)
        .bind(override_config)
        .bind(current_state)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let row = sqlx::query(
            "SELECT id, name, avatar, override_config, current_state FROM chat_sessions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some(row_to_session(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_sessions(&self) -> Result<Vec<Session>> {
        let rows = sqlx::query(
            "SELECT id, name, avatar, override_config, current_state FROM chat_sessions ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_session).collect()
    }

    async fn create_message(&self, message: &Message) -> Result<()> {
        let snapshot = serde_json::to_string(&message.snapshot_state)?;
        sqlx::query(
            "INSERT INTO messages \
             (id, session_id, parent_id, role, raw_content, display_content, is_hidden, snapshot_state, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&message.id)
        .bind(&message.session_id)
        .bind(&message.parent_id)
        .bind(message.role.as_str())
        .bind(&message.raw_content)
        .bind(&message.display_content)
        .bind(message.is_hidden as i64)
        .bind(snapshot)
        .bind(&message.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let rows = sqlx::query(
            "SELECT id, session_id, parent_id, role, raw_content, display_content, is_hidden, snapshot_state, created_at \
             FROM messages WHERE session_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_message).collect()
    }
```

- [ ] **Step 6: 运行测试，确认通过**

Run: `cargo test -p shirita-core`
Expected: 全部通过（M0 的 7 + message 2 + session_and_message 1 = 10）。

- [ ] **Step 7: 提交**

```bash
git add shirita-core/migrations shirita-core/src/storage
git commit -m "feat(m1): add sessions/messages storage CRUD and created_at migration"
```

---

## Task 3: TokenCounter trait 与 TiktokenCounter（TDD）

**Files:**
- Create: `shirita-core/src/tokenizer/mod.rs`
- Create: `shirita-core/src/tokenizer/tiktoken.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 创建 `shirita-core/src/tokenizer/mod.rs`（trait）**

```rust
//! Token 计数抽象。M1 仅用于日志/预算展示，不做裁剪。

pub mod tiktoken;

pub trait TokenCounter: Send + Sync {
    fn count(&self, text: &str) -> usize;
}
```

- [ ] **Step 2: 写失败测试 —— 创建 `shirita-core/src/tokenizer/tiktoken.rs`（骨架 + 测试）**

```rust
//! 基于 tiktoken（cl100k_base）的轻量计数器，作为所有模型的近似计数。

use tiktoken_rs::CoreBPE;

use super::TokenCounter;

pub struct TiktokenCounter {
    bpe: CoreBPE,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_positive_and_monotonic() {
        let counter = TiktokenCounter::new();
        assert!(counter.count("hello world") > 0);
        assert!(
            counter.count("a longer piece of text goes here")
                > counter.count("hi")
        );
        assert_eq!(counter.count(""), 0);
    }
}
```

- [ ] **Step 3: 在 `lib.rs` 挂载并 re-export**

`shirita-core/src/lib.rs` 增加：
```rust
pub mod tokenizer;

pub use tokenizer::{tiktoken::TiktokenCounter, TokenCounter};
```

- [ ] **Step 4: 运行测试，确认失败**

Run: `cargo test -p shirita-core tiktoken::`
Expected: 编译失败 —— `no function or associated item named 'new'` / 未实现 `TokenCounter`。

- [ ] **Step 5: 实现 `TiktokenCounter`**

在 `tiktoken.rs` 的 `struct TiktokenCounter { ... }` 之后插入：
```rust
impl TiktokenCounter {
    pub fn new() -> Self {
        // cl100k_base 内置词表，无需联网。
        let bpe = tiktoken_rs::cl100k_base().expect("cl100k_base must load");
        Self { bpe }
    }
}

impl Default for TiktokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter for TiktokenCounter {
    fn count(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        self.bpe.encode_with_special_tokens(text).len()
    }
}
```

- [ ] **Step 6: 运行测试，确认通过**

Run: `cargo test -p shirita-core tiktoken::`
Expected: `test result: ok. 1 passed`。

- [ ] **Step 7: 提交**

```bash
git add shirita-core/src/tokenizer shirita-core/src/lib.rs
git commit -m "feat(m1): add TokenCounter trait and tiktoken-based counter"
```

---

## Task 4: ModelProvider trait、类型、SSE 解析、Echo 与 OpenAI 适配器（TDD 解析器）

**Files:**
- Create: `shirita-core/src/model/mod.rs`
- Create: `shirita-core/src/model/echo.rs`
- Create: `shirita-core/src/model/openai.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 创建 `shirita-core/src/model/mod.rs`（trait + 类型 + parse_delta + 测试）**

```rust
//! 模型适配层：统一的流式聊天接口。

pub mod echo;
pub mod openai;

use async_trait::async_trait;
use futures::stream::BoxStream;

use crate::models::message::Role;
use crate::Result;

pub use echo::EchoProvider;
pub use openai::OpenAiProvider;

/// 发给模型的单条消息（与持久化的 Message 解耦）。
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// 一次聊天补全请求。
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
}

/// 流式聊天：每个元素是一段文本增量；流结束即 done。
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn stream_chat(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<String>>>;
}

/// 解析 OpenAI SSE 中 `data:` 之后的 JSON，提取 `choices[0].delta.content`。
/// 仅含 role（无 content）的首帧返回 `Ok(None)`。
pub fn parse_delta(json_after_data: &str) -> Result<Option<String>> {
    let v: serde_json::Value = serde_json::from_str(json_after_data)?;
    Ok(v["choices"][0]["delta"]["content"]
        .as_str()
        .map(|s| s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_delta_extracts_content() {
        let line = r#"{"choices":[{"delta":{"content":"He"}}]}"#;
        assert_eq!(parse_delta(line).unwrap(), Some("He".to_string()));
    }

    #[test]
    fn parse_delta_role_only_is_none() {
        let line = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert_eq!(parse_delta(line).unwrap(), None);
    }

    #[test]
    fn parse_delta_invalid_json_errors() {
        assert!(parse_delta("not json").is_err());
    }
}
```

- [ ] **Step 2: 创建离线 `shirita-core/src/model/echo.rs`**

```rust
//! 离线 Echo 适配器：把最后一条 user 消息以 `echo: <text>` 形式逐词流式回放。
//! 用于无 API key 时的演示与确定性测试。

use async_trait::async_trait;
use futures::stream::{self, BoxStream};

use crate::models::message::Role;
use crate::Result;

use super::{ChatRequest, ModelProvider};

pub struct EchoProvider;

#[async_trait]
impl ModelProvider for EchoProvider {
    async fn stream_chat(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<String>>> {
        let last_user = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let reply = format!("echo: {last_user}");
        // split_inclusive 保留空格，拼回去 == reply。
        let chunks: Vec<Result<String>> = reply
            .split_inclusive(' ')
            .map(|s| Ok(s.to_string()))
            .collect();
        Ok(Box::pin(stream::iter(chunks)))
    }
}
```

- [ ] **Step 3: 创建真实 `shirita-core/src/model/openai.rs`**

```rust
//! OpenAI 兼容流式适配器（POST /chat/completions, stream=true）。

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;

use crate::{Error, Result};

use super::{parse_delta, ChatRequest, ModelProvider};

pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl OpenAiProvider {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }
}

#[async_trait]
impl ModelProvider for OpenAiProvider {
    async fn stream_chat(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<String>>> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": req.model,
            "stream": true,
            "messages": req.messages.iter().map(|m| serde_json::json!({
                "role": m.role.as_str(),
                "content": m.content,
            })).collect::<Vec<_>>(),
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Config(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Config(format!("provider {status}: {text}")));
        }

        // 把字节流解析为 content 增量流。
        let mut bytes = resp.bytes_stream();
        let stream = async_stream::stream! {
            let mut buf = String::new();
            while let Some(chunk) = bytes.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => { yield Err(Error::Config(format!("stream error: {e}"))); return; }
                };
                buf.push_str(&String::from_utf8_lossy(&chunk));

                // 逐行处理已完整接收的行（以 '\n' 结尾）。
                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf.drain(..=pos);

                    let data = match line.strip_prefix("data:") {
                        Some(d) => d.trim(),
                        None => continue, // 跳过空行/注释行
                    };
                    if data == "[DONE]" {
                        return;
                    }
                    match parse_delta(data) {
                        Ok(Some(content)) => yield Ok(content),
                        Ok(None) => {}
                        Err(e) => { yield Err(e); return; }
                    }
                }
            }
        };
        Ok(Box::pin(stream))
    }
}
```

- [ ] **Step 4: 在 `lib.rs` 挂载并 re-export**

`shirita-core/src/lib.rs` 增加：
```rust
pub mod model;

pub use model::{ChatMessage, ChatRequest, EchoProvider, ModelProvider, OpenAiProvider};
```

- [ ] **Step 5: 运行测试，确认通过**

Run: `cargo test -p shirita-core model::`
Expected: parse_delta 的 3 个测试通过；整 crate 编译通过。

- [ ] **Step 6: 提交**

```bash
git add shirita-core/src/model shirita-core/src/lib.rs
git commit -m "feat(m1): add ModelProvider trait, SSE parser, Echo and OpenAI adapters"
```

---

## Task 5: 对话服务 send_message（TDD，用 EchoProvider）

**Files:**
- Create: `shirita-core/src/conversation.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 创建 `shirita-core/src/conversation.rs`（SendEvent + send_message + 测试）**

```rust
//! 对话服务：发送消息并流式返回助手回复，结束时落库。

use std::sync::Arc;

use futures::{Stream, StreamExt};

use crate::model::{ChatMessage, ChatRequest, ModelProvider};
use crate::models::message::{Message, Role};
use crate::storage::Storage;
use crate::tokenizer::TokenCounter;

/// 流式发送过程对外暴露的事件。
#[derive(Debug, Clone, PartialEq)]
pub enum SendEvent {
    /// 一段文本增量。
    Delta(String),
    /// 完成，附助手消息 id。
    Done { message_id: String },
    /// 出错（流随后结束）。
    Error(String),
}

/// 发送一条 user 消息：落库 user → 组装历史 → 调用 provider 流式 → 累积 → 落库 assistant。
/// 返回一个事件流；assistant 消息在收到完整回复后写入存储，然后才 yield `Done`。
pub fn send_message(
    storage: Arc<dyn Storage>,
    provider: Arc<dyn ModelProvider>,
    counter: Arc<dyn TokenCounter>,
    model: String,
    session_id: String,
    user_text: String,
) -> impl Stream<Item = SendEvent> {
    async_stream::stream! {
        // 1) 落库 user 消息（parent = 当前末条消息）。
        let history = match storage.list_messages(&session_id).await {
            Ok(h) => h,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let last_id = history.last().map(|m| m.id.clone());
        let user_msg = Message::new(&session_id, last_id, Role::User, &user_text);
        if let Err(e) = storage.create_message(&user_msg).await {
            yield SendEvent::Error(e.to_string());
            return;
        }

        // 2) 组装请求消息（含刚落库的 user，过滤隐藏）。
        let mut chat_messages: Vec<ChatMessage> = history
            .iter()
            .filter(|m| !m.is_hidden)
            .map(|m| ChatMessage { role: m.role, content: m.raw_content.clone() })
            .collect();
        chat_messages.push(ChatMessage { role: Role::User, content: user_text.clone() });

        let prompt_text: String =
            chat_messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n");
        tracing::debug!(prompt_tokens = counter.count(&prompt_text), "assembled prompt");

        let req = ChatRequest { model, messages: chat_messages };

        // 3) 调 provider 流，逐 delta 累积 + yield。
        let mut full = String::new();
        let mut stream = match provider.stream_chat(req).await {
            Ok(s) => s,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        while let Some(item) = stream.next().await {
            match item {
                Ok(delta) => { full.push_str(&delta); yield SendEvent::Delta(delta); }
                Err(e) => { yield SendEvent::Error(e.to_string()); return; }
            }
        }

        // 4) 落库 assistant 消息，再 yield Done。
        let assistant = Message::new(&session_id, Some(user_msg.id.clone()), Role::Assistant, &full);
        if let Err(e) = storage.create_message(&assistant).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
        yield SendEvent::Done { message_id: assistant.id };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EchoProvider;
    use crate::models::session::Session;
    use crate::storage::sqlite::SqliteStorage;
    use crate::tokenizer::TiktokenCounter;

    async fn temp_storage() -> SqliteStorage {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conv.db");
        std::mem::forget(dir);
        let s = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
        s.run_migrations().await.unwrap();
        s
    }

    #[tokio::test]
    async fn echo_send_streams_and_persists() {
        let storage = Arc::new(temp_storage().await);
        let session = Session::new("t");
        storage.create_session(&session).await.unwrap();

        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        let stream = send_message(
            storage_dyn,
            provider,
            counter,
            "test-model".into(),
            session.id.clone(),
            "hello".into(),
        );
        futures::pin_mut!(stream);

        let mut deltas = String::new();
        let mut done_id = None;
        while let Some(ev) = stream.next().await {
            match ev {
                SendEvent::Delta(d) => deltas.push_str(&d),
                SendEvent::Done { message_id } => done_id = Some(message_id),
                SendEvent::Error(e) => panic!("unexpected error: {e}"),
            }
        }
        assert_eq!(deltas, "echo: hello");
        assert!(done_id.is_some());

        // 持久化校验：user + assistant 各一条，内容正确。
        let msgs = storage.list_messages(&session.id).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].raw_content, "hello");
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[1].raw_content, "echo: hello");
        assert_eq!(msgs[1].parent_id.as_deref(), Some(msgs[0].id.as_str()));
    }
}
```

- [ ] **Step 2: 在 `lib.rs` 挂载并 re-export**

`shirita-core/src/lib.rs` 增加：
```rust
pub mod conversation;

pub use conversation::{send_message, SendEvent};
```

- [ ] **Step 3: 运行测试，确认通过**

Run: `cargo test -p shirita-core conversation::`
Expected: `test result: ok. 1 passed`。

- [ ] **Step 4: 跑全部 core 测试**

Run: `cargo test -p shirita-core`
Expected: 全部通过（约 14 个）。

- [ ] **Step 5: 提交**

```bash
git add shirita-core/src/conversation.rs shirita-core/src/lib.rs
git commit -m "feat(m1): add send_message conversation service with streaming + persistence"
```

---

## Task 6: Web —— AppState 扩展、sessions/messages REST、SSE send 端点（TDD）

**Files:**
- Modify: `shirita-web/Cargo.toml`
- Modify: `shirita-web/src/state.rs`
- Create: `shirita-web/src/routes/sessions.rs`
- Create: `shirita-web/src/routes/chat.rs`
- Modify: `shirita-web/src/routes/mod.rs`
- Modify: `shirita-web/src/lib.rs`
- Create: `shirita-web/tests/sessions_test.rs`
- Create: `shirita-web/tests/chat_test.rs`

- [ ] **Step 1: 在 `shirita-web/Cargo.toml` 的 `[dependencies]` 末尾追加**

```toml
futures = "0.3"
```

- [ ] **Step 2: 扩展 AppState（`shirita-web/src/state.rs` 替换全文）**

```rust
use std::sync::Arc;

use shirita_core::{Config, ModelProvider, Storage, TokenCounter};

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage>,
    pub config: Arc<Config>,
    pub provider: Arc<dyn ModelProvider>,
    pub token_counter: Arc<dyn TokenCounter>,
    pub model: String,
}
```

- [ ] **Step 3: 创建 sessions REST 处理器 `shirita-web/src/routes/sessions.rs`**

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use shirita_core::models::message::Message;
use shirita_core::models::session::Session;

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateSession {
    pub name: String,
}

pub async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSession>,
) -> Result<Json<Session>, StatusCode> {
    let session = Session::new(body.name);
    state
        .storage
        .create_session(&session)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(session))
}

pub async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<Session>>, StatusCode> {
    let sessions = state
        .storage
        .list_sessions()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(sessions))
}

pub async fn list_messages(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<Message>>, StatusCode> {
    let msgs = state
        .storage
        .list_messages(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(msgs))
}
```

- [ ] **Step 4: 创建 SSE send 端点 `shirita-web/src/routes/chat.rs`**

```rust
use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;

use shirita_core::{send_message, SendEvent};

use crate::AppState;

#[derive(Deserialize)]
pub struct SendBody {
    pub text: String,
}

pub async fn send(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<SendBody>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let events = send_message(
        state.storage.clone(),
        state.provider.clone(),
        state.token_counter.clone(),
        state.model.clone(),
        session_id,
        body.text,
    );

    let sse = events.map(|ev| {
        let payload = match ev {
            SendEvent::Delta(text) => json!({ "type": "delta", "text": text }),
            SendEvent::Done { message_id } => json!({ "type": "done", "message_id": message_id }),
            SendEvent::Error(message) => json!({ "type": "error", "message": message }),
        };
        Ok(Event::default().data(payload.to_string()))
    });

    Sse::new(sse)
}
```

- [ ] **Step 5: 挂载路由（`routes/mod.rs` 与 `lib.rs`）**

`shirita-web/src/routes/mod.rs`（替换全文）:
```rust
pub mod chat;
pub mod health;
pub mod ping;
pub mod sessions;
```

`shirita-web/src/lib.rs` 的 `app` 函数替换为（把新端点加进受保护的 `/api`）：
```rust
/// 构建应用路由。`/health` 公开；`/api/*` 走 Bearer 中间件。
pub fn app(state: AppState) -> Router {
    let protected = Router::new()
        .route("/ping", get(routes::ping::ping))
        .route("/sessions", get(routes::sessions::list_sessions).post(routes::sessions::create_session))
        .route("/sessions/{id}/messages", get(routes::sessions::list_messages).post(routes::chat::send))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_bearer,
        ));

    Router::new()
        .route("/health", get(routes::health::health))
        .nest("/api", protected)
        .with_state(state)
}
```

- [ ] **Step 6: 创建 sessions 集成测试 `shirita-web/tests/sessions_test.rs`**

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
    let path = dir.path().join("sess_test.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState { storage, config, provider, token_counter, model: "test-model".into() }
}

fn auth(req: Request<Body>) -> Request<Body> {
    let (mut parts, body) = req.into_parts();
    parts.headers.insert(header::AUTHORIZATION, "Bearer secret-token".parse().unwrap());
    Request::from_parts(parts, body)
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

    let req = Request::builder().uri("/api/sessions").body(Body::empty()).unwrap();
    let res = app(state).oneshot(auth(req)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
}
```

- [ ] **Step 7: 创建 SSE 流式测试 `shirita-web/tests/chat_test.rs`**

```rust
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use shirita_core::{Config, EchoProvider, ModelProvider, Session, SqliteStorage, Storage, TiktokenCounter, TokenCounter};
use shirita_web::{app, AppState};

async fn state_with_session() -> (AppState, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat_test.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let session = Session::new("c");
    storage.create_session(&session).await.unwrap();

    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    let state = AppState { storage, config, provider, token_counter, model: "m".into() };
    (state, session.id)
}

#[tokio::test]
async fn send_streams_echo_and_persists() {
    let (state, session_id) = state_with_session().await;

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/sessions/{session_id}/messages"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .body(Body::from(r#"{"text":"hello"}"#))
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 收集整段 SSE body，断言含 echo 分片与 done。
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains(r#""type":"delta""#), "should contain delta events: {text}");
    assert!(text.contains("echo:"), "should echo the input: {text}");
    assert!(text.contains(r#""type":"done""#), "should end with done: {text}");

    // 落库校验。
    let req = Request::builder()
        .uri(format!("/api/sessions/{session_id}/messages"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .body(Body::empty())
        .unwrap();
    let res = app(state).oneshot(req).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let msgs: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = msgs.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["role"], "user");
    assert_eq!(arr[1]["role"], "assistant");
    assert_eq!(arr[1]["raw_content"], "echo: hello");
}
```

- [ ] **Step 8: 运行 web 测试，确认通过**

Run: `cargo test -p shirita-web`
Expected: api_test(4) + sessions_test(1) + chat_test(1) 全部通过。
> 备注：若编译报 `Session`/`Message` 未从 `shirita_core` 导出，确认 Task 1 Step 5 已 `pub use models::session::Session;`，并按需在测试里用 `shirita_core::models::message::Message`。

- [ ] **Step 9: 提交**

```bash
git add shirita-web/Cargo.toml shirita-web/src/state.rs shirita-web/src/routes shirita-web/src/lib.rs shirita-web/tests
git commit -m "feat(m1): web sessions/messages REST and SSE send endpoint"
```

---

## Task 7: 极薄验证页、入口装配与离线冒烟

**Files:**
- Create: `shirita-web/static/index.html`
- Create: `shirita-web/src/routes/index.rs`
- Modify: `shirita-web/src/routes/mod.rs`
- Modify: `shirita-web/src/lib.rs`
- Modify: `shirita-web/src/main.rs`

- [ ] **Step 1: 创建极薄验证页 `shirita-web/static/index.html`**

```html
<!doctype html>
<html lang="zh">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Shirita M1 验证页</title>
  <style>
    body { font-family: system-ui, sans-serif; max-width: 720px; margin: 2rem auto; padding: 0 1rem; }
    #log { white-space: pre-wrap; border: 1px solid #ccc; border-radius: 8px; padding: 1rem; min-height: 160px; }
    input, button { font-size: 1rem; padding: .4rem .6rem; }
    .row { display: flex; gap: .5rem; margin: .5rem 0; }
    .row input { flex: 1; }
  </style>
</head>
<body>
  <h1>Shirita — M1 最小端到端对话</h1>
  <div class="row">
    <input id="token" placeholder="Bearer token" value="devtoken" />
    <button onclick="createSession()">新建会话</button>
  </div>
  <div class="row">
    <input id="msg" placeholder="说点什么…" value="hello" />
    <button onclick="send()">发送</button>
  </div>
  <div id="log"></div>

  <script>
    let sessionId = null;
    const log = (s) => { document.getElementById('log').textContent += s; };
    const tok = () => document.getElementById('token').value;

    async function createSession() {
      const res = await fetch('/api/sessions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + tok() },
        body: JSON.stringify({ name: 'web ' + new Date().toISOString() }),
      });
      const s = await res.json();
      sessionId = s.id;
      log('\n[会话已创建] ' + sessionId + '\n');
    }

    async function send() {
      if (!sessionId) { await createSession(); }
      const text = document.getElementById('msg').value;
      log('\n你: ' + text + '\n助手: ');
      const res = await fetch('/api/sessions/' + sessionId + '/messages', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + tok() },
        body: JSON.stringify({ text }),
      });
      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buf = '';
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        buf += decoder.decode(value, { stream: true });
        let idx;
        while ((idx = buf.indexOf('\n\n')) >= 0) {
          const frame = buf.slice(0, idx); buf = buf.slice(idx + 2);
          for (const line of frame.split('\n')) {
            if (!line.startsWith('data:')) continue;
            const payload = JSON.parse(line.slice(5).trim());
            if (payload.type === 'delta') log(payload.text);
            else if (payload.type === 'done') log('\n[done ' + payload.message_id + ']\n');
            else if (payload.type === 'error') log('\n[error] ' + payload.message + '\n');
          }
        }
      }
    }
  </script>
</body>
</html>
```

- [ ] **Step 2: 创建 `shirita-web/src/routes/index.rs`**

```rust
use axum::response::Html;

pub async fn index() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}
```

- [ ] **Step 3: 挂载 index 路由（`routes/mod.rs` 与 `lib.rs`）**

`shirita-web/src/routes/mod.rs` 增加一行：
```rust
pub mod index;
```

`shirita-web/src/lib.rs` 的最外层 Router 增加 `/` 路由（公开），即把：
```rust
    Router::new()
        .route("/health", get(routes::health::health))
        .nest("/api", protected)
        .with_state(state)
```
替换为：
```rust
    Router::new()
        .route("/", get(routes::index::index))
        .route("/health", get(routes::health::health))
        .nest("/api", protected)
        .with_state(state)
```

- [ ] **Step 4: 装配入口 `shirita-web/src/main.rs`（替换全文）**

```rust
use std::sync::Arc;

use shirita_core::{
    Config, EchoProvider, ModelProvider, OpenAiProvider, SqliteStorage, Storage, TiktokenCounter,
    TokenCounter,
};
use shirita_web::{app, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let storage = SqliteStorage::connect(&config.database_path).await?;
    storage.run_migrations().await?;

    // 无 API key → 离线 Echo；有 key → 真实 OpenAI 兼容接口。
    let provider: Arc<dyn ModelProvider> = if config.openai_api_key.is_empty() {
        tracing::info!("OPENAI_API_KEY empty: using offline EchoProvider");
        Arc::new(EchoProvider)
    } else {
        tracing::info!("using OpenAiProvider at {}", config.openai_base_url);
        Arc::new(OpenAiProvider::new(
            config.openai_base_url.clone(),
            config.openai_api_key.clone(),
        ))
    };

    let model = config.openai_model.clone();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    let state = AppState {
        storage,
        config: Arc::new(config),
        provider,
        token_counter,
        model,
    };

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("shirita-web listening on {bind_addr}");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
```

- [ ] **Step 5: 全量构建 + 测试**

Run: `cargo test --workspace`
Expected: 全部通过（core ~14 + web 6）。

- [ ] **Step 6: 离线冒烟（EchoProvider，无需 key）**

后台启动：
```bash
TOKEN_SECRET=devtoken DATABASE_PATH=/home/cc/.claude/jobs/b73d9870/tmp/m1-smoke.db BIND_ADDR=127.0.0.1:8788 cargo run -q -p shirita-web
```
另一终端：
```bash
# 建会话，取 id
SID=$(curl -s -H "Authorization: Bearer devtoken" -H "Content-Type: application/json" \
  -d '{"name":"smoke"}' http://127.0.0.1:8788/api/sessions | python3 -c 'import sys,json;print(json.load(sys.stdin)["id"])')
echo "session=$SID"
# 发消息，观察 SSE 流
curl -N -s -H "Authorization: Bearer devtoken" -H "Content-Type: application/json" \
  -d '{"text":"hello"}' http://127.0.0.1:8788/api/sessions/$SID/messages
echo
# 校验落库
curl -s -H "Authorization: Bearer devtoken" http://127.0.0.1:8788/api/sessions/$SID/messages
```
Expected: SSE 输出多帧 `data: {"type":"delta","text":"echo:"/" hello"...}` 然后 `data: {"type":"done",...}`；最后 GET 返回 user + assistant 两条，assistant 的 `raw_content` 为 `echo: hello`。
停止服务并清理冒烟 db。

- [ ] **Step 7: 提交**

```bash
git add shirita-web/static shirita-web/src
git commit -m "feat(m1): thin verification page, index route, provider wiring in entrypoint"
```

---

## M1 完成判定（Definition of Done）

- [ ] `cargo test --workspace` 全绿（core ~14 + web 6）。
- [ ] 离线（EchoProvider）下：浏览器打开 `/`，新建会话 → 发消息 → 看到 `echo: <text>` 流式逐词返回，刷新后历史仍在（已持久化）。
- [ ] sessions/messages REST 正常：创建/列出会话、列出消息。
- [ ] SSE 端点输出 `delta…done` 帧；助手消息以正确 `parent_id` 落库。
- [ ] 所有改动已在 `main` 提交。

### 可选：真实 OpenAI 兼容接口在线冒烟（需用户提供凭据）
设置 `OPENAI_API_KEY`（必要时 `OPENAI_BASE_URL` / `OPENAI_MODEL`）后重启服务，在验证页发消息，应看到**真实模型**的流式回复。此步在执行到 Task 7 之后、需要时再向用户索取 key/endpoint。

## 自检备注（Self-Review）

- **Spec 覆盖**：roadmap M1 的 7 项 —— ModelProvider trait + OpenAI 流式适配器(T4)、sessions/messages CRUD(T2)、最简 prompt 组装(T5 的 `send_message` 内)、`send_message` 返回流 + 落库(T5)、Web SSE + REST(T6)、TokenCounter + tiktoken(T3)、极薄前端验证页(T7)。完成标志=DoD。
- **类型一致性**：`ModelProvider::stream_chat -> BoxStream<'static, Result<String>>`、`ChatRequest{model,messages}`/`ChatMessage{role,content}`、`SendEvent::{Delta,Done{message_id},Error}`、`send_message(storage,provider,counter,model,session_id,user_text)`、`AppState{storage,config,provider,token_counter,model}`、`Storage` 新增 5 方法、`Role::{as_str,from_db}`、`Message::new`/`Session::new`、`TokenCounter::count`、`parse_delta` 在各 Task 间签名一致。
- **离线可跑**：EchoProvider 让 DoD 的浏览器验证与所有测试不依赖网络/key。
- **无占位符**：所有代码步骤均为完整代码与确切命令/期望输出。
```

