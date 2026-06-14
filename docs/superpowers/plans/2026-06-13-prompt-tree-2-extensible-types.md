# Prompt Tree v2 — Plan 2: Extensible definition types (`def_types`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hard-coded `DefinitionType` enum with a registry of container types (`def_types` table: 3 builtin + user-defined) plus a small set of reserved code constants, so users can add/remove their own definition types and containers.

**Architecture:** A new `def_types` table stores **container** types only (`char`/`persona`/`world` builtin, plus custom). `prompt`/`regex_rule`/`tool` stay as **reserved code constants** (never in the table). `Definition.def_type` becomes a plain `String`; validation moves from a compile-time enum to a runtime check against `{reserved} ∪ def_types.id`. A `deftype` helper module centralizes classification (`is_reserved`, `is_prompt`). New `/api/types` endpoints list/create/delete container types.

**Tech Stack:** Rust, sqlx/SQLite, Axum, existing `Storage` trait + `temp_storage()` test base. No new crates.

**Spec:** `docs/superpowers/specs/2026-06-13-prompt-tree-worldbook-design.md` §3 (type system), §12 (migrations/API).

**Out of scope (later plans / when needed):**
- Frontend type chips / "New type…" UI → Plan 3 (consumes `GET /api/types`).
- Session-node HTTP endpoints + lazy fork + override-`trigger` extension → deferred until in-chat structural editing UI exists (spec §2/§7 non-goals); Plan 1 already made sessions *reference* their template.
- ST import/export → Plan 6.

> **Migration numbering:** Plan 1 already shipped `0007_prompt_nodes_history.sql`. This plan adds `0008_def_types.sql` (next free).

---

## File structure

- `shirita-core/migrations/0008_def_types.sql` — **new**: `def_types` table + seed 3 builtin containers.
- `shirita-core/src/models/def_type.rs` — **new**: `DefType` row model + reserved-constant helpers (`is_reserved`, `is_prompt`, `RESERVED`).
- `shirita-core/src/models/definition.rs` — **modify**: `def_type: DefinitionType` → `def_type: String`; delete the `DefinitionType` enum; update `Definition::new` signature + tests.
- `shirita-core/src/models/mod.rs` — **modify**: `pub mod def_type;`.
- `shirita-core/src/storage/mod.rs` — **modify**: add `def_types` CRUD to the `Storage` trait.
- `shirita-core/src/storage/sqlite.rs` — **modify**: implement the new trait methods.
- `shirita-core/src/lib.rs` — **modify**: re-export `DefType` + `deftype` helpers; drop `DefinitionType` re-export.
- `shirita-core/src/conversation.rs` — **modify**: RegexRule filter + tests use the `"regex_rule"` string.
- `shirita-core/src/assembly.rs` — **modify**: tests use string types.
- `shirita-web/src/routes/definitions.rs` — **modify**: validate `type` against `{reserved} ∪ container ids`.
- `shirita-web/src/routes/types.rs` — **new**: `list` / `create` / `delete` container types.
- `shirita-web/src/lib.rs` — **modify**: mount `/types` routes.

---

## Task 1: `def_types` table + row model + reserved helpers

**Files:**
- Create: `shirita-core/migrations/0008_def_types.sql`
- Create: `shirita-core/src/models/def_type.rs`
- Modify: `shirita-core/src/models/mod.rs`

- [ ] **Step 1: Write the migration** `shirita-core/migrations/0008_def_types.sql`:

```sql
CREATE TABLE def_types (
    id         TEXT PRIMARY KEY,                 -- stable english id, e.g. char/persona/world
    label      TEXT NOT NULL,                    -- display name (i18n-able)
    sort       INTEGER NOT NULL DEFAULT 0,
    builtin    INTEGER NOT NULL DEFAULT 0,       -- 1 = cannot delete
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO def_types (id, label, sort, builtin) VALUES
    ('char',    'Character', 0, 1),
    ('persona', 'User',      1, 1),
    ('world',   'World',     2, 1);
```

- [ ] **Step 2: Write the failing test.** Create `shirita-core/src/models/def_type.rs`:

```rust
//! def_type：可扩展「容器类型」注册表行 + 保留类型常量。

use serde::{Deserialize, Serialize};

/// 保留类型（代码常量，永不入 def_types 表，不进节点树容器）。
pub const RESERVED: [&str; 3] = ["prompt", "regex_rule", "tool"];

/// 是否保留类型（prompt / regex_rule / tool）。
pub fn is_reserved(t: &str) -> bool {
    RESERVED.contains(&t)
}

/// 是否根级裸文本的 prompt 类型。
pub fn is_prompt(t: &str) -> bool {
    t == "prompt"
}

/// 容器类型注册表的一行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefType {
    pub id: String,
    pub label: String,
    pub sort: i64,
    pub builtin: bool,
    pub created_at: String,
}

impl DefType {
    /// 新建一个用户自定义容器类型（builtin = false）。
    pub fn new(id: impl Into<String>, label: impl Into<String>, sort: i64) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            sort,
            builtin: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_classification() {
        assert!(is_reserved("prompt"));
        assert!(is_reserved("regex_rule"));
        assert!(is_reserved("tool"));
        assert!(!is_reserved("char"));
        assert!(is_prompt("prompt"));
        assert!(!is_prompt("char"));
    }

    #[test]
    fn new_custom_is_not_builtin() {
        let t = DefType::new("faction", "Faction", 5);
        assert_eq!(t.id, "faction");
        assert!(!t.builtin);
        assert_eq!(t.created_at.len() > 0, true);
    }
}
```

- [ ] **Step 3: Register the module.** In `shirita-core/src/models/mod.rs` add `pub mod def_type;` next to the other `pub mod` lines.

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test -p shirita-core def_type::`
Expected: PASS (2 tests). (Migration isn't exercised yet — it runs in Task 2's storage test.)

- [ ] **Step 5: Commit**

```bash
git add shirita-core/migrations/0008_def_types.sql shirita-core/src/models/def_type.rs shirita-core/src/models/mod.rs
git commit -m "feat(core): def_types table + DefType row model + reserved helpers"
```

---

## Task 2: `def_types` storage CRUD

**Files:**
- Modify: `shirita-core/src/storage/mod.rs`, `shirita-core/src/storage/sqlite.rs`

- [ ] **Step 1: Write the failing test.** Add to the `tests` module in `shirita-core/src/storage/sqlite.rs`:

```rust
    #[tokio::test]
    async fn def_types_seed_and_crud() {
        let storage = temp_storage().await;
        // migration seeds 3 builtin containers
        let types = storage.list_container_types().await.unwrap();
        assert_eq!(types.len(), 3);
        assert!(types.iter().all(|t| t.builtin));
        assert_eq!(types[0].id, "char"); // ordered by sort

        // create a custom type
        let faction = crate::models::def_type::DefType::new("faction", "Faction", 9);
        storage.create_def_type(&faction).await.unwrap();
        let types = storage.list_container_types().await.unwrap();
        assert_eq!(types.len(), 4);
        assert!(types.iter().any(|t| t.id == "faction" && !t.builtin));

        // delete it
        storage.delete_def_type("faction").await.unwrap();
        assert_eq!(storage.list_container_types().await.unwrap().len(), 3);
    }
```

- [ ] **Step 2: Add the trait methods.** In `shirita-core/src/storage/mod.rs`, add to the `Storage` trait (after the `// --- settings ---` block) and import `DefType`:

```rust
use crate::models::def_type::DefType;
```
```rust
    // --- def types (container type registry) ---
    /// 列出容器类型（按 sort 升序）。
    async fn list_container_types(&self) -> Result<Vec<DefType>>;
    async fn create_def_type(&self, ty: &DefType) -> Result<()>;
    async fn delete_def_type(&self, id: &str) -> Result<()>;
```

- [ ] **Step 3: Implement in sqlite.** In `shirita-core/src/storage/sqlite.rs`, add inside `impl Storage for SqliteStorage` (mirror the existing `settings` methods' `sqlx::query` style):

```rust
    async fn list_container_types(&self) -> Result<Vec<DefType>> {
        let rows = sqlx::query_as::<_, (String, String, i64, i64, String)>(
            "SELECT id, label, sort, builtin, created_at FROM def_types ORDER BY sort ASC, id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, label, sort, builtin, created_at)| DefType {
                id,
                label,
                sort,
                builtin: builtin != 0,
                created_at,
            })
            .collect())
    }

    async fn create_def_type(&self, ty: &DefType) -> Result<()> {
        sqlx::query(
            "INSERT INTO def_types (id, label, sort, builtin, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&ty.id)
        .bind(&ty.label)
        .bind(ty.sort)
        .bind(ty.builtin as i64)
        .bind(&ty.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_def_type(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM def_types WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```

Add `use crate::models::def_type::DefType;` to the top of `sqlite.rs` if not already imported via the trait import.

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test -p shirita-core storage::sqlite::tests::def_types_seed_and_crud`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/storage/mod.rs shirita-core/src/storage/sqlite.rs
git commit -m "feat(core): def_types storage CRUD (list_container_types/create/delete)"
```

---

## Task 3: `Definition.def_type` → `String` (retire the enum)

This is the cross-cutting refactor flagged in the spec (§3, §17). Do it in one task so the tree never sits broken.

**Files:**
- Modify: `shirita-core/src/models/definition.rs`, `shirita-core/src/lib.rs`, `shirita-core/src/conversation.rs`, `shirita-core/src/assembly.rs`

- [ ] **Step 1: Change the model.** In `shirita-core/src/models/definition.rs`, **delete** the entire `DefinitionType` enum + its `impl` + the `type_db_roundtrip`/`unknown_type_errors` tests, and change the struct + constructor:

```rust
//! Definition 模型：type 为可扩展字符串（见 models::def_type）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    pub id: String,
    #[serde(rename = "type")]
    pub def_type: String,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub meta: serde_json::Value,
}

impl Definition {
    pub fn new(
        def_type: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            def_type: def_type.into(),
            name: name.into(),
            content: content.into(),
            meta: serde_json::json!({}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_definition_has_uuid_and_empty_meta() {
        let d = Definition::new("char", "Alice", "<char>...</char>");
        assert_eq!(d.def_type, "char");
        assert_eq!(d.name, "Alice");
        assert_eq!(d.meta, serde_json::json!({}));
        assert_eq!(d.id.len(), 36, "uuid v4 string is 36 chars");
    }
}
```

- [ ] **Step 2: Fix the lib re-export.** In `shirita-core/src/lib.rs`, change:

```rust
pub use models::definition::{Definition, DefinitionType};
```
to:
```rust
pub use models::def_type::{is_prompt, is_reserved, DefType};
pub use models::definition::Definition;
```

- [ ] **Step 3: Fix conversation.rs (non-test).** In `shirita-core/src/conversation.rs`, the regex-rule filter currently reads:

```rust
.filter(|d| d.def_type == crate::models::definition::DefinitionType::RegexRule)
```
Change to:
```rust
.filter(|d| d.def_type == "regex_rule")
```

- [ ] **Step 4: Fix conversation.rs tests.** Replace the two `Definition::new(crate::models::definition::DefinitionType::Char, …)` / `…::RegexRule, …` call sites with string literals:

```rust
        let ch = crate::models::definition::Definition::new("char", "C", "I am {{who}}");
```
```rust
        let mut rule = crate::models::definition::Definition::new("regex_rule", "R", "");
```

- [ ] **Step 5: Fix assembly.rs tests.** In `shirita-core/src/assembly.rs`, the tests import + helper:
  - Change `use crate::models::definition::{Definition, DefinitionType};` → `use crate::models::definition::Definition;`
  - Change the `def` helper signature `fn def(t: DefinitionType, …)` → `fn def(t: &str, …)` and call `Definition::new(t, name, content)`.
  - Update every `def(DefinitionType::Char, …)` → `def("char", …)`, `DefinitionType::Prompt` → `"prompt"`, `DefinitionType::World` → `"world"`, `DefinitionType::RegexRule` → `"regex_rule"`. (Search the test module for `DefinitionType::`.)

- [ ] **Step 6: Run the full core suite, verify pass**

Run: `cargo test -p shirita-core`
Expected: PASS. (If the compiler flags a remaining `DefinitionType` reference, fix it — `grep -rn DefinitionType shirita-core/src` must return nothing.)

- [ ] **Step 7: Commit**

```bash
git add shirita-core/src/models/definition.rs shirita-core/src/lib.rs shirita-core/src/conversation.rs shirita-core/src/assembly.rs
git commit -m "refactor(core): Definition.def_type is a String; retire DefinitionType enum"
```

---

## Task 4: Validate definition `type` against the registry (web)

**Files:**
- Modify: `shirita-web/src/routes/definitions.rs`

- [ ] **Step 1: Write the failing test.** In the web definitions integration test (`shirita-web/tests/definitions_test.rs` — follow the existing `bad_type_is_rejected` test there), add:

```rust
#[tokio::test]
async fn custom_container_type_is_accepted_after_registration() {
    let state = test_state().await; // existing helper in this file
    // register a custom container type
    let (st, _) = post_json(&state, "/api/types", r#"{"id":"faction","label":"Faction"}"#).await;
    assert_eq!(st, StatusCode::OK);
    // a definition of that type is now valid
    let (st, _) =
        post_json(&state, "/api/definitions", r#"{"type":"faction","name":"Zion","content":"x"}"#)
            .await;
    assert_eq!(st, StatusCode::OK);
    // an unregistered type is still rejected
    let (st, _) =
        post_json(&state, "/api/definitions", r#"{"type":"bogus","name":"X","content":"x"}"#).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}
```

> If `definitions_test.rs` has no `post_json`/`test_state` helper, copy the harness shape from `shirita-web/tests/template_assembly_test.rs` (the `send`/`test_state` helpers). Task 5 adds the `/api/types` POST route this test calls — if running Task 4 before Task 5, this test will fail on the `/api/types` call; that's expected, it passes once Task 5 lands. Prefer implementing Task 5 first, then this test goes green.

- [ ] **Step 2: Replace the enum validation.** In `shirita-web/src/routes/definitions.rs`:
  - Remove `use shirita_core::models::definition::{Definition, DefinitionType};` → `use shirita_core::models::definition::Definition;`
  - In `build()` (or wherever `DefinitionType::from_db(&body.r#type)` was), replace the compile-time check with a runtime registry check. Change `build` to be `async` and take `&AppState`, or validate in the handler before calling `build`. Concretely, in the `create` handler:

```rust
pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<DefinitionBody>,
) -> Result<Json<Definition>, StatusCode> {
    validate_type(&state, &body.r#type).await?;
    let def = Definition {
        id: uuid::Uuid::new_v4().to_string(),
        def_type: body.r#type.clone(),
        name: body.name,
        content: body.content,
        meta: if body.meta.is_null() { serde_json::json!({}) } else { body.meta },
    };
    state.storage.create_definition(&def).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(def))
}

/// type 必须是保留类型，或已注册的容器类型。
async fn validate_type(state: &AppState, t: &str) -> Result<(), StatusCode> {
    if shirita_core::is_reserved(t) {
        return Ok(());
    }
    let containers = state.storage.list_container_types().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if containers.iter().any(|c| c.id == t) {
        Ok(())
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}
```
  - Apply the same `validate_type(&state, &body.r#type).await?;` guard at the start of the `update` handler.

- [ ] **Step 3: Run tests, verify pass** (after Task 5 lands the route)

Run: `cargo test -p shirita-web definitions`
Expected: PASS, incl. existing `bad_type_is_rejected` (now "bogus" fails the registry check).

- [ ] **Step 4: Commit**

```bash
git add shirita-web/src/routes/definitions.rs shirita-web/tests/definitions_test.rs
git commit -m "feat(web): validate definition type against reserved + registered containers"
```

---

## Task 5: `/api/types` endpoints

**Files:**
- Create: `shirita-web/src/routes/types.rs`
- Modify: `shirita-web/src/lib.rs`, `shirita-web/src/routes/mod.rs`

- [ ] **Step 1: Write the failing test.** Add to a new `shirita-web/tests/types_test.rs` (harness shaped like `template_assembly_test.rs`):

```rust
#[tokio::test]
async fn types_crud_and_builtin_protected() {
    let state = test_state().await;
    // list seeds 3 builtin
    let (st, body) = send(&state, "GET", "/api/types", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&body).as_array().unwrap().len(), 3);

    // create custom
    let (st, _) = send(&state, "POST", "/api/types", Some(r#"{"id":"faction","label":"Faction"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&send(&state, "GET", "/api/types", None).await.1).as_array().unwrap().len(), 4);

    // delete custom OK
    let (st, _) = send(&state, "DELETE", "/api/types/faction", None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    // delete builtin rejected
    let (st, _) = send(&state, "DELETE", "/api/types/char", None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Implement the route module.** Create `shirita-web/src/routes/types.rs`:

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use shirita_core::models::def_type::DefType;

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateTypeBody {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub sort: i64,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<DefType>>, StatusCode> {
    state
        .storage
        .list_container_types()
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateTypeBody>,
) -> Result<Json<DefType>, StatusCode> {
    // id 不得与保留类型冲突，也不得空。
    if body.id.trim().is_empty() || shirita_core::is_reserved(&body.id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let ty = DefType::new(body.id, body.label, body.sort);
    state
        .storage
        .create_def_type(&ty)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ty))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    // 内置类型不可删。
    let containers = state
        .storage
        .list_container_types()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match containers.iter().find(|c| c.id == id) {
        Some(c) if c.builtin => Err(StatusCode::BAD_REQUEST),
        Some(_) => {
            state
                .storage
                .delete_def_type(&id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}
```

- [ ] **Step 3: Register the module + routes.** In `shirita-web/src/routes/mod.rs` add `pub mod types;`. In `shirita-web/src/lib.rs`, inside the `protected` router, add:

```rust
        .route("/types", get(routes::types::list).post(routes::types::create))
        .route("/types/{id}", delete(routes::types::delete))
```
(Ensure `delete` is in the `use axum::routing::{...}` import list — it already imports `get`/`post`/`put`; add `delete` if missing.)

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test -p shirita-web types && cargo test -p shirita-web definitions`
Expected: PASS (types CRUD + the Task 4 validation test).

- [ ] **Step 5: Commit**

```bash
git add shirita-web/src/routes/types.rs shirita-web/src/routes/mod.rs shirita-web/src/lib.rs shirita-web/tests/types_test.rs
git commit -m "feat(web): /api/types endpoints (list/create/delete; builtin protected)"
```

---

## Task 6: Full-workspace verification

**Files:** none (verification only)

- [ ] **Step 1: Run the whole suite.**

Run: `cargo test`
Expected: PASS across core + web.

- [ ] **Step 2: Grep for stragglers.**

Run: `grep -rn "DefinitionType" shirita-core/src shirita-web/src`
Expected: **no matches** (the enum is fully retired).

- [ ] **Step 3: Clippy the touched crates** (pre-existing debt in `routes/assets.rs` + `routes/provider.rs` is out of scope — confirm no *new* findings in the files this plan touched).

Run: `cargo clippy -p shirita-core`
Expected: clean.

- [ ] **Step 4: Commit** (only if any verification fix was needed; otherwise skip).

---

## Self-review checklist

- **Spec coverage (§3):** `def_types` table + 3 builtin containers (T1) ✓ · prompt/regex_rule/tool reserved constants (T1) ✓ · `def_type` String + retire enum (T3, §17) ✓ · validation set `{reserved} ∪ def_types.id` (T4) ✓ · `GET/POST/DELETE /api/types`, builtin undeletable (T5, §12) ✓ · `list_container_types()` storage (T2, §12) ✓. **Deferred (noted):** frontend type UI (Plan 3), session-node endpoints/lazy-fork/override-trigger (until in-chat editing), ST export (Plan 6). `item` type intentionally **not** seeded (spec §3 — no existing data depends on it).
- **Placeholder scan:** no TBD/"handle errors" — every code step is concrete.
- **Type consistency:** `DefType{id,label,sort,builtin,created_at}`, `DefType::new(id,label,sort)`, `Storage::list_container_types/create_def_type/delete_def_type`, `is_reserved`/`is_prompt`, `Definition.def_type: String`, `validate_type(state, t)` — names identical across tasks.
- **Ordering note:** Task 4's test depends on Task 5's `/api/types` POST. Executor should land Task 5 before re-running Task 4's test (called out in T4 Step 1).
