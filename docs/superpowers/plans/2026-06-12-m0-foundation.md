# M0 — 地基 / 脚手架 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 搭起 Shirita 的 Cargo workspace 地基：`shirita-core`（含 Config、Definition 模型、Storage trait + SQLite 实现、迁移）与 `shirita-web`（Axum：health 路由 + Bearer 鉴权链），使服务可启动、迁移可执行、definitions CRUD 可往返。

**Architecture:** 两个 crate 的 workspace。`shirita-core` 是纯业务库，对外暴露 `Config`、`Definition` 模型与 `Storage` trait（SQLite 实现）。`shirita-web` 是薄 Axum 适配层，持有 `AppState { storage, config }`，公开未鉴权的 `/health` 与受 Bearer 中间件保护的 `/api/ping`。core 与 web 通过 trait 解耦，均可独立单测。

**Tech Stack:** Rust 1.95 / Cargo workspace · sqlx 0.8（SQLite, WAL, **runtime query API，不用 `query!` 宏**，故无需 `DATABASE_URL`/sqlx-cli）· Axum 0.8 · tokio · async-trait · serde/serde_json · uuid · thiserror · tempfile（测试）。

---

## 前置说明（实现者必读）

- **不使用 sqlx 的编译期检查宏**（`query!`/`query_as!`）。一律用运行时 API（`sqlx::query(...)` + `.bind(...)` + `Row::try_get`）。这样**编译期不需要 `DATABASE_URL`，也不需要 `sqlx-cli`**。
- 迁移用 `sqlx::migrate!("./migrations")`，目录是 **`shirita-core/migrations/`**（宏路径相对于 crate 的 manifest 目录）。
- 首次 `cargo build` 需要联网拉取依赖。
- 全程在 `main` 分支工作（仓库已 `git init`）。每个 Task 末尾提交一次。
- 测试用 `tempfile` 建临时 DB 文件，路径与生产一致（避免 in-memory 单连接的坑）。

## 文件结构（本里程碑创建/修改）

```
shirita/
├── Cargo.toml                         # 创建：workspace 清单 + workspace.dependencies
├── .gitignore                         # 创建
├── shirita-core/
│   ├── Cargo.toml                     # 创建
│   ├── migrations/0001_init.sql       # 创建：三张表 + 索引
│   └── src/
│       ├── lib.rs                     # 创建：模块装配 + re-export
│       ├── error.rs                   # 创建：Error / Result
│       ├── config.rs                  # 创建：Config（new + from_env）
│       ├── models/
│       │   ├── mod.rs                 # 创建
│       │   └── definition.rs          # 创建：Definition / DefinitionType
│       └── storage/
│           ├── mod.rs                 # 创建：Storage trait
│           └── sqlite.rs              # 创建：SqliteStorage + CRUD + 单测
└── shirita-web/
    ├── Cargo.toml                     # 创建
    ├── src/
    │   ├── lib.rs                     # 创建：app() 路由装配 + re-export
    │   ├── state.rs                   # 创建：AppState
    │   ├── auth.rs                    # 创建：require_bearer 中间件
    │   ├── main.rs                    # 创建：入口（load config → connect → migrate → serve）
    │   └── routes/
    │       ├── mod.rs                 # 创建
    │       ├── health.rs              # 创建：GET /health
    │       └── ping.rs                # 创建：GET /api/ping（受保护）
    └── tests/
        └── api_test.rs                # 创建：health/ping 鉴权集成测试
```

---

## Task 1: Workspace 脚手架与编译冒烟

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `shirita-core/Cargo.toml`
- Create: `shirita-core/src/lib.rs`
- Create: `shirita-web/Cargo.toml`
- Create: `shirita-web/src/lib.rs`
- Create: `shirita-web/src/main.rs`

- [ ] **Step 1: 创建根 workspace 清单 `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["shirita-core", "shirita-web"]

[workspace.package]
edition = "2021"
version = "0.0.0"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
thiserror = "2"
uuid = { version = "1", features = ["v4"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "migrate"] }
tracing = "0.1"
```

- [ ] **Step 2: 创建 `.gitignore`**

```gitignore
/target
**/*.rs.bk
*.db
*.db-shm
*.db-wal
/assets/
.env
```

- [ ] **Step 3: 创建 `shirita-core/Cargo.toml`**

```toml
[package]
name = "shirita-core"
edition.workspace = true
version.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
async-trait.workspace = true
thiserror.workspace = true
uuid.workspace = true
sqlx.workspace = true

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 4: 创建占位 `shirita-core/src/lib.rs`（后续 Task 填充模块）**

```rust
//! shirita-core: 业务核心库（存储、模型、上下文工程……）
```

- [ ] **Step 5: 创建 `shirita-web/Cargo.toml`**

```toml
[package]
name = "shirita-web"
edition.workspace = true
version.workspace = true

[dependencies]
shirita-core = { path = "../shirita-core" }
axum = "0.8"
tokio.workspace = true
tower = "0.5"
tower-http = { version = "0.6", features = ["trace"] }
serde.workspace = true
serde_json.workspace = true
tracing.workspace = true
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tempfile = "3"
tower = "0.5"
http-body-util = "0.1"
```

- [ ] **Step 6: 创建占位 `shirita-web/src/lib.rs` 与 `shirita-web/src/main.rs`**

`shirita-web/src/lib.rs`:
```rust
//! shirita-web: Axum 适配层（REST + SSE + 静态文件 + 鉴权）
```

`shirita-web/src/main.rs`:
```rust
fn main() {
    println!("shirita-web placeholder");
}
```

- [ ] **Step 7: 编译冒烟**

Run: `cargo build`
Expected: 联网拉依赖后编译通过（`Finished` ...）。无错误。

- [ ] **Step 8: 提交**

```bash
git add Cargo.toml .gitignore shirita-core shirita-web
git commit -m "chore(m0): scaffold cargo workspace (shirita-core + shirita-web)"
```

---

## Task 2: 核心 Error 类型

**Files:**
- Create: `shirita-core/src/error.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 创建 `shirita-core/src/error.rs`**

```rust
//! 核心错误类型。

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("invalid definition type: {0}")]
    InvalidDefinitionType(String),

    #[error("config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 2: 在 `lib.rs` 挂载并 re-export**

`shirita-core/src/lib.rs`（替换全文）:
```rust
//! shirita-core: 业务核心库（存储、模型、上下文工程……）

pub mod error;

pub use error::{Error, Result};
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p shirita-core`
Expected: 编译通过，无错误（允许 dead_code 警告）。

- [ ] **Step 4: 提交**

```bash
git add shirita-core/src/error.rs shirita-core/src/lib.rs
git commit -m "feat(m0): add core Error/Result types"
```

---

## Task 3: Config（TDD）

**Files:**
- Create: `shirita-core/src/config.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 写失败测试（放在 `config.rs` 内 `#[cfg(test)]`）**

先创建 `shirita-core/src/config.rs`，仅含测试与待实现的签名占位：
```rust
//! 运行时配置：DATABASE_PATH / ASSETS_DIR / TOKEN_SECRET。

pub struct Config {
    pub database_path: String,
    pub assets_dir: String,
    pub token_secret: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty_token() {
        let err = Config::new("db.sqlite", "./assets", "   ");
        assert!(err.is_err(), "empty/whitespace token must be rejected");
    }

    #[test]
    fn new_keeps_fields() {
        let cfg = Config::new("db.sqlite", "./assets", "secret").unwrap();
        assert_eq!(cfg.database_path, "db.sqlite");
        assert_eq!(cfg.assets_dir, "./assets");
        assert_eq!(cfg.token_secret, "secret");
    }
}
```

- [ ] **Step 2: 在 `lib.rs` 挂载模块（让测试能编译）**

`shirita-core/src/lib.rs` 增加：
```rust
pub mod config;

pub use config::Config;
```
（与已有的 `pub mod error;` / `pub use error::...` 并列）

- [ ] **Step 3: 运行测试，确认失败**

Run: `cargo test -p shirita-core config::`
Expected: 编译失败 —— `no function or associated item named 'new' found for struct 'Config'`。

- [ ] **Step 4: 实现 `Config::new` 与 `Config::from_env`**

在 `config.rs` 的 `struct Config { ... }` 之后、`#[cfg(test)]` 之前插入：
```rust
use crate::{Error, Result};

impl Config {
    pub fn new(
        database_path: impl Into<String>,
        assets_dir: impl Into<String>,
        token_secret: impl Into<String>,
    ) -> Result<Self> {
        let token_secret = token_secret.into();
        if token_secret.trim().is_empty() {
            return Err(Error::Config("TOKEN_SECRET must not be empty".into()));
        }
        Ok(Self {
            database_path: database_path.into(),
            assets_dir: assets_dir.into(),
            token_secret,
        })
    }

    /// 从环境变量读取；DATABASE_PATH/ASSETS_DIR 有默认值，TOKEN_SECRET 必填。
    pub fn from_env() -> Result<Self> {
        let database_path =
            std::env::var("DATABASE_PATH").unwrap_or_else(|_| "shirita.db".into());
        let assets_dir = std::env::var("ASSETS_DIR").unwrap_or_else(|_| "./assets".into());
        let token_secret = std::env::var("TOKEN_SECRET")
            .map_err(|_| Error::Config("TOKEN_SECRET env var is required".into()))?;
        Self::new(database_path, assets_dir, token_secret)
    }
}
```

- [ ] **Step 5: 运行测试，确认通过**

Run: `cargo test -p shirita-core config::`
Expected: `test result: ok. 2 passed`。

- [ ] **Step 6: 提交**

```bash
git add shirita-core/src/config.rs shirita-core/src/lib.rs
git commit -m "feat(m0): add Config with validation and from_env"
```

---

## Task 4: Definition 模型与 DefinitionType（TDD）

**Files:**
- Create: `shirita-core/src/models/mod.rs`
- Create: `shirita-core/src/models/definition.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 写失败测试**

创建 `shirita-core/src/models/definition.rs`，先只放类型骨架 + 测试：
```rust
//! Definition 模型与类型标签。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionType {
    Char,
    Prompt,
    World,
    Item,
    Persona,
    RegexRule,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    pub id: String,
    #[serde(rename = "type")]
    pub def_type: DefinitionType,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub meta: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_db_roundtrip() {
        for (variant, s) in [
            (DefinitionType::Char, "char"),
            (DefinitionType::Prompt, "prompt"),
            (DefinitionType::World, "world"),
            (DefinitionType::Item, "item"),
            (DefinitionType::Persona, "persona"),
            (DefinitionType::RegexRule, "regex_rule"),
            (DefinitionType::Tool, "tool"),
        ] {
            assert_eq!(variant.as_str(), s);
            assert_eq!(DefinitionType::from_db(s).unwrap(), variant);
        }
    }

    #[test]
    fn unknown_type_errors() {
        assert!(DefinitionType::from_db("nope").is_err());
    }

    #[test]
    fn new_definition_has_uuid_and_empty_meta() {
        let d = Definition::new(DefinitionType::Char, "Alice", "<char>...</char>");
        assert_eq!(d.def_type, DefinitionType::Char);
        assert_eq!(d.name, "Alice");
        assert_eq!(d.content, "<char>...</char>");
        assert_eq!(d.meta, serde_json::json!({}));
        assert_eq!(d.id.len(), 36, "uuid v4 string is 36 chars");
    }
}
```

- [ ] **Step 2: 创建 `shirita-core/src/models/mod.rs` 并在 `lib.rs` 挂载**

`shirita-core/src/models/mod.rs`:
```rust
pub mod definition;
```

`shirita-core/src/lib.rs` 增加：
```rust
pub mod models;

pub use models::definition::{Definition, DefinitionType};
```

- [ ] **Step 3: 运行测试，确认失败**

Run: `cargo test -p shirita-core definition::`
Expected: 编译失败 —— `no function or associated item named 'as_str'`（及 `from_db` / `new`）。

- [ ] **Step 4: 实现 `as_str` / `from_db` / `Definition::new`**

在 `definition.rs` 的 `enum DefinitionType { ... }` 之后插入：
```rust
impl DefinitionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DefinitionType::Char => "char",
            DefinitionType::Prompt => "prompt",
            DefinitionType::World => "world",
            DefinitionType::Item => "item",
            DefinitionType::Persona => "persona",
            DefinitionType::RegexRule => "regex_rule",
            DefinitionType::Tool => "tool",
        }
    }

    pub fn from_db(s: &str) -> crate::Result<Self> {
        Ok(match s {
            "char" => DefinitionType::Char,
            "prompt" => DefinitionType::Prompt,
            "world" => DefinitionType::World,
            "item" => DefinitionType::Item,
            "persona" => DefinitionType::Persona,
            "regex_rule" => DefinitionType::RegexRule,
            "tool" => DefinitionType::Tool,
            other => {
                return Err(crate::Error::InvalidDefinitionType(other.to_string()))
            }
        })
    }
}
```

在 `struct Definition { ... }` 之后插入：
```rust
impl Definition {
    pub fn new(
        def_type: DefinitionType,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            def_type,
            name: name.into(),
            content: content.into(),
            meta: serde_json::json!({}),
        }
    }
}
```

- [ ] **Step 5: 运行测试，确认通过**

Run: `cargo test -p shirita-core definition::`
Expected: `test result: ok. 3 passed`。

- [ ] **Step 6: 提交**

```bash
git add shirita-core/src/models shirita-core/src/lib.rs
git commit -m "feat(m0): add Definition model and DefinitionType"
```

---

## Task 5: 迁移 SQL 与迁移运行器（TDD）

**Files:**
- Create: `shirita-core/migrations/0001_init.sql`
- Create: `shirita-core/src/storage/mod.rs`
- Create: `shirita-core/src/storage/sqlite.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 创建迁移 `shirita-core/migrations/0001_init.sql`**

```sql
CREATE TABLE IF NOT EXISTS definitions (
    id      TEXT PRIMARY KEY,
    type    TEXT NOT NULL,
    name    TEXT NOT NULL,
    content TEXT NOT NULL,
    meta    TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS chat_sessions (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    avatar          TEXT,
    override_config TEXT NOT NULL DEFAULT '{}',
    current_state   TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS messages (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    parent_id       TEXT REFERENCES messages(id) ON DELETE CASCADE,
    role            TEXT NOT NULL,
    raw_content     TEXT NOT NULL,
    display_content TEXT,
    is_hidden       INTEGER NOT NULL DEFAULT 0,
    snapshot_state  TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_definitions_type ON definitions(type);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
CREATE INDEX IF NOT EXISTS idx_messages_parent  ON messages(parent_id);
```

- [ ] **Step 2: 写失败测试（在 `sqlite.rs` 内）**

创建 `shirita-core/src/storage/sqlite.rs`，先放连接/迁移骨架占位 + 测试：
```rust
//! SqliteStorage：连接、迁移与 definitions CRUD。

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::Result;

#[derive(Clone)]
pub struct SqliteStorage {
    pool: SqlitePool,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn temp_storage() -> SqliteStorage {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        // 让临时目录在整个测试进程存活，避免连接期间被删除。
        std::mem::forget(dir);
        let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
        storage.run_migrations().await.unwrap();
        storage
    }

    #[tokio::test]
    async fn migrations_create_tables() {
        let storage = temp_storage().await;
        // 查询 sqlite_master 确认三张表都存在。
        for table in ["definitions", "chat_sessions", "messages"] {
            let row: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
            )
            .bind(table)
            .fetch_one(storage.pool())
            .await
            .unwrap();
            assert_eq!(row.0, 1, "table {table} should exist");
        }
    }
}
```

- [ ] **Step 3: 创建 `storage/mod.rs` 并在 `lib.rs` 挂载**

`shirita-core/src/storage/mod.rs`:
```rust
pub mod sqlite;
```

`shirita-core/src/lib.rs` 增加：
```rust
pub mod storage;

pub use storage::sqlite::SqliteStorage;
```

- [ ] **Step 4: 运行测试，确认失败**

Run: `cargo test -p shirita-core sqlite::`
Expected: 编译失败 —— `no function or associated item named 'connect'`（及 `run_migrations` / `pool`）。

- [ ] **Step 5: 实现 connect / run_migrations / pool**

在 `sqlite.rs` 的 `struct SqliteStorage { ... }` 之后插入：
```rust
impl SqliteStorage {
    pub async fn connect(database_path: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;
        Ok(Self { pool })
    }

    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
```

- [ ] **Step 6: 运行测试，确认通过**

Run: `cargo test -p shirita-core sqlite::`
Expected: `test result: ok. 1 passed`。

- [ ] **Step 7: 提交**

```bash
git add shirita-core/migrations shirita-core/src/storage shirita-core/src/lib.rs
git commit -m "feat(m0): add SQLite migrations and connection/migration runner"
```

---

## Task 6: Storage trait 与 definitions CRUD（TDD）

**Files:**
- Modify: `shirita-core/src/storage/mod.rs`
- Modify: `shirita-core/src/storage/sqlite.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: 在 `sqlite.rs` 追加 CRUD 往返失败测试**

在 `sqlite.rs` 的 `#[cfg(test)] mod tests { ... }` 内、`migrations_create_tables` 之后追加：
```rust
    use crate::models::definition::{Definition, DefinitionType};
    use crate::Storage;

    #[tokio::test]
    async fn definition_crud_roundtrip() {
        let storage = temp_storage().await;

        // create
        let mut def = Definition::new(DefinitionType::Char, "Alice", "<char>hi</char>");
        def.meta = serde_json::json!({ "avatar": "/a.png" });
        storage.create_definition(&def).await.unwrap();

        // get
        let got = storage.get_definition(&def.id).await.unwrap().unwrap();
        assert_eq!(got, def);

        // list
        let all = storage.list_definitions().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, def.id);

        // update
        let mut updated = def.clone();
        updated.name = "Alicia".into();
        updated.def_type = DefinitionType::Persona;
        storage.update_definition(&updated).await.unwrap();
        let got = storage.get_definition(&def.id).await.unwrap().unwrap();
        assert_eq!(got.name, "Alicia");
        assert_eq!(got.def_type, DefinitionType::Persona);

        // delete
        storage.delete_definition(&def.id).await.unwrap();
        assert!(storage.get_definition(&def.id).await.unwrap().is_none());
        assert!(storage.list_definitions().await.unwrap().is_empty());
    }
```

- [ ] **Step 2: 定义 `Storage` trait（`storage/mod.rs`）**

`shirita-core/src/storage/mod.rs`（替换全文）:
```rust
use async_trait::async_trait;

use crate::models::definition::Definition;
use crate::Result;

pub mod sqlite;

/// 存储抽象层。M0 仅覆盖 definitions；后续里程碑扩展 sessions/messages。
#[async_trait]
pub trait Storage: Send + Sync {
    async fn create_definition(&self, def: &Definition) -> Result<()>;
    async fn get_definition(&self, id: &str) -> Result<Option<Definition>>;
    async fn list_definitions(&self) -> Result<Vec<Definition>>;
    async fn update_definition(&self, def: &Definition) -> Result<()>;
    async fn delete_definition(&self, id: &str) -> Result<()>;
}
```

- [ ] **Step 3: 在 `lib.rs` re-export `Storage` trait**

`shirita-core/src/lib.rs` 把 storage 的 re-export 改为：
```rust
pub use storage::{sqlite::SqliteStorage, Storage};
```

- [ ] **Step 4: 运行测试，确认失败**

Run: `cargo test -p shirita-core sqlite::definition_crud`
Expected: 编译失败 —— `SqliteStorage` 未实现 `Storage` trait（方法缺失）。

- [ ] **Step 5: 为 `SqliteStorage` 实现 `Storage`（行映射 + CRUD）**

在 `sqlite.rs` 顶部 `use` 区补充导入（与现有 use 合并）：
```rust
use async_trait::async_trait;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

use crate::models::definition::{Definition, DefinitionType};
use crate::Storage;
```

在 `impl SqliteStorage { ... }` 之后、`#[cfg(test)]` 之前插入：
```rust
fn row_to_definition(row: &SqliteRow) -> Result<Definition> {
    let type_str: String = row.try_get("type")?;
    let meta_str: String = row.try_get("meta")?;
    Ok(Definition {
        id: row.try_get("id")?,
        def_type: DefinitionType::from_db(&type_str)?,
        name: row.try_get("name")?,
        content: row.try_get("content")?,
        meta: serde_json::from_str(&meta_str)?,
    })
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn create_definition(&self, def: &Definition) -> Result<()> {
        let meta = serde_json::to_string(&def.meta)?;
        sqlx::query(
            "INSERT INTO definitions (id, type, name, content, meta) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&def.id)
        .bind(def.def_type.as_str())
        .bind(&def.name)
        .bind(&def.content)
        .bind(meta)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_definition(&self, id: &str) -> Result<Option<Definition>> {
        let row = sqlx::query("SELECT id, type, name, content, meta FROM definitions WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(r) => Ok(Some(row_to_definition(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_definitions(&self) -> Result<Vec<Definition>> {
        let rows =
            sqlx::query("SELECT id, type, name, content, meta FROM definitions ORDER BY name")
                .fetch_all(&self.pool)
                .await?;
        rows.iter().map(row_to_definition).collect()
    }

    async fn update_definition(&self, def: &Definition) -> Result<()> {
        let meta = serde_json::to_string(&def.meta)?;
        sqlx::query(
            "UPDATE definitions SET type = ?, name = ?, content = ?, meta = ? WHERE id = ?",
        )
        .bind(def.def_type.as_str())
        .bind(&def.name)
        .bind(&def.content)
        .bind(meta)
        .bind(&def.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_definition(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM definitions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

- [ ] **Step 6: 运行全部 core 测试，确认通过**

Run: `cargo test -p shirita-core`
Expected: 全部通过（config 2 + definition 3 + sqlite 2 = 7）。

- [ ] **Step 7: 提交**

```bash
git add shirita-core/src/storage shirita-core/src/lib.rs
git commit -m "feat(m0): add Storage trait and SQLite definitions CRUD"
```

---

## Task 7: Web AppState、health 与 ping 路由、Bearer 鉴权（TDD）

**Files:**
- Create: `shirita-web/src/state.rs`
- Create: `shirita-web/src/auth.rs`
- Create: `shirita-web/src/routes/mod.rs`
- Create: `shirita-web/src/routes/health.rs`
- Create: `shirita-web/src/routes/ping.rs`
- Modify: `shirita-web/src/lib.rs`
- Create: `shirita-web/tests/api_test.rs`

- [ ] **Step 1: 写失败集成测试 `shirita-web/tests/api_test.rs`**

```rust
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt; // oneshot

use shirita_core::{Config, SqliteStorage, Storage};
use shirita_web::{app, AppState};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("web_test.db");
    std::mem::forget(dir);
    let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", "./assets", "secret-token").unwrap());
    AppState { storage, config }
}

#[tokio::test]
async fn health_is_public() {
    let res = app(test_state().await)
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn ping_requires_token() {
    let res = app(test_state().await)
        .oneshot(Request::builder().uri("/api/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ping_rejects_wrong_token() {
    let res = app(test_state().await)
        .oneshot(
            Request::builder()
                .uri("/api/ping")
                .header(header::AUTHORIZATION, "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ping_accepts_correct_token() {
    let res = app(test_state().await)
        .oneshot(
            Request::builder()
                .uri("/api/ping")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["pong"], true);
}
```

- [ ] **Step 2: 创建 AppState `shirita-web/src/state.rs`**

```rust
use std::sync::Arc;

use shirita_core::{Config, Storage};

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage>,
    pub config: Arc<Config>,
}
```

- [ ] **Step 3: 创建路由 handler**

`shirita-web/src/routes/mod.rs`:
```rust
pub mod health;
pub mod ping;
```

`shirita-web/src/routes/health.rs`:
```rust
use axum::Json;
use serde_json::{json, Value};

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
```

`shirita-web/src/routes/ping.rs`:
```rust
use axum::Json;
use serde_json::{json, Value};

pub async fn ping() -> Json<Value> {
    Json(json!({ "pong": true }))
}
```

- [ ] **Step 4: 创建鉴权中间件 `shirita-web/src/auth.rs`**

```rust
use axum::extract::{Request, State};
use axum::http::{header::AUTHORIZATION, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;

/// 校验 `Authorization: Bearer <token>` 是否等于配置中的静态 token。
/// MVP 阶段为简单字符串比较；链路必须存在（后续可替换为更强校验）。
pub async fn require_bearer(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match provided {
        Some(token) if token == state.config.token_secret => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
```

- [ ] **Step 5: 装配路由 `shirita-web/src/lib.rs`（替换全文）**

```rust
//! shirita-web: Axum 适配层（REST + SSE + 静态文件 + 鉴权）

pub mod auth;
pub mod routes;
pub mod state;

pub use state::AppState;

use axum::{middleware, routing::get, Router};

/// 构建应用路由。`/health` 公开；`/api/*` 走 Bearer 中间件。
pub fn app(state: AppState) -> Router {
    let protected = Router::new()
        .route("/ping", get(routes::ping::ping))
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

- [ ] **Step 6: 运行测试，确认通过**

Run: `cargo test -p shirita-web`
Expected: 4 个测试全部通过。
（若先于实现运行 Step 1 的测试，应见编译失败 `unresolved import shirita_web::app` —— 实现完 Step 2–5 后转为通过。）

- [ ] **Step 7: 提交**

```bash
git add shirita-web/src shirita-web/tests
git commit -m "feat(m0): add web AppState, health/ping routes, bearer auth"
```

---

## Task 8: 入口 main.rs 与启动冒烟

**Files:**
- Modify: `shirita-web/src/main.rs`

- [ ] **Step 1: 实现 `shirita-web/src/main.rs`（替换全文）**

```rust
use std::sync::Arc;

use shirita_core::{Config, SqliteStorage, Storage};
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

    let storage: Arc<dyn Storage> = Arc::new(storage);
    let state = AppState {
        storage,
        config: Arc::new(config),
    };

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("shirita-web listening on {bind_addr}");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
```

- [ ] **Step 2: 编译整个 workspace**

Run: `cargo build`
Expected: 编译通过，无错误。

- [ ] **Step 3: 启动冒烟（手动验证服务可跑、迁移可执行、鉴权链生效）**

启动（后台）：
```bash
TOKEN_SECRET=devtoken DATABASE_PATH=/home/cc/.claude/jobs/b73d9870/tmp/shirita-smoke.db cargo run -p shirita-web
```
另开一个终端验证：
```bash
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8787/health
# 期望: 200

curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8787/api/ping
# 期望: 401

curl -s -H "Authorization: Bearer devtoken" http://127.0.0.1:8787/api/ping
# 期望: {"pong":true}
```
验证后停止服务（Ctrl-C 或 kill）。确认临时 DB 文件已创建（迁移成功）。

- [ ] **Step 4: 提交**

```bash
git add shirita-web/src/main.rs
git commit -m "feat(m0): wire web entrypoint (config -> migrate -> serve)"
```

---

## M0 完成判定（Definition of Done）

- [ ] `cargo build` 整个 workspace 通过。
- [ ] `cargo test` 全部通过（core 7 + web 4 = 11）。
- [ ] 服务可启动，`/health` 返回 200。
- [ ] 迁移自动执行，三张表（definitions / chat_sessions / messages）存在。
- [ ] Bearer 鉴权链生效：无/错 token → 401，正确 token → 200。
- [ ] definitions CRUD 在测试中往返成功。
- [ ] 所有改动已在 `main` 分支提交。

## 自检备注（Self-Review）

- **Spec 覆盖**：M0 spec 的 7 项要求逐一映射到 Task —— git/main(已 init)、workspace(T1)、env 配置(T3)、sqlx+WAL+池+迁移+三表(T4/T5)、Storage trait+SQLite 骨架(T6)、测试基座(T5 的 `temp_storage`)、Axum 骨架+Bearer 链(T7)。完成判定即 spec 的"完成标志"。
- **类型一致性**：`SqliteStorage::{connect,run_migrations,pool}`、`Storage` 五方法、`Definition{id,def_type,name,content,meta}`、`DefinitionType::{as_str,from_db}`、`Config::{new,from_env}`、`AppState{storage,config}`、`app(state)`、`require_bearer` 在各 Task 间签名一致。
- **无占位符**：所有代码步骤均给出完整代码与确切命令/期望输出。
