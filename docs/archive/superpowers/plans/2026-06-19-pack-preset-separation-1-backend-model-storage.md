# Pack/Preset Separation — Plan 1: Backend Model & Storage

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the persistence foundation for Packs — the new `Pack` entity, the `content` mount node kind, the `pack` node-owner kind, session→pack mounting, and a `<<content>>` node in every template.

**Architecture:** Reuse the existing `prompt_nodes` tree wholesale: a Pack's content is just nodes with `owner_kind='pack'`; the mount point is a new `kind='content'` node (sibling concept to the existing `history` node). Packs get their own table mirroring `templates`. Session→pack mounting mirrors the existing `mounted_definitions` JSON column. No assembly/API/frontend changes here — those are Plans 2–5.

**Tech Stack:** Rust, `sqlx` runtime query API (no `query!` macro, no `DATABASE_URL`), SQLite, `async_trait`.

## Global Constraints

- Code comments and git commit messages in **English** (project rule).
- `sqlx` **runtime** query API only (`sqlx::query(...)`, `.bind(...)`); migrations are plain `.sql` files auto-discovered by `sqlx::migrate!("./migrations")`.
- Migrations are immutable & ordered by filename; next free numbers are **0017, 0018, 0019** (current max is `0016_assets_kind.sql`).
- SQLite cannot alter a `CHECK` in place — relax one by **rebuilding the table** (copy → drop → rename), per the existing `0007_prompt_nodes_history.sql`.
- Every task ends green: `cargo test --workspace` passes with **zero warnings**.
- This plan touches **only `shirita-core`**. Do not edit `shirita-web`, `shirita-ui`, or `shirita-tauri`.

---

### Task 1: `NodeKind::Content` + `OwnerKind::Pack` + prompt_nodes migration

**Files:**
- Modify: `shirita-core/src/models/prompt_node.rs`
- Create: `shirita-core/migrations/0017_prompt_nodes_pack_content.sql`
- Test: `shirita-core/src/models/prompt_node.rs` (unit) + `shirita-core/src/storage/sqlite.rs` (storage roundtrip)

**Interfaces:**
- Produces: `NodeKind::Content` (`as_str()=="content"`, `from_db("content")`), `OwnerKind::Pack` (`as_str()=="pack"`, `from_db("pack")`). Later tasks/plans rely on these to own pack trees and mark mount points.

- [ ] **Step 1: Write the failing unit test** — append inside the existing `#[cfg(test)] mod tests` in `shirita-core/src/models/prompt_node.rs`:

```rust
    #[test]
    fn content_kind_roundtrip() {
        assert_eq!(NodeKind::Content.as_str(), "content");
        assert_eq!(NodeKind::from_db("content").unwrap(), NodeKind::Content);
    }

    #[test]
    fn owner_kind_pack_roundtrip() {
        assert_eq!(OwnerKind::Pack.as_str(), "pack");
        assert_eq!(OwnerKind::from_db("pack").unwrap(), OwnerKind::Pack);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-core models::prompt_node`
Expected: FAIL — `no variant named Content` / `no variant named Pack`.

- [ ] **Step 3: Add the enum variants** in `shirita-core/src/models/prompt_node.rs`.

In `enum NodeKind` add `Content`; in `NodeKind::as_str` add `NodeKind::Content => "content",`; in `NodeKind::from_db` add `"content" => NodeKind::Content,`.

In `enum OwnerKind` add `Pack`; in `OwnerKind::as_str` add `OwnerKind::Pack => "pack",`; in `OwnerKind::from_db` add `"pack" => OwnerKind::Pack,`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p shirita-core models::prompt_node`
Expected: PASS.

- [ ] **Step 5: Write the failing storage roundtrip test** — append inside `#[cfg(test)] mod tests` in `shirita-core/src/storage/sqlite.rs` (the module already has a `temp_storage()` helper that connects + runs migrations):

```rust
    #[tokio::test]
    async fn pack_owner_and_content_node_roundtrip() {
        let s = temp_storage().await;
        // a content node owned by a template
        let t = crate::models::template::Template::new("T");
        s.create_template(&t).await.unwrap();
        let mut content = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "content");
        content.kind = NodeKind::Content;
        content.tag = None;
        s.create_node(&content).await.unwrap();
        // a ref node owned by a pack (owner_kind='pack')
        let def = Definition::new("char", "Alice", "hi");
        s.create_definition(&def).await.unwrap();
        let pref = PromptNode::new_ref(OwnerKind::Pack, "pack-1", None, 0, &def.id);
        s.create_node(&pref).await.unwrap();

        let got_content = s.get_node(&content.id).await.unwrap().unwrap();
        assert_eq!(got_content.kind, NodeKind::Content);
        let pack_nodes = s.list_nodes(&OwnerKind::Pack, "pack-1").await.unwrap();
        assert_eq!(pack_nodes.len(), 1);
        assert_eq!(pack_nodes[0].owner_kind, OwnerKind::Pack);
        assert_eq!(pack_nodes[0].definition_id.as_deref(), Some(def.id.as_str()));
    }
```

- [ ] **Step 6: Run to verify it fails**

Run: `cargo test -p shirita-core storage::sqlite::tests::pack_owner_and_content_node_roundtrip`
Expected: FAIL — `create_node` errors with a `CHECK constraint failed` (old constraints reject `kind='content'` / `owner_kind='pack'`).

- [ ] **Step 7: Create the migration** `shirita-core/migrations/0017_prompt_nodes_pack_content.sql`:

```sql
-- Relax prompt_nodes CHECKs: allow owner_kind='pack' and kind='content'.
-- SQLite can't alter a CHECK in place, so rebuild (mirrors 0007). Preserves the
-- meta column added in 0015.
PRAGMA foreign_keys=OFF;

CREATE TABLE prompt_nodes_new (
    id            TEXT PRIMARY KEY,
    owner_kind    TEXT NOT NULL CHECK(owner_kind IN ('template', 'session', 'pack')),
    owner_id      TEXT NOT NULL,
    parent_id     TEXT REFERENCES prompt_nodes_new(id) ON DELETE CASCADE,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    kind          TEXT NOT NULL CHECK(kind IN ('folder', 'ref', 'history', 'content')),
    tag           TEXT,
    definition_id TEXT REFERENCES definitions(id) ON DELETE SET NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    meta          TEXT NOT NULL DEFAULT '{}'
);

INSERT INTO prompt_nodes_new
    SELECT id, owner_kind, owner_id, parent_id, sort_order, kind, tag, definition_id, enabled, created_at, meta
    FROM prompt_nodes;

DROP TABLE prompt_nodes;
ALTER TABLE prompt_nodes_new RENAME TO prompt_nodes;

CREATE INDEX IF NOT EXISTS idx_prompt_nodes_owner ON prompt_nodes(owner_kind, owner_id);
CREATE INDEX IF NOT EXISTS idx_prompt_nodes_parent ON prompt_nodes(parent_id);

PRAGMA foreign_keys=ON;
```

- [ ] **Step 8: Run to verify it passes**

Run: `cargo test -p shirita-core storage::sqlite::tests::pack_owner_and_content_node_roundtrip`
Expected: PASS.

- [ ] **Step 9: Full suite + commit**

Run: `cargo test --workspace`
Expected: PASS, zero warnings.

```bash
git add shirita-core/src/models/prompt_node.rs shirita-core/src/storage/sqlite.rs shirita-core/migrations/0017_prompt_nodes_pack_content.sql
git commit -m "feat(core): add content node kind + pack node-owner kind"
```

---

### Task 2: `Pack` model + packs table + Pack CRUD

**Files:**
- Create: `shirita-core/src/models/pack.rs`
- Modify: `shirita-core/src/models/mod.rs`
- Create: `shirita-core/migrations/0018_packs.sql`
- Modify: `shirita-core/src/storage/mod.rs` (trait), `shirita-core/src/storage/sqlite.rs` (impl)
- Test: `shirita-core/src/models/pack.rs` (unit) + `shirita-core/src/storage/sqlite.rs` (roundtrip)

**Interfaces:**
- Consumes: `OwnerKind::Pack` (Task 1).
- Produces: `Pack { id, name, identity: PackIdentity, meta: serde_json::Value, created_at, updated_at }`, `PackIdentity { display_name: Option<String>, avatar: Option<String> }`, `Pack::new(name)`. Storage: `create_pack`, `get_pack`, `list_packs`, `update_pack`, `delete_pack`.

- [ ] **Step 1: Write the failing model test** — create `shirita-core/src/models/pack.rs`:

```rust
//! Pack model: a content bundle — a node tree (owner=pack) plus an optional
//! identity and (via bound regex/var/html definitions) scoped behaviors.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PackIdentity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub identity: PackIdentity,
    #[serde(default)]
    pub meta: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

impl Pack {
    pub fn new(name: impl Into<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            identity: PackIdentity::default(),
            meta: serde_json::json!({}),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pack_has_uuid_empty_identity_and_meta() {
        let p = Pack::new("Alice");
        assert_eq!(p.name, "Alice");
        assert_eq!(p.id.len(), 36);
        assert_eq!(p.identity, PackIdentity::default());
        assert_eq!(p.meta, serde_json::json!({}));
        assert_eq!(p.created_at, p.updated_at);
    }
}
```

- [ ] **Step 2: Register the module** — add to `shirita-core/src/models/mod.rs`, alphabetically between `message` and `prompt_node`:

```rust
pub mod pack;
```

- [ ] **Step 3: Run to verify it passes**

Run: `cargo test -p shirita-core models::pack`
Expected: PASS.

- [ ] **Step 4: Create the migration** `shirita-core/migrations/0018_packs.sql`:

```sql
CREATE TABLE IF NOT EXISTS packs (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    identity_json TEXT NOT NULL DEFAULT '{}',
    meta          TEXT NOT NULL DEFAULT '{}',
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
```

- [ ] **Step 5: Add the trait methods** — in `shirita-core/src/storage/mod.rs`, add the import and a new section after the `// --- templates ---` block:

```rust
use crate::models::pack::Pack;
```
```rust
    // --- packs ---
    async fn create_pack(&self, pack: &Pack) -> Result<()>;
    async fn get_pack(&self, id: &str) -> Result<Option<Pack>>;
    async fn list_packs(&self) -> Result<Vec<Pack>>;
    async fn update_pack(&self, pack: &Pack) -> Result<()>;
    /// Delete a pack and its node tree (`owner_kind='pack'`).
    async fn delete_pack(&self, id: &str) -> Result<()>;
```

- [ ] **Step 6: Write the failing storage test** — append inside `#[cfg(test)] mod tests` in `shirita-core/src/storage/sqlite.rs`:

```rust
    #[tokio::test]
    async fn pack_crud_and_delete_cascades_nodes() {
        let s = temp_storage().await;
        let mut p = crate::models::pack::Pack::new("Alice");
        p.identity.display_name = Some("Alice".into());
        p.identity.avatar = Some("a.png".into());
        s.create_pack(&p).await.unwrap();

        let got = s.get_pack(&p.id).await.unwrap().unwrap();
        assert_eq!(got.name, "Alice");
        assert_eq!(got.identity.display_name.as_deref(), Some("Alice"));
        assert_eq!(s.list_packs().await.unwrap().len(), 1);

        let def = Definition::new("char", "Alice", "hi");
        s.create_definition(&def).await.unwrap();
        let node = PromptNode::new_ref(OwnerKind::Pack, &p.id, None, 0, &def.id);
        s.create_node(&node).await.unwrap();

        s.delete_pack(&p.id).await.unwrap();
        assert!(s.get_pack(&p.id).await.unwrap().is_none());
        assert!(s.list_nodes(&OwnerKind::Pack, &p.id).await.unwrap().is_empty());
    }
```

- [ ] **Step 7: Run to verify it fails**

Run: `cargo test -p shirita-core storage::sqlite::tests::pack_crud_and_delete_cascades_nodes`
Expected: FAIL — `create_pack` not found (trait not implemented for `SqliteStorage`).

- [ ] **Step 8: Implement the impl** — in `shirita-core/src/storage/sqlite.rs`:

Add the import to the top `use` block:
```rust
use crate::models::pack::Pack;
```
Add the row mapper next to `row_to_template`:
```rust
fn row_to_pack(row: &SqliteRow) -> Result<Pack> {
    let identity_str: String = row.try_get("identity_json")?;
    let meta_str: String = row.try_get("meta")?;
    Ok(Pack {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        identity: serde_json::from_str(&identity_str)?,
        meta: serde_json::from_str(&meta_str)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
```
Add the methods inside `impl Storage for SqliteStorage` (after the templates block):
```rust
    // --- packs ---
    async fn create_pack(&self, pack: &Pack) -> Result<()> {
        let identity = serde_json::to_string(&pack.identity)?;
        let meta = serde_json::to_string(&pack.meta)?;
        sqlx::query("INSERT INTO packs (id, name, identity_json, meta, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&pack.id).bind(&pack.name).bind(identity).bind(meta)
            .bind(&pack.created_at).bind(&pack.updated_at)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_pack(&self, id: &str) -> Result<Option<Pack>> {
        let row = sqlx::query("SELECT id, name, identity_json, meta, created_at, updated_at FROM packs WHERE id = ?")
            .bind(id).fetch_optional(&self.pool).await?;
        match row { Some(r) => Ok(Some(row_to_pack(&r)?)), None => Ok(None) }
    }

    async fn list_packs(&self) -> Result<Vec<Pack>> {
        let rows = sqlx::query("SELECT id, name, identity_json, meta, created_at, updated_at FROM packs ORDER BY name")
            .fetch_all(&self.pool).await?;
        rows.iter().map(row_to_pack).collect()
    }

    async fn update_pack(&self, pack: &Pack) -> Result<()> {
        let identity = serde_json::to_string(&pack.identity)?;
        let meta = serde_json::to_string(&pack.meta)?;
        sqlx::query("UPDATE packs SET name = ?, identity_json = ?, meta = ?, updated_at = ? WHERE id = ?")
            .bind(&pack.name).bind(identity).bind(meta).bind(&pack.updated_at).bind(&pack.id)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn delete_pack(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM prompt_nodes WHERE owner_kind = 'pack' AND owner_id = ?").bind(id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM packs WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }
```

- [ ] **Step 9: Run to verify it passes**

Run: `cargo test -p shirita-core storage::sqlite::tests::pack_crud_and_delete_cascades_nodes`
Expected: PASS.

- [ ] **Step 10: Full suite + commit**

Run: `cargo test --workspace`
Expected: PASS, zero warnings.

```bash
git add shirita-core/src/models/pack.rs shirita-core/src/models/mod.rs shirita-core/migrations/0018_packs.sql shirita-core/src/storage/mod.rs shirita-core/src/storage/sqlite.rs
git commit -m "feat(core): Pack model + packs table + Pack CRUD storage"
```

---

### Task 3: Session `mounted_packs`

**Files:**
- Create: `shirita-core/migrations/0019_session_mounted_packs.sql`
- Modify: `shirita-core/src/models/session.rs`, `shirita-core/src/storage/mod.rs`, `shirita-core/src/storage/sqlite.rs`
- Test: `shirita-core/src/storage/sqlite.rs` (roundtrip)

**Interfaces:**
- Produces: `Session.mounted_packs: Vec<String>` (ordered pack ids); Storage `set_mounted_packs(session_id, &[String])`. Plan 2 (assembly) and Plan 3 (API) consume these.

- [ ] **Step 1: Create the migration** `shirita-core/migrations/0019_session_mounted_packs.sql`:

```sql
-- Ordered list of mounted pack ids, mirroring mounted_definitions.
ALTER TABLE chat_sessions ADD COLUMN mounted_packs TEXT NOT NULL DEFAULT '[]';
```

- [ ] **Step 2: Extend the Session model** in `shirita-core/src/models/session.rs`.

Add the field right after `mounted_definitions`:
```rust
    #[serde(default)]
    pub mounted_packs: Vec<String>,
```
Add to `Session::new()` right after `mounted_definitions: Vec::new(),`:
```rust
            mounted_packs: Vec::new(),
```

- [ ] **Step 3: Add the trait method** in `shirita-core/src/storage/mod.rs`, in the `// --- sessions ---` block next to `set_mounted_definitions`:

```rust
    /// Replace the session's ordered mounted-pack id list wholesale.
    async fn set_mounted_packs(&self, session_id: &str, ids: &[String]) -> Result<()>;
```

- [ ] **Step 4: Write the failing storage test** — append inside `#[cfg(test)] mod tests` in `shirita-core/src/storage/sqlite.rs`:

```rust
    #[tokio::test]
    async fn session_mounted_packs_roundtrip() {
        let s = temp_storage().await;
        let sess = Session::new("Chat");
        s.create_session(&sess).await.unwrap();
        assert!(s.get_session(&sess.id).await.unwrap().unwrap().mounted_packs.is_empty());

        s.set_mounted_packs(&sess.id, &["p1".into(), "p2".into()]).await.unwrap();
        let got = s.get_session(&sess.id).await.unwrap().unwrap();
        assert_eq!(got.mounted_packs, vec!["p1".to_string(), "p2".to_string()]);
    }
```

- [ ] **Step 5: Run to verify it fails**

Run: `cargo test -p shirita-core storage::sqlite::tests::session_mounted_packs_roundtrip`
Expected: FAIL — `no field mounted_packs` / `set_mounted_packs` not found / `no column named mounted_packs`.

- [ ] **Step 6: Wire the column through sqlite.rs** in `shirita-core/src/storage/sqlite.rs`:

In `row_to_session`, after the `mounted` line add:
```rust
    let mounted_packs: String = row.try_get("mounted_packs")?;
```
and in the returned `Session { … }` after `mounted_definitions: …,` add:
```rust
        mounted_packs: serde_json::from_str(&mounted_packs)?,
```
In `create_session`, add `mounted_packs` to the column list and a matching `?`, bind it after `mounted`:
```rust
        let mounted_packs = serde_json::to_string(&session.mounted_packs)?;
```
(INSERT becomes `… mounted_definitions, mounted_packs, created_at …` with one more `?`, and `.bind(mounted)` is followed by `.bind(mounted_packs)`.)

In `get_session` and `list_sessions`, add `mounted_packs` to each `SELECT` column list (right after `mounted_definitions`).

Add the impl method inside `impl Storage for SqliteStorage`, next to `set_mounted_definitions`:
```rust
    async fn set_mounted_packs(&self, session_id: &str, ids: &[String]) -> Result<()> {
        let json = serde_json::to_string(ids)?;
        sqlx::query("UPDATE chat_sessions SET mounted_packs = ? WHERE id = ?")
            .bind(json)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```

- [ ] **Step 7: Run to verify it passes**

Run: `cargo test -p shirita-core storage::sqlite::tests::session_mounted_packs_roundtrip`
Expected: PASS.

- [ ] **Step 8: Full suite + commit**

Run: `cargo test --workspace`
Expected: PASS, zero warnings. (Check that any other test constructing `Session { … }` by struct literal still compiles — the field has `#[serde(default)]` but struct literals need it; `Session::new` covers the common path. If a literal exists, add `mounted_packs: Vec::new(),`.)

```bash
git add shirita-core/migrations/0019_session_mounted_packs.sql shirita-core/src/models/session.rs shirita-core/src/storage/mod.rs shirita-core/src/storage/sqlite.rs
git commit -m "feat(core): session mounted_packs ordered mount list"
```

---

### Task 4: Seed a `<<content>>` node in templates

**Files:**
- Modify: `shirita-core/src/seed.rs`
- Test: `shirita-core/src/seed.rs`

**Interfaces:**
- Consumes: `NodeKind::Content` (Task 1).
- Produces: `ensure_default_template` now also seeds a `content` node (sorted before `history`); new `ensure_templates_have_content_node(storage)` backfills existing templates. Plan 3 wires the backfill into app startup.

- [ ] **Step 1: Write the failing tests** — in `shirita-core/src/seed.rs`, update the existing `seeds_a_default_template_with_history` test to also assert a content node, and add a backfill test. Inside `#[cfg(test)] mod tests`:

```rust
    #[tokio::test]
    async fn default_template_has_content_before_history() {
        let storage = mem_storage().await;
        ensure_default_template(&storage).await.unwrap();
        let t = &storage.list_templates().await.unwrap()[0];
        let nodes = storage.list_nodes(&OwnerKind::Template, &t.id).await.unwrap();
        let content = nodes.iter().find(|n| n.kind == NodeKind::Content).expect("content node");
        let history = nodes.iter().find(|n| n.kind == NodeKind::History).expect("history node");
        assert!(content.sort_order < history.sort_order, "content sorts before history");
    }

    #[tokio::test]
    async fn backfill_adds_one_content_node_idempotently() {
        let storage = mem_storage().await;
        // a template with only a history node (legacy shape)
        let t = Template::new("Legacy");
        storage.create_template(&t).await.unwrap();
        let mut hist = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "history");
        hist.kind = NodeKind::History;
        hist.tag = None;
        storage.create_node(&hist).await.unwrap();

        ensure_templates_have_content_node(&storage).await.unwrap();
        ensure_templates_have_content_node(&storage).await.unwrap(); // idempotent

        let nodes = storage.list_nodes(&OwnerKind::Template, &t.id).await.unwrap();
        let contents: Vec<_> = nodes.iter().filter(|n| n.kind == NodeKind::Content).collect();
        assert_eq!(contents.len(), 1, "exactly one content node");
        let history = nodes.iter().find(|n| n.kind == NodeKind::History).unwrap();
        assert!(contents[0].sort_order < history.sort_order, "content backfilled before history");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-core seed`
Expected: FAIL — `ensure_templates_have_content_node` not found + no content node in default template.

- [ ] **Step 3: Update `ensure_default_template`** in `shirita-core/src/seed.rs` — replace the body that creates only the history node so it seeds content (sort 0) then history (sort 1):

```rust
    let t = Template::new("Default");
    storage.create_template(&t).await?;
    let mut content = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "content");
    content.kind = NodeKind::Content;
    content.tag = None;
    storage.create_node(&content).await?;
    let mut hist = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 1, "history");
    hist.kind = NodeKind::History;
    hist.tag = None;
    storage.create_node(&hist).await?;
    Ok(())
```

- [ ] **Step 4: Add the backfill function** in `shirita-core/src/seed.rs` (after `ensure_default_template`):

```rust
/// Backfill: every template must own exactly one `content` mount node. For each
/// template lacking one, insert it and reorder so it sits just before the
/// history node (or last if there is none). Idempotent. Plan 3 calls this at
/// startup alongside `ensure_default_template`.
pub async fn ensure_templates_have_content_node<S: Storage + ?Sized>(storage: &S) -> Result<()> {
    for t in storage.list_templates().await? {
        let nodes = storage.list_nodes(&OwnerKind::Template, &t.id).await?;
        if nodes.iter().any(|n| n.kind == NodeKind::Content) {
            continue;
        }
        let mut content = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "content");
        content.kind = NodeKind::Content;
        content.tag = None;
        storage.create_node(&content).await?;
        // reorder root nodes so content lands right before history (else last).
        let mut root: Vec<&PromptNode> =
            nodes.iter().filter(|n| n.parent_id.is_none()).collect();
        root.sort_by_key(|n| n.sort_order);
        let mut ordered: Vec<String> = Vec::new();
        for n in &root {
            if n.kind == NodeKind::History {
                ordered.push(content.id.clone());
            }
            ordered.push(n.id.clone());
        }
        if !ordered.contains(&content.id) {
            ordered.push(content.id.clone());
        }
        storage
            .reorder_nodes(&OwnerKind::Template, &t.id, &ordered)
            .await?;
    }
    Ok(())
}
```

Ensure the file's imports cover `NodeKind` (the existing `use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};` already does).

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p shirita-core seed`
Expected: PASS.

- [ ] **Step 6: Full suite + commit**

Run: `cargo test --workspace`
Expected: PASS, zero warnings.

```bash
git add shirita-core/src/seed.rs
git commit -m "feat(core): seed content mount node in templates + backfill helper"
```

---

## Self-Review

**Spec coverage (Plan 1 slice of §16.1):** packs table ✓ (T2), `session_packs`/`mounted_packs` ✓ (T3, JSON column mirroring `mounted_definitions` instead of a join table — deliberate consistency with existing code, noted), `prompt_nodes` content/pack migration ✓ (T1), `OwnerKind::Pack`/`NodeKind::Content` ✓ (T1), Storage CRUD ✓ (T2), default-template content node + backfill ✓ (T4). Assembly/API/identity/frontend/import are out of this plan (Plans 2–5).

**Placeholder scan:** none — every step has exact code/commands.

**Type consistency:** `Pack`/`PackIdentity`/`Pack::new` (T2) match storage usage; `OwnerKind::Pack`/`NodeKind::Content` (T1) used identically in T2/T4; `set_mounted_packs` signature matches trait (T3). Migration numbers 0017→0018→0019 are unique and ordered.

**Deferred to later plans (intentional):** startup wiring of `ensure_templates_have_content_node` (Plan 3, where `main.rs`/web state init lives); assembly reading `mounted_packs` + `content` node (Plan 2); pack/session-pack REST endpoints (Plan 3).
