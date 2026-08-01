# M3 Templates, Prompt Nodes, and New-Chat Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add backend tables for templates and prompt node trees, upgrade session creation to deep-copy template trees, upgrade assembly to walk a node tree instead of a flat mounted list, and build the two-step new-chat flow (avatar+name → prompt tree) with the shared PromptTree component.

**Architecture:** A single `prompt_nodes` table with `owner_kind ∈ {template, session}` holds both template master trees and per-session copies. Session creation deep-copies the template's nodes. Assembly traverses the tree (folder nodes → XML tags; ref nodes → definition content with `{{var}}` rendering). The frontend PromptTree component renders the node tree with indentation, enable toggles, and an inline `+` picker to add definitions.

**Tech Stack:** Rust (Axum, sqlx, sqlite), Vue 3, TypeScript, Tailwind CSS v4, Pinia, lucide-vue-next, Vitest + @vue/test-utils (jsdom) + tokio-test (backend).

---

## Plan series (M3 → 5 plans)

This is **Plan 3 of 5**. Prerequisites: Plans 1–2 complete.

| Plan | Slice | Spec sections |
|------|-------|---------------|
| 1 ✓ | Scaffold + design tokens + AppShell + router + api client + Home chat list | §2, §3, §4.1, §4.2, §8 (partial) |
| 2 ✓ | Chat detail: message list (bubble/flat), composer, SSE streaming, message actions | §4.5, §10 |
| **3 (this)** | Backend templates/`prompt_nodes`/assembly upgrade + 2-step new-chat + PromptTree | §4.3, §4.4, §5, §6, §7 |
| 4 | Book editor + definitions CRUD/search + overrides | §4.6, §6.2, §7 |
| 5 | Settings: provider list, generation, custom CSS, regex, test-connection | §4.7, §6.1, §7, §9 |

---

## File Structure (created / modified in this plan)

```
Backend (Rust):
shirita-core/
├── migrations/
│   └── 0004_templates_nodes.sql          (create)
├── src/
│   ├── models/
│   │   ├── template.rs                    (create)
│   │   └── prompt_node.rs                 (create)
│   │   └── session.rs                     (modify: add template_id)
│   ├── storage/
│   │   ├── mod.rs                         (modify: extend trait)
│   │   └── sqlite.rs                      (modify: implement new methods)
│   ├── assembly.rs                        (modify: tree-based assembly)
│   └── lib.rs                             (modify: re-export new types)
│
shirita-web/
├── src/
│   ├── routes/
│   │   ├── templates.rs                   (create)
│   │   ├── prompt_nodes.rs                (create)
│   │   └── sessions.rs                    (modify: accept template_id, deep-copy)
│   └── lib.rs                             (modify: add new routes)

Frontend (Vue):
shirita-ui/src/
├── api/
│   ├── types.ts                           (modify: add Template, PromptNode)
│   └── client.ts                          (modify: add template/node endpoints)
├── stores/
│   └── library.ts                         (create: templates + definitions cache)
├── components/
│   ├── AvatarPicker.vue                   (create: inline avatar library picker)
│   ├── PromptTree.vue                     (create: node tree with indentation)
│   ├── PromptTree.test.ts                 (create)
│   ├── NodeRow.vue                        (create: single tree row)
│   └── NodePicker.vue                     (create: inline + definition picker)
└── views/
    ├── NewChatView.vue                    (modify: replace stub, step 1)
    └── NewChatPromptView.vue              (create: step 2, prompt tree + create)
```

New routes: `/new/prompt` (step 2).

---

## Task 1: Migration for templates + prompt_nodes tables (TDD)

**Files:**
- Create: `shirita-core/migrations/0004_templates_nodes.sql`
- Modify: `shirita-core/src/storage/sqlite.rs` (test for new tables)

- [ ] **Step 1: Write the failing test in `shirita-core/src/storage/sqlite.rs`**

Add this test inside the `#[cfg(test)] mod tests` block:

```rust
#[tokio::test]
async fn migration_0004_creates_templates_and_prompt_nodes_tables() {
    let storage = temp_storage().await;
    for table in ["templates", "prompt_nodes"] {
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
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shirita-core migration_0004_creates_templates_and_prompt_nodes_tables`
Expected: FAIL — assertion `row.0 == 1` fails (tables don't exist yet).

- [ ] **Step 3: Create `shirita-core/migrations/0004_templates_nodes.sql`**

```sql
CREATE TABLE IF NOT EXISTS templates (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    meta       TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS prompt_nodes (
    id            TEXT PRIMARY KEY,
    owner_kind    TEXT NOT NULL CHECK(owner_kind IN ('template', 'session')),
    owner_id      TEXT NOT NULL,
    parent_id     TEXT REFERENCES prompt_nodes(id) ON DELETE CASCADE,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    kind          TEXT NOT NULL CHECK(kind IN ('folder', 'ref')),
    tag           TEXT,
    definition_id TEXT REFERENCES definitions(id) ON DELETE SET NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_prompt_nodes_owner ON prompt_nodes(owner_kind, owner_id);
CREATE INDEX IF NOT EXISTS idx_prompt_nodes_parent ON prompt_nodes(parent_id);
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shirita-core migration_0004_creates_templates_and_prompt_nodes_tables`
Expected: PASS.

- [ ] **Step 5: Add `template_id` to chat_sessions**

Create `shirita-core/migrations/0005_session_template_id.sql`:

```sql
ALTER TABLE chat_sessions ADD COLUMN template_id TEXT REFERENCES templates(id) ON DELETE SET NULL;
```

Note: SQLite `ALTER TABLE ADD COLUMN` is safe (no data migration needed; existing rows get NULL).

- [ ] **Step 6: Verify the column exists**

Add to the same test in Step 1:

```rust
// Verify template_id column exists on chat_sessions
let row: (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM pragma_table_info('chat_sessions') WHERE name='template_id'",
)
.fetch_one(storage.pool())
.await
.unwrap();
assert_eq!(row.0, 1, "chat_sessions.template_id column should exist");
```

Run: `cargo test -p shirita-core migration_0004`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add shirita-core/migrations/0004_templates_nodes.sql shirita-core/migrations/0005_session_template_id.sql shirita-core/src/storage/sqlite.rs
git commit -m "feat(m3): templates + prompt_nodes tables migration"
```

---

## Task 2: Template and PromptNode Rust models (TDD)

**Files:**
- Create: `shirita-core/src/models/template.rs`
- Create: `shirita-core/src/models/prompt_node.rs`
- Modify: `shirita-core/src/models/session.rs` (add `template_id` field)
- Modify: `shirita-core/src/models/mod.rs` (declare new modules)
- Modify: `shirita-core/src/lib.rs` (re-export)

- [ ] **Step 1: Write the failing test — create `shirita-core/src/models/template.rs` with inline test**

```rust
//! Template 模型。
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub meta: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

impl Template {
    pub fn new(name: impl Into<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
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
    fn new_template_has_uuid_and_timestamps() {
        let t = Template::new("My Template");
        assert_eq!(t.name, "My Template");
        assert_eq!(t.id.len(), 36);
        assert_eq!(t.meta, serde_json::json!({}));
        assert!(!t.created_at.is_empty());
        assert_eq!(t.created_at, t.updated_at);
    }
}
```

- [ ] **Step 2: Create `shirita-core/src/models/prompt_node.rs` with inline test**

```rust
//! PromptNode：模板/会话节点树的一员。
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Folder,
    Ref,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Folder => "folder",
            NodeKind::Ref => "ref",
        }
    }

    pub fn from_db(s: &str) -> crate::Result<Self> {
        Ok(match s {
            "folder" => NodeKind::Folder,
            "ref" => NodeKind::Ref,
            other => return Err(crate::Error::InvalidDefinitionType(other.to_string())),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerKind {
    Template,
    Session,
}

impl OwnerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            OwnerKind::Template => "template",
            OwnerKind::Session => "session",
        }
    }

    pub fn from_db(s: &str) -> crate::Result<Self> {
        Ok(match s {
            "template" => OwnerKind::Template,
            "session" => OwnerKind::Session,
            other => return Err(crate::Error::InvalidDefinitionType(other.to_string())),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptNode {
    pub id: String,
    pub owner_kind: OwnerKind,
    pub owner_id: String,
    pub parent_id: Option<String>,
    pub sort_order: i64,
    pub kind: NodeKind,
    pub tag: Option<String>,
    pub definition_id: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

impl PromptNode {
    pub fn new_folder(
        owner_kind: OwnerKind,
        owner_id: impl Into<String>,
        parent_id: Option<String>,
        sort_order: i64,
        tag: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            owner_kind,
            owner_id: owner_id.into(),
            parent_id,
            sort_order,
            kind: NodeKind::Folder,
            tag: Some(tag.into()),
            definition_id: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn new_ref(
        owner_kind: OwnerKind,
        owner_id: impl Into<String>,
        parent_id: Option<String>,
        sort_order: i64,
        definition_id: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            owner_kind,
            owner_id: owner_id.into(),
            parent_id,
            sort_order,
            kind: NodeKind::Ref,
            tag: None,
            definition_id: Some(definition_id.into()),
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_folder_node() {
        let n = PromptNode::new_folder(
            OwnerKind::Template, "t1", None, 0, "char",
        );
        assert_eq!(n.kind, NodeKind::Folder);
        assert_eq!(n.tag.as_deref(), Some("char"));
        assert!(n.definition_id.is_none());
        assert!(n.enabled);
        assert_eq!(n.id.len(), 36);
    }

    #[test]
    fn new_ref_node() {
        let n = PromptNode::new_ref(
            OwnerKind::Session, "s1", Some("parent".into()), 1, "def-1",
        );
        assert_eq!(n.kind, NodeKind::Ref);
        assert_eq!(n.definition_id.as_deref(), Some("def-1"));
        assert!(n.tag.is_none());
    }

    #[test]
    fn node_kind_roundtrip() {
        assert_eq!(NodeKind::Folder.as_str(), "folder");
        assert_eq!(NodeKind::Ref.as_str(), "ref");
        assert_eq!(NodeKind::from_db("folder").unwrap(), NodeKind::Folder);
        assert_eq!(NodeKind::from_db("ref").unwrap(), NodeKind::Ref);
        assert!(NodeKind::from_db("nope").is_err());
    }

    #[test]
    fn owner_kind_roundtrip() {
        assert_eq!(OwnerKind::Template.as_str(), "template");
        assert_eq!(OwnerKind::Session.as_str(), "session");
    }
}
```

- [ ] **Step 3: Modify `shirita-core/src/models/session.rs` to add `template_id`**

Replace the `Session` struct with:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    #[serde(default)]
    pub template_id: Option<String>,
    #[serde(default)]
    pub override_config: serde_json::Value,
    #[serde(default)]
    pub current_state: serde_json::Value,
    #[serde(default)]
    pub mounted_definitions: Vec<String>,
}
```

And update `Session::new`:

```rust
impl Session {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            avatar: None,
            template_id: None,
            override_config: serde_json::json!({}),
            current_state: serde_json::json!({}),
            mounted_definitions: Vec::new(),
        }
    }
}
```

- [ ] **Step 4: Create `shirita-core/src/models/mod.rs`**

```rust
pub mod definition;
pub mod message;
pub mod prompt_node;
pub mod session;
pub mod template;
```

- [ ] **Step 5: Update `shirita-core/src/lib.rs` re-exports**

Add to the existing re-exports:

```rust
pub use models::prompt_node::{NodeKind, OwnerKind, PromptNode};
pub use models::template::Template;
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p shirita-core`
Expected: all existing + new model tests pass.

- [ ] **Step 7: Commit**

```bash
git add shirita-core/src/models/
git commit -m "feat(m3): Template + PromptNode models, template_id on Session"
```

---

## Task 3: Extend Storage trait + SqliteStorage for templates and nodes (TDD)

**Files:**
- Modify: `shirita-core/src/storage/mod.rs` (extend trait)
- Modify: `shirita-core/src/storage/sqlite.rs` (implement)

- [ ] **Step 1: Extend the Storage trait in `shirita-core/src/storage/mod.rs`**

Add these methods to the `Storage` trait (before the closing `}`):

```rust
    // --- templates ---
    async fn create_template(&self, template: &Template) -> Result<()>;
    async fn get_template(&self, id: &str) -> Result<Option<Template>>;
    async fn list_templates(&self) -> Result<Vec<Template>>;
    async fn update_template(&self, template: &Template) -> Result<()>;
    async fn delete_template(&self, id: &str) -> Result<()>;

    // --- prompt nodes ---
    async fn list_nodes(&self, owner_kind: &OwnerKind, owner_id: &str) -> Result<Vec<PromptNode>>;
    async fn create_node(&self, node: &PromptNode) -> Result<()>;
    async fn update_node(&self, node: &PromptNode) -> Result<()>;
    async fn delete_node(&self, id: &str) -> Result<()>;
    async fn reorder_nodes(&self, owner_kind: &OwnerKind, owner_id: &str, ordered_ids: &[String]) -> Result<()>;
    /// Deep-copy all nodes from one owner to another (e.g. template → session).
    /// Returns a mapping old_id → new_id so callers can fix up parent references.
    async fn copy_nodes(
        &self,
        from_kind: &OwnerKind,
        from_id: &str,
        to_kind: &OwnerKind,
        to_id: &str,
    ) -> Result<std::collections::HashMap<String, String>>;
```

Note: `copy_nodes` returns a `HashMap` — add `use std::collections::HashMap;` to the top of `mod.rs`.

Make sure the trait bounds match: `use crate::models::prompt_node::{OwnerKind, PromptNode};` and `use crate::models::template::Template;` are in scope.

- [ ] **Step 2: Implement in `shirita-core/src/storage/sqlite.rs` — row mappers**

Add:

```rust
fn row_to_template(row: &SqliteRow) -> Result<Template> {
    let meta_str: String = row.try_get("meta")?;
    Ok(Template {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        meta: serde_json::from_str(&meta_str)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_prompt_node(row: &SqliteRow) -> Result<PromptNode> {
    let owner_kind_str: String = row.try_get("owner_kind")?;
    let kind_str: String = row.try_get("kind")?;
    let enabled: i64 = row.try_get("enabled")?;
    Ok(PromptNode {
        id: row.try_get("id")?,
        owner_kind: OwnerKind::from_db(&owner_kind_str)?,
        owner_id: row.try_get("owner_id")?,
        parent_id: row.try_get("parent_id")?,
        sort_order: row.try_get("sort_order")?,
        kind: NodeKind::from_db(&kind_str)?,
        tag: row.try_get("tag")?,
        definition_id: row.try_get("definition_id")?,
        enabled: enabled != 0,
        created_at: row.try_get("created_at")?,
    })
}
```

- [ ] **Step 3: Implement template CRUD in SqliteStorage**

Add inside `impl Storage for SqliteStorage`:

```rust
    async fn create_template(&self, template: &Template) -> Result<()> {
        let meta = serde_json::to_string(&template.meta)?;
        sqlx::query(
            "INSERT INTO templates (id, name, meta, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&template.id)
        .bind(&template.name)
        .bind(meta)
        .bind(&template.created_at)
        .bind(&template.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_template(&self, id: &str) -> Result<Option<Template>> {
        let row = sqlx::query("SELECT id, name, meta, created_at, updated_at FROM templates WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(r) => Ok(Some(row_to_template(&r)?)),
            None => Ok(None),
        }
    }

    async fn list_templates(&self) -> Result<Vec<Template>> {
        let rows = sqlx::query("SELECT id, name, meta, created_at, updated_at FROM templates ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(row_to_template).collect()
    }

    async fn update_template(&self, template: &Template) -> Result<()> {
        let meta = serde_json::to_string(&template.meta)?;
        sqlx::query(
            "UPDATE templates SET name = ?, meta = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&template.name)
        .bind(meta)
        .bind(&template.updated_at)
        .bind(&template.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_template(&self, id: &str) -> Result<()> {
        // prompt_nodes with owner_kind='template' and owner_id=id cascade via FK? No — manual cleanup.
        sqlx::query("DELETE FROM prompt_nodes WHERE owner_kind = 'template' AND owner_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM templates WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```

- [ ] **Step 4: Implement prompt node CRUD in SqliteStorage**

Add inside `impl Storage for SqliteStorage`:

```rust
    async fn list_nodes(&self, owner_kind: &OwnerKind, owner_id: &str) -> Result<Vec<PromptNode>> {
        let rows = sqlx::query(
            "SELECT id, owner_kind, owner_id, parent_id, sort_order, kind, tag, definition_id, enabled, created_at \
             FROM prompt_nodes WHERE owner_kind = ? AND owner_id = ? ORDER BY sort_order ASC, id ASC",
        )
        .bind(owner_kind.as_str())
        .bind(owner_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_prompt_node).collect()
    }

    async fn create_node(&self, node: &PromptNode) -> Result<()> {
        sqlx::query(
            "INSERT INTO prompt_nodes (id, owner_kind, owner_id, parent_id, sort_order, kind, tag, definition_id, enabled, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&node.id)
        .bind(node.owner_kind.as_str())
        .bind(&node.owner_id)
        .bind(&node.parent_id)
        .bind(node.sort_order)
        .bind(node.kind.as_str())
        .bind(&node.tag)
        .bind(&node.definition_id)
        .bind(node.enabled as i64)
        .bind(&node.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_node(&self, node: &PromptNode) -> Result<()> {
        sqlx::query(
            "UPDATE prompt_nodes SET parent_id = ?, sort_order = ?, kind = ?, tag = ?, definition_id = ?, enabled = ? WHERE id = ?",
        )
        .bind(&node.parent_id)
        .bind(node.sort_order)
        .bind(node.kind.as_str())
        .bind(&node.tag)
        .bind(&node.definition_id)
        .bind(node.enabled as i64)
        .bind(&node.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_node(&self, id: &str) -> Result<()> {
        // Delete children first (cascade in Rust for clarity; FK ON DELETE CASCADE on parent_id is also set)
        sqlx::query("DELETE FROM prompt_nodes WHERE parent_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM prompt_nodes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn reorder_nodes(&self, owner_kind: &OwnerKind, owner_id: &str, ordered_ids: &[String]) -> Result<()> {
        for (i, nid) in ordered_ids.iter().enumerate() {
            sqlx::query("UPDATE prompt_nodes SET sort_order = ? WHERE id = ? AND owner_kind = ? AND owner_id = ?")
                .bind(i as i64)
                .bind(nid)
                .bind(owner_kind.as_str())
                .bind(owner_id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn copy_nodes(
        &self,
        from_kind: &OwnerKind,
        from_id: &str,
        to_kind: &OwnerKind,
        to_id: &str,
    ) -> Result<std::collections::HashMap<String, String>> {
        let source = self.list_nodes(from_kind, from_id).await?;
        let mut id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        // First pass: create all nodes with new UUIDs, remapping parent_id references.
        // Sort by parent_id NULLs first so we never reference an unmapped parent.
        let mut sorted = source.clone();
        sorted.sort_by_key(|n| (n.parent_id.is_some(), n.sort_order));

        for node in &sorted {
            let new_id = uuid::Uuid::new_v4().to_string();
            let new_parent_id = node.parent_id.as_ref().and_then(|pid| id_map.get(pid).cloned());

            let copy = PromptNode {
                id: new_id.clone(),
                owner_kind: to_kind.clone(),
                owner_id: to_id.to_string(),
                parent_id: new_parent_id,
                sort_order: node.sort_order,
                kind: node.kind.clone(),
                tag: node.tag.clone(),
                definition_id: node.definition_id.clone(),
                enabled: node.enabled,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            self.create_node(&copy).await?;
            id_map.insert(node.id.clone(), new_id);
        }

        Ok(id_map)
    }
```

- [ ] **Step 5: Write the storage test for template/node CRUD**

Add to the test module in `sqlite.rs`:

```rust
    use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
    use crate::models::template::Template;
    use std::collections::HashMap;

    #[tokio::test]
    async fn template_crud_roundtrip() {
        let storage = temp_storage().await;

        let t = Template::new("My Template");
        storage.create_template(&t).await.unwrap();

        let got = storage.get_template(&t.id).await.unwrap().unwrap();
        assert_eq!(got.name, "My Template");

        let all = storage.list_templates().await.unwrap();
        assert_eq!(all.len(), 1);

        let mut updated = t.clone();
        updated.name = "Renamed".into();
        storage.update_template(&updated).await.unwrap();
        assert_eq!(storage.get_template(&t.id).await.unwrap().unwrap().name, "Renamed");

        storage.delete_template(&t.id).await.unwrap();
        assert!(storage.get_template(&t.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn prompt_nodes_crud_and_tree() {
        let storage = temp_storage().await;

        // root folder
        let root = PromptNode::new_folder(OwnerKind::Template, "t1", None, 0, "root");
        storage.create_node(&root).await.unwrap();

        // child ref under root
        let child = PromptNode::new_ref(OwnerKind::Template, "t1", Some(root.id.clone()), 1, "def-1");
        storage.create_node(&child).await.unwrap();

        let nodes = storage.list_nodes(&OwnerKind::Template, "t1").await.unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].id, root.id);
        assert_eq!(nodes[1].parent_id.as_deref(), Some(root.id.as_str()));

        // Toggle enabled
        let mut updated = child.clone();
        updated.enabled = false;
        storage.update_node(&updated).await.unwrap();
        let reloaded = storage.list_nodes(&OwnerKind::Template, "t1").await.unwrap();
        assert!(!reloaded[1].enabled);

        // Reorder
        storage.reorder_nodes(&OwnerKind::Template, "t1", &[child.id.clone(), root.id.clone()]).await.unwrap();
        let reordered = storage.list_nodes(&OwnerKind::Template, "t1").await.unwrap();
        assert_eq!(reordered[0].id, child.id);

        // Delete node (and child)
        storage.delete_node(&root.id).await.unwrap();
        let after = storage.list_nodes(&OwnerKind::Template, "t1").await.unwrap();
        assert!(after.is_empty());
    }

    #[tokio::test]
    async fn copy_nodes_deep_clones_tree() {
        let storage = temp_storage().await;

        // Build template tree: root folder → child ref
        let root = PromptNode::new_folder(OwnerKind::Template, "t1", None, 0, "char");
        storage.create_node(&root).await.unwrap();
        let child = PromptNode::new_ref(OwnerKind::Template, "t1", Some(root.id.clone()), 0, "def-x");
        storage.create_node(&child).await.unwrap();

        // Deep-copy to session
        let id_map = storage.copy_nodes(&OwnerKind::Template, "t1", &OwnerKind::Session, "s1").await.unwrap();
        assert_eq!(id_map.len(), 2);

        let copied = storage.list_nodes(&OwnerKind::Session, "s1").await.unwrap();
        assert_eq!(copied.len(), 2);

        // Verify parent_id remapping
        let copied_root = copied.iter().find(|n| n.kind == NodeKind::Folder).unwrap();
        let copied_child = copied.iter().find(|n| n.kind == NodeKind::Ref).unwrap();
        assert_eq!(copied_child.parent_id.as_deref(), Some(copied_root.id.as_str()));
        assert_eq!(copied_child.owner_kind, OwnerKind::Session);
        assert_eq!(copied_child.owner_id, "s1");
        assert_eq!(copied_child.definition_id.as_deref(), Some("def-x"));

        // Original template nodes unchanged
        let original = storage.list_nodes(&OwnerKind::Template, "t1").await.unwrap();
        assert_eq!(original.len(), 2);
    }
```

- [ ] **Step 6: Update `shirita-core/src/storage/sqlite.rs` to serialize `template_id`**

Modify `row_to_session`:

```rust
fn row_to_session(row: &SqliteRow) -> Result<Session> {
    let override_config: String = row.try_get("override_config")?;
    let current_state: String = row.try_get("current_state")?;
    let mounted: String = row.try_get("mounted_definitions")?;
    Ok(Session {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        avatar: row.try_get("avatar")?,
        template_id: row.try_get("template_id")?,
        override_config: serde_json::from_str(&override_config)?,
        current_state: serde_json::from_str(&current_state)?,
        mounted_definitions: serde_json::from_str(&mounted)?,
    })
}
```

Modify `create_session` query:

```rust
    async fn create_session(&self, session: &Session) -> Result<()> {
        let override_config = serde_json::to_string(&session.override_config)?;
        let current_state = serde_json::to_string(&session.current_state)?;
        let mounted = serde_json::to_string(&session.mounted_definitions)?;
        sqlx::query(
            "INSERT INTO chat_sessions (id, name, avatar, template_id, override_config, current_state, mounted_definitions) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&session.id)
        .bind(&session.name)
        .bind(&session.avatar)
        .bind(&session.template_id)
        .bind(override_config)
        .bind(current_state)
        .bind(mounted)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
```

- [ ] **Step 7: Run all storage tests**

Run: `cargo test -p shirita-core`
Expected: all tests pass (including 3 new storage tests).

- [ ] **Step 8: Commit**

```bash
git add shirita-core/src/storage/
git commit -m "feat(m3): template + prompt_node storage CRUD + deep-copy"
```

---

## Task 4: Upgrade assembly to tree-based traversal (TDD)

**Files:**
- Modify: `shirita-core/src/assembly.rs`

- [ ] **Step 1: Rewrite `assemble_system_prompt` in `shirita-core/src/assembly.rs`**

Replace the current `assemble_system_prompt` function and its `wrap_tag` helper. The new version takes a node tree instead of a flat mounted list.

Remove `wrap_tag` and replace `assemble_system_prompt` with:

```rust
use crate::models::prompt_node::{NodeKind, PromptNode};

/// Walk a PromptNode tree, building the assembled prompt string.
///
/// * `nodes`       – all nodes for this owner (session/template), pre-sorted by sort_order.
/// * `definitions` – map of definition_id → Definition for ref node lookup.
/// * `overrides`   – local_overrides for definition content.
/// * `state`       – `{{var}}` render context.
///
/// Folder nodes become `<tag>…children…</tag>`.
/// Ref nodes render the referenced definition's effective content after `{{var}}` substitution.
/// Disabled nodes are skipped (including their children).
/// RegexRule / Tool definitions referenced by ref nodes are skipped (no system prompt output).
fn walk_tree(
    nodes: &[PromptNode],
    parent_id: Option<&str>,
    definitions: &std::collections::HashMap<String, &Definition>,
    overrides: &serde_json::Value,
    state: &serde_json::Value,
    output: &mut Vec<String>,
) {
    for node in nodes.iter().filter(|n| n.parent_id.as_deref() == parent_id) {
        if !node.enabled {
            continue;
        }
        match node.kind {
            NodeKind::Folder => {
                let tag = node.tag.as_deref().unwrap_or("unnamed");
                let mut children: Vec<String> = Vec::new();
                walk_tree(nodes, Some(&node.id), definitions, overrides, state, &mut children);
                if !children.is_empty() {
                    let inner = children.join("\n");
                    output.push(format!("<{tag}>\n{inner}\n</{tag}>"));
                }
            }
            NodeKind::Ref => {
                if let Some(def_id) = &node.definition_id {
                    if let Some(def) = definitions.get(def_id.as_str()) {
                        // Skip regex_rule and tool refs in system prompt
                        if def.def_type == DefinitionType::RegexRule || def.def_type == DefinitionType::Tool {
                            continue;
                        }
                        let content = effective_content(def, overrides);
                        let rendered = render_vars(&content, state);
                        output.push(rendered);
                    }
                }
            }
        }
    }
}

pub fn assemble_system_prompt(
    nodes: &[PromptNode],
    definitions: &[Definition],
    local_overrides: &serde_json::Value,
    state: &serde_json::Value,
) -> String {
    let def_map: std::collections::HashMap<String, &Definition> = definitions
        .iter()
        .map(|d| (d.id.clone(), d))
        .collect();
    let mut blocks: Vec<String> = Vec::new();
    // Sort nodes by sort_order then id for deterministic output
    let mut sorted = nodes.to_vec();
    sorted.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then_with(|| a.id.cmp(&b.id)));
    walk_tree(&sorted, None, &def_map, local_overrides, state, &mut blocks);
    blocks.join("\n")
}
```

- [ ] **Step 2: Update the existing assembly tests**

Replace the existing test `assemble_groups_in_order_with_tags` with tree-based equivalents:

```rust
    use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};

    #[test]
    fn assemble_walks_tree_with_tags() {
        // Build a tree: char folder → prompt ref
        let root = PromptNode::new_folder(OwnerKind::Template, "t1", None, 0, "characters");
        let ref_node = PromptNode::new_ref(OwnerKind::Template, "t1", Some(root.id.clone()), 0, "d1");
        let nodes = vec![root, ref_node];
        let defs = vec![Definition::new(DefinitionType::Char, "alice", "I am {{name}}")];
        // Override the first definition's id to match
        let mut defs_fixed = defs.clone();
        defs_fixed[0].id = "d1".into();

        let out = assemble_system_prompt(&nodes, &defs_fixed, &json!({}), &json!({ "name": "Alice" }));
        assert!(out.contains("<characters>"));
        assert!(out.contains("I am Alice"));
        assert!(out.contains("</characters>"));
    }

    #[test]
    fn assemble_skips_disabled_nodes() {
        let root = PromptNode::new_folder(OwnerKind::Template, "t1", None, 0, "characters");
        let mut ref_node = PromptNode::new_ref(OwnerKind::Template, "t1", Some(root.id.clone()), 0, "d1");
        ref_node.enabled = false;
        let nodes = vec![root, ref_node];
        let mut defs = vec![Definition::new(DefinitionType::Char, "alice", "I am {{name}}")];
        defs[0].id = "d1".into();

        let out = assemble_system_prompt(&nodes, &defs, &json!({}), &json!({ "name": "Alice" }));
        assert!(!out.contains("I am Alice"));
    }

    #[test]
    fn assemble_skips_regex_and_tool_refs() {
        let mut r = Definition::new(DefinitionType::RegexRule, "r", "");
        r.id = "rx1".into();
        let ref_node = PromptNode::new_ref(OwnerKind::Template, "t1", None, 0, "rx1");
        let nodes = vec![ref_node];
        let defs = vec![r];

        let out = assemble_system_prompt(&nodes, &defs, &json!({}), &json!({}));
        assert!(!out.contains("rx1"));
        assert!(out.is_empty());
    }

    #[test]
    fn local_override_replaces_content_in_tree() {
        let mut d = Definition::new(DefinitionType::Char, "c", "global");
        d.id = "d1".into();
        let ref_node = PromptNode::new_ref(OwnerKind::Template, "t1", None, 0, "d1");
        let nodes = vec![ref_node];
        let overrides = json!({ "d1": "overridden" });

        let out = assemble_system_prompt(&nodes, &[d], &overrides, &json!({}));
        assert!(out.contains("overridden"));
        assert!(!out.contains("global"));
    }
```

- [ ] **Step 3: Run assembly tests**

Run: `cargo test -p shirita-core assembly`
Expected: all assembly tests pass (includes existing `render_vars` and `regex_rules` tests + 4 new/updated tree-based tests).

- [ ] **Step 4: Commit**

```bash
git add shirita-core/src/assembly.rs
git commit -m "feat(m3): tree-based assembly — walk prompt_nodes instead of flat mounted list"
```

---

## Task 5: Template and Node API endpoints (TDD)

**Files:**
- Create: `shirita-web/src/routes/templates.rs`
- Create: `shirita-web/src/routes/prompt_nodes.rs`
- Modify: `shirita-web/src/routes/sessions.rs` (update create_session to accept template_id + deep-copy)
- Modify: `shirita-web/src/lib.rs` (add routes)

- [ ] **Step 1: Create `shirita-web/src/routes/templates.rs`**

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use shirita_core::Template;

use crate::AppState;

#[derive(Deserialize)]
pub struct TemplateBody {
    pub name: String,
    #[serde(default)]
    pub meta: Value,
}

fn build_template(id: String, body: TemplateBody) -> Template {
    let now = chrono::Utc::now().to_rfc3339();
    let meta = if body.meta.is_null() { serde_json::json!({}) } else { body.meta };
    Template { id, name: body.name, meta, created_at: now.clone(), updated_at: now }
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Template>>, StatusCode> {
    state.storage.list_templates().await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<TemplateBody>,
) -> Result<Json<Template>, StatusCode> {
    let t = build_template(uuid::Uuid::new_v4().to_string(), body);
    state.storage.create_template(&t).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(t))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Template>, StatusCode> {
    state.storage.get_template(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json).ok_or(StatusCode::NOT_FOUND)
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<TemplateBody>,
) -> Result<Json<Template>, StatusCode> {
    let existing = state.storage.get_template(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let meta = if body.meta.is_null() { serde_json::json!({}) } else { body.meta };
    let updated = Template {
        id,
        name: body.name,
        meta,
        created_at: existing.created_at,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    state.storage.update_template(&updated).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(updated))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    state.storage.delete_template(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn duplicate(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Template>, StatusCode> {
    let original = state.storage.get_template(&id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let copy = Template {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!("{} (copy)", original.name),
        meta: original.meta.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    state.storage.create_template(&copy).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Deep-copy nodes
    use shirita_core::OwnerKind;
    state.storage.copy_nodes(&OwnerKind::Template, &id, &OwnerKind::Template, &copy.id)
        .await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(copy))
}
```

- [ ] **Step 2: Create `shirita-web/src/routes/prompt_nodes.rs`**

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use shirita_core::{NodeKind, OwnerKind, PromptNode};

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateNodeBody {
    pub parent_id: Option<String>,
    pub kind: String,
    pub tag: Option<String>,
    pub definition_id: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateNodeBody {
    pub parent_id: Option<String>,
    pub sort_order: Option<i64>,
    pub tag: Option<String>,
    pub definition_id: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Deserialize)]
pub struct ReorderBody {
    pub ordered_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct NodesQuery {
    pub owner_kind: String,
}

pub async fn list_nodes(
    State(state): State<AppState>,
    Path(owner_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<NodesQuery>,
) -> Result<Json<Vec<PromptNode>>, StatusCode> {
    let kind = OwnerKind::from_db(&q.owner_kind).map_err(|_| StatusCode::BAD_REQUEST)?;
    state.storage.list_nodes(&kind, &owner_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn create_node(
    State(state): State<AppState>,
    Path(owner_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<NodesQuery>,
    Json(body): Json<CreateNodeBody>,
) -> Result<Json<PromptNode>, StatusCode> {
    let owner_kind = OwnerKind::from_db(&q.owner_kind).map_err(|_| StatusCode::BAD_REQUEST)?;
    let kind = NodeKind::from_db(&body.kind).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Compute next sort_order for this parent
    let siblings = state.storage.list_nodes(&owner_kind, &owner_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let next_order = siblings.iter()
        .filter(|n| n.parent_id == body.parent_id)
        .count() as i64;

    let node = match kind {
        NodeKind::Folder => PromptNode::new_folder(
            owner_kind, &owner_id, body.parent_id, next_order,
            body.tag.unwrap_or_else(|| "unnamed".into()),
        ),
        NodeKind::Ref => PromptNode::new_ref(
            owner_kind, &owner_id, body.parent_id, next_order,
            body.definition_id.ok_or(StatusCode::BAD_REQUEST)?,
        ),
    };

    state.storage.create_node(&node).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(node))
}

pub async fn update_node(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    Json(body): Json<UpdateNodeBody>,
) -> Result<Json<PromptNode>, StatusCode> {
    // Load existing, apply partial update
    // We need to find the node first. Add a get_node method or list+find.
    // For simplicity, add a helper on SqliteStorage:
    // Actually, let's add get_node to Storage trait.
    // For now, we simulate by listing all nodes for the owner — the plan adds get_node next.
    // REPLACED: The Storage trait now includes get_node.
    let existing = state.storage.get_node(&node_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let updated = PromptNode {
        parent_id: body.parent_id.or(existing.parent_id),
        sort_order: body.sort_order.unwrap_or(existing.sort_order),
        tag: body.tag.or(existing.tag),
        definition_id: body.definition_id.or(existing.definition_id),
        enabled: body.enabled.unwrap_or(existing.enabled),
        ..existing
    };

    state.storage.update_node(&updated).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(updated))
}

pub async fn delete_node(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    state.storage.delete_node(&node_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reorder_nodes(
    State(state): State<AppState>,
    Path(owner_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<NodesQuery>,
    Json(body): Json<ReorderBody>,
) -> Result<StatusCode, StatusCode> {
    let kind = OwnerKind::from_db(&q.owner_kind).map_err(|_| StatusCode::BAD_REQUEST)?;
    state.storage.reorder_nodes(&kind, &owner_id, &body.ordered_ids)
        .await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}
```

Note: The `update_node` handler needs `get_node` on the Storage trait. Add this minimal method to the Storage trait in `mod.rs`:

```rust
    async fn get_node(&self, id: &str) -> Result<Option<PromptNode>>;
```

And implement in `sqlite.rs`:

```rust
    async fn get_node(&self, id: &str) -> Result<Option<PromptNode>> {
        let row = sqlx::query(
            "SELECT id, owner_kind, owner_id, parent_id, sort_order, kind, tag, definition_id, enabled, created_at \
             FROM prompt_nodes WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some(row_to_prompt_node(&r)?)),
            None => Ok(None),
        }
    }
```

- [ ] **Step 3: Update `shirita-web/src/routes/sessions.rs` — modify `create_session`**

Replace `create_session` to accept optional `template_id` and deep-copy nodes:

```rust
#[derive(Deserialize)]
pub struct CreateSession {
    pub name: String,
    pub template_id: Option<String>,
}

pub async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSession>,
) -> Result<Json<Session>, StatusCode> {
    let mut session = Session::new(body.name);
    session.template_id = body.template_id.clone();

    state
        .storage
        .create_session(&session)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // If a template was specified, deep-copy its node tree to the session
    if let Some(tid) = body.template_id {
        if state.storage.get_template(&tid).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.is_some() {
            state.storage.copy_nodes(
                &shirita_core::OwnerKind::Template, &tid,
                &shirita_core::OwnerKind::Session, &session.id,
            ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    Ok(Json(session))
}
```

- [ ] **Step 4: Update `shirita-web/src/lib.rs` to add routes**

Add the new routes inside the `protected` router block:

```rust
        .route(
            "/templates",
            get(routes::templates::list).post(routes::templates::create),
        )
        .route(
            "/templates/{id}",
            get(routes::templates::get)
                .put(routes::templates::update)
                .delete(routes::templates::delete),
        )
        .route("/templates/{id}/duplicate", post(routes::templates::duplicate))
        .route(
            "/templates/{id}/nodes",
            get(routes::prompt_nodes::list_nodes).post(routes::prompt_nodes::create_node),
        )
        .route(
            "/nodes/{id}",
            put(routes::prompt_nodes::update_node).delete(routes::prompt_nodes::delete_node),
        )
        .route("/templates/{id}/nodes/reorder", put(routes::prompt_nodes::reorder_nodes))
```

And declare the modules at the top of `lib.rs`:

```rust
pub mod routes;
```

Make sure `routes/mod.rs` exists and declares all route modules:

```rust
pub mod assets;
pub mod chat;
pub mod definitions;
pub mod health;
pub mod index;
pub mod ping;
pub mod prompt_nodes;
pub mod sessions;
pub mod templates;
```

- [ ] **Step 5: Add integration tests**

Create `shirita-web/tests/templates_integration.rs`:

```rust
// (Basic smoke: create template, list, add nodes, create session from template)
// Full integration test will be run in Task 9.
```

- [ ] **Step 6: Run backend tests**

Run: `cargo test -p shirita-core && cargo test -p shirita-web`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add shirita-web/src/routes/templates.rs shirita-web/src/routes/prompt_nodes.rs shirita-web/src/routes/sessions.rs shirita-web/src/lib.rs shirita-core/src/storage/
git commit -m "feat(m3): template + node API endpoints, session creation with template deep-copy"
```

---

## Task 6: Frontend API types + client for templates/nodes (TDD)

**Files:**
- Modify: `shirita-ui/src/api/types.ts`
- Modify: `shirita-ui/src/api/client.ts`
- Modify: `shirita-ui/src/api/client.test.ts`

- [ ] **Step 1: Add Template and PromptNode types to `shirita-ui/src/api/types.ts`**

Append:

```ts
export interface Template {
  id: string
  name: string
  meta: Record<string, unknown>
  created_at: string
  updated_at: string
}

export interface PromptNode {
  id: string
  owner_kind: 'template' | 'session'
  owner_id: string
  parent_id: string | null
  sort_order: number
  kind: 'folder' | 'ref'
  tag: string | null
  definition_id: string | null
  enabled: boolean
  created_at: string
}
```

- [ ] **Step 2: Add client functions to `shirita-ui/src/api/client.ts`**

Append (after the SSE functions):

```ts
// --- Templates ---

export function listTemplates(): Promise<Template[]> {
  return apiGet<Template[]>('/templates')
}

export function getTemplate(id: string): Promise<Template> {
  return apiGet<Template>(`/templates/${id}`)
}

export async function createTemplate(name: string): Promise<Template> {
  const res = await fetch(`${BASE}/api/templates`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  })
  if (!res.ok) throw new Error(`Create template failed: ${res.status}`)
  return res.json()
}

export async function updateTemplate(id: string, name: string): Promise<Template> {
  const res = await fetch(`${BASE}/api/templates/${id}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  })
  if (!res.ok) throw new Error(`Update template failed: ${res.status}`)
  return res.json()
}

export async function deleteTemplate(id: string): Promise<void> {
  const res = await fetch(`${BASE}/api/templates/${id}`, {
    method: 'DELETE',
    headers: authHeaders(),
  })
  if (!res.ok) throw new Error(`Delete template failed: ${res.status}`)
}

export async function duplicateTemplate(id: string): Promise<Template> {
  const res = await fetch(`${BASE}/api/templates/${id}/duplicate`, {
    method: 'POST',
    headers: authHeaders(),
  })
  if (!res.ok) throw new Error(`Duplicate template failed: ${res.status}`)
  return res.json()
}

// --- Prompt Nodes ---

export function listNodes(ownerKind: string, ownerId: string): Promise<PromptNode[]> {
  return apiGet<PromptNode[]>(`/templates/${ownerId}/nodes?owner_kind=${ownerKind}`)
}

export async function createNode(
  ownerKind: string,
  ownerId: string,
  body: { parent_id?: string | null; kind: string; tag?: string; definition_id?: string },
): Promise<PromptNode> {
  const res = await fetch(
    `${BASE}/api/templates/${ownerId}/nodes?owner_kind=${ownerKind}`,
    {
      method: 'POST',
      headers: { ...authHeaders(), 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    },
  )
  if (!res.ok) throw new Error(`Create node failed: ${res.status}`)
  return res.json()
}

export async function updateNode(
  nodeId: string,
  body: { parent_id?: string | null; sort_order?: number; tag?: string; definition_id?: string; enabled?: boolean },
): Promise<PromptNode> {
  const res = await fetch(`${BASE}/api/nodes/${nodeId}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Update node failed: ${res.status}`)
  return res.json()
}

export async function deleteNode(nodeId: string): Promise<void> {
  const res = await fetch(`${BASE}/api/nodes/${nodeId}`, {
    method: 'DELETE',
    headers: authHeaders(),
  })
  if (!res.ok) throw new Error(`Delete node failed: ${res.status}`)
}

export async function reorderNodes(
  ownerKind: string,
  ownerId: string,
  orderedIds: string[],
): Promise<void> {
  const res = await fetch(
    `${BASE}/api/templates/${ownerId}/nodes/reorder?owner_kind=${ownerKind}`,
    {
      method: 'PUT',
      headers: { ...authHeaders(), 'Content-Type': 'application/json' },
      body: JSON.stringify({ ordered_ids: orderedIds }),
    },
  )
  if (!res.ok) throw new Error(`Reorder nodes failed: ${res.status}`)
}
```

Also add the `Template` and `PromptNode` imports at the top of `client.ts`:

```ts
import type { Message, Session, Template, PromptNode } from './types'
```

- [ ] **Step 3: Add tests to `shirita-ui/src/api/client.test.ts`**

Add after the SSE describe block:

```ts
describe('template API', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('listTemplates GETs /api/templates', async () => {
    const templates = [{ id: 't1', name: 'Default', meta: {}, created_at: '', updated_at: '' }]
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, json: async () => templates }))
    const { listTemplates } = await import('./client')
    const result = await listTemplates()
    expect(result).toEqual(templates)
  })

  it('createTemplate POSTs to /api/templates', async () => {
    const t = { id: 't2', name: 'New', meta: {}, created_at: '', updated_at: '' }
    const fm = vi.fn().mockResolvedValue({ ok: true, json: async () => t })
    vi.stubGlobal('fetch', fm)
    const { createTemplate } = await import('./client')
    const result = await createTemplate('New')
    expect(result.name).toBe('New')
    expect(fm).toHaveBeenCalledWith('/api/templates', expect.objectContaining({ method: 'POST' }))
  })
})
```

- [ ] **Step 4: Run tests**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts`
Expected: all tests pass (previous + new template tests).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/api/
git commit -m "feat(m3): frontend api types + client for templates and prompt nodes"
```

---

## Task 7: NewChatView step 1 — avatar + name (TDD)

**Files:**
- Modify: `shirita-ui/src/views/NewChatView.vue` (replace stub)
- Create: `shirita-ui/src/components/AvatarPicker.vue`
- Create: `shirita-ui/src/views/NewChatView.test.ts`

- [ ] **Step 1: Create `shirita-ui/src/components/AvatarPicker.vue`**

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { Camera, Upload } from 'lucide-vue-next'

const emit = defineEmits<{
  select: [path: string | null]
}>()

const isOpen = ref(false)
const selectedPath = ref<string | null>(null)

// Stub: hardcoded avatar library (real avatars come from GET /api/avatars in Plan 4/5)
const library = ref<string[]>([])

function toggle() {
  isOpen.value = !isOpen.value
}

function selectAvatar(path: string | null) {
  selectedPath.value = path
  emit('select', path)
  isOpen.value = false
}
</script>

<template>
  <div class="relative">
    <!-- Avatar display circle -->
    <button
      type="button"
      class="relative w-20 h-20 rounded-full bg-sky/20 overflow-hidden group"
      @click="toggle"
    >
      <img
        v-if="selectedPath"
        :src="`/assets/${selectedPath}`"
        class="w-full h-full object-cover"
        alt=""
      />
      <div
        class="absolute inset-0 bg-black/0 group-hover:bg-black/20 transition-colors flex items-center justify-center"
      >
        <Camera
          :size="22"
          class="text-white/80 opacity-0 group-hover:opacity-100 transition-opacity"
        />
      </div>
    </button>

    <!-- Inline avatar library panel -->
    <div
      v-if="isOpen"
      class="absolute top-full left-0 mt-2 bg-white border border-line rounded-xl shadow-lg p-3 w-64 z-10"
    >
      <div class="flex flex-wrap gap-2 mb-2">
        <div
          v-for="(avatar, i) in library"
          :key="i"
          class="w-12 h-12 rounded-full bg-sky/10 overflow-hidden cursor-pointer hover:ring-2 ring-primary"
          @click="selectAvatar(avatar)"
        >
          <img :src="`/assets/${avatar}`" class="w-full h-full object-cover" alt="" />
        </div>
      </div>
      <p v-if="library.length === 0" class="text-muted text-xs text-center py-2">
        No avatars yet. Upload one below.
      </p>
      <button
        class="w-full flex items-center justify-center gap-1.5 text-xs text-muted hover:text-ink py-1.5 border border-dashed border-line rounded-lg"
        @click="selectAvatar(null)"
      >
        <Upload :size="14" />
        Upload new
      </button>
    </div>
  </div>
</template>
```

- [ ] **Step 2: Replace `shirita-ui/src/views/NewChatView.vue`**

```vue
<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import AvatarPicker from '../components/AvatarPicker.vue'

const router = useRouter()
const name = ref('')
const avatar = ref<string | null>(null)

const canProceed = computed(() => name.value.trim().length > 0)

function proceed() {
  const params: Record<string, string> = {}
  if (name.value.trim()) {
    params.name = name.value.trim()
  }
  if (avatar.value) {
    params.avatar = avatar.value
  }
  router.push({ path: '/new/prompt', query: params })
}
</script>

<template>
  <div class="max-w-[480px] mx-auto px-5 pt-10">
    <!-- Breadcrumb: Chat / New -->
    <div class="flex items-center gap-1.5 text-[13px] text-muted mb-8">
      <router-link to="/" class="hover:text-ink">Chat</router-link>
      <span>/</span>
      <span class="text-ink">New</span>
    </div>

    <div class="flex flex-col items-center gap-6">
      <!-- Avatar picker -->
      <AvatarPicker @select="avatar = $event" />

      <!-- Name input (merged label + field) -->
      <div class="w-full">
        <input
          v-model="name"
          type="text"
          placeholder="Name"
          class="w-full text-center text-xl font-semibold bg-transparent border-b-2 border-line
                 focus:border-primary outline-none pb-2 placeholder:text-muted/50"
          @keydown.enter="proceed()"
        />
      </div>

      <!-- Adaptive primary button -->
      <button
        :class="[
          'px-8 py-2.5 rounded-full font-medium text-[15px] transition-colors',
          canProceed
            ? 'bg-primary text-white hover:bg-primary-strong'
            : 'bg-line text-muted',
        ]"
        @click="proceed()"
      >
        {{ canProceed ? 'Next' : 'Skip' }}
      </button>
    </div>
  </div>
</template>
```

- [ ] **Step 3: Add route for step 2**

Modify `shirita-ui/src/router/index.ts` — add the `/new/prompt` route:

```ts
{ path: '/new/prompt', name: 'newPrompt', component: () => import('../views/NewChatPromptView.vue') },
```

- [ ] **Step 4: Write test `shirita-ui/src/views/NewChatView.test.ts`**

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import NewChatView from './NewChatView.vue'

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/new', component: NewChatView },
      { path: '/new/prompt', component: { template: '<div />' } },
    ],
  })
}

describe('NewChatView', () => {
  it('shows Skip button when name is empty', async () => {
    const router = makeRouter()
    router.push('/new')
    await router.isReady()
    const wrapper = mount(NewChatView, { global: { plugins: [router] } })
    expect(wrapper.text()).toContain('Skip')
  })

  it('shows Next button when name is filled', async () => {
    const router = makeRouter()
    router.push('/new')
    await router.isReady()
    const wrapper = mount(NewChatView, { global: { plugins: [router] } })
    const input = wrapper.find('input')
    await input.setValue('Neo')
    expect(wrapper.text()).toContain('Next')
  })

  it('navigates to /new/prompt with query on Next', async () => {
    const router = makeRouter()
    router.push('/new')
    await router.isReady()
    const wrapper = mount(NewChatView, { global: { plugins: [router] } })
    const input = wrapper.find('input')
    await input.setValue('Morpheus')
    await wrapper.find('button').trigger('click')
    await router.isReady()
    expect(router.currentRoute.value.path).toBe('/new/prompt')
    expect(router.currentRoute.value.query.name).toBe('Morpheus')
  })
})
```

- [ ] **Step 5: Run test**

Run: `cd shirita-ui && npx vitest run src/views/NewChatView.test.ts`
Expected: PASS (3 passed).

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/views/NewChatView.vue shirita-ui/src/views/NewChatView.test.ts shirita-ui/src/components/AvatarPicker.vue shirita-ui/src/router/index.ts
git commit -m "feat(m3): NewChatView step 1 — avatar picker + name + Skip/Next"
```

---

## Task 8: PromptTree component (shared core) (TDD)

**Files:**
- Create: `shirita-ui/src/components/PromptTree.vue`
- Create: `shirita-ui/src/components/PromptTree.test.ts`
- Create: `shirita-ui/src/components/NodeRow.vue`
- Create: `shirita-ui/src/components/NodePicker.vue`

- [ ] **Step 1: Create `shirita-ui/src/components/NodeRow.vue`**

```vue
<script setup lang="ts">
import { computed } from 'vue'
import { ChevronDown, GripVertical, Plus } from 'lucide-vue-next'
import type { PromptNode, Definition } from '../api/types'

const props = defineProps<{
  node: PromptNode
  definitions: Record<string, Definition>
  depth: number
  isExpanded: boolean
}>()

const emit = defineEmits<{
  toggleEnabled: []
  toggleExpand: []
  addChild: []
  selectDef: []
}>()

const label = computed(() => {
  if (props.node.kind === 'folder') {
    return props.node.tag || '(folder)'
  }
  // ref node: show the definition name
  const def = props.node.definition_id ? props.definitions[props.node.definition_id] : null
  return def ? def.name : '(missing definition)'
})

const isFolder = computed(() => props.node.kind === 'folder')
</script>

<template>
  <div
    data-test="node-row"
    :style="{ paddingLeft: `${depth * 20}px` }"
    class="flex items-center gap-1.5 py-1.5 group text-[14px]"
  >
    <!-- Drag handle (visual stub for M3) -->
    <GripVertical :size="14" class="text-muted/40 shrink-0" />

    <!-- Enable checkbox -->
    <input
      type="checkbox"
      :checked="node.enabled"
      class="w-3.5 h-3.5 rounded accent-primary shrink-0"
      data-test="enable-checkbox"
      @change="emit('toggleEnabled')"
    />

    <!-- Expand chevron (folder only) -->
    <button
      v-if="isFolder"
      data-test="expand-btn"
      class="text-muted hover:text-ink shrink-0"
      @click="emit('toggleExpand')"
    >
      <ChevronDown
        :size="14"
        :class="isExpanded ? '' : '-rotate-90'"
        class="transition-transform"
      />
    </button>
    <span v-else class="w-[14px] shrink-0" /> <!-- spacer for alignment -->

    <!-- Label -->
    <span
      :class="[
        'truncate flex-1',
        isFolder ? 'font-semibold text-mauve' : 'text-ink',
        !node.enabled ? 'line-through text-muted/50' : '',
      ]"
    >
      {{ label }}
    </span>

    <!-- Add child button (folder only) -->
    <button
      v-if="isFolder"
      data-test="add-child-btn"
      class="text-muted hover:text-primary opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
      @click="emit('addChild')"
    >
      <Plus :size="16" />
    </button>
    <span v-else class="w-[24px] shrink-0" /> <!-- spacer for alignment -->
  </div>
</template>
```

- [ ] **Step 2: Create `shirita-ui/src/components/NodePicker.vue`**

```vue
<script setup lang="ts">
import { ref, computed } from 'vue'
import { Search } from 'lucide-vue-next'
import type { Definition } from '../api/types'

const props = defineProps<{
  definitions: Definition[]
  filterType: string | null
}>()

const emit = defineEmits<{
  select: [definitionId: string]
  createNew: []
  changeType: [type: string]
}>()

const query = ref('')
const isOpen = ref(false)

const filtered = computed(() => {
  let defs = props.definitions
  if (props.filterType) {
    defs = defs.filter((d) => d.type === props.filterType)
  }
  if (query.value.trim()) {
    const q = query.value.toLowerCase()
    defs = defs.filter((d) => d.name.toLowerCase().includes(q))
  }
  return defs.slice(0, 8) // limit visible items
})

const typeOptions = ['char', 'world', 'persona', 'item', 'prompt']

function open() {
  isOpen.value = true
  query.value = ''
}

function close() {
  isOpen.value = false
}

defineExpose({ open, close })
</script>

<template>
  <div v-if="isOpen" data-test="node-picker" class="bg-white border border-line rounded-xl shadow-lg p-3 w-72 z-20">
    <!-- Search -->
    <div class="flex items-center gap-2 pb-2 mb-2 border-b border-line">
      <Search :size="14" class="text-muted shrink-0" />
      <input
        v-model="query"
        type="text"
        placeholder="Search definitions…"
        class="flex-1 text-[13px] bg-transparent outline-none placeholder:text-muted/50"
      />
    </div>

    <!-- Matching definitions -->
    <div class="max-h-40 overflow-y-auto">
      <button
        v-for="def in filtered"
        :key="def.id"
        class="w-full text-left px-2 py-1.5 text-[13px] hover:bg-surface rounded-md flex items-center gap-2"
        @click="emit('select', def.id); close()"
      >
        <span class="text-[11px] text-muted uppercase w-12 shrink-0">{{ def.type }}</span>
        <span class="truncate">{{ def.name }}</span>
      </button>
    </div>

    <p v-if="filtered.length === 0" class="text-muted text-xs py-2 text-center">
      No matching definitions
    </p>

    <!-- Divider -->
    <div class="border-t border-line my-2" />

    <!-- New definition -->
    <button
      class="w-full text-left px-2 py-1.5 text-[13px] text-primary hover:bg-surface rounded-md"
      @click="emit('createNew'); close()"
    >
      + New definition
    </button>

    <!-- Other type -->
    <div class="mt-1 text-[11px] text-muted px-2">Other type:</div>
    <div class="flex flex-wrap gap-1 mt-1">
      <button
        v-for="t in typeOptions"
        :key="t"
        :class="[
          'px-2 py-0.5 text-[11px] rounded-full',
          props.filterType === t ? 'bg-primary/10 text-primary' : 'text-muted hover:text-ink bg-line/30',
        ]"
        @click="emit('changeType', t)"
      >
        {{ t }}
      </button>
    </div>
  </div>
</template>
```

- [ ] **Step 3: Create `shirita-ui/src/components/PromptTree.vue`**

```vue
<script setup lang="ts">
import { ref, computed } from 'vue'
import type { PromptNode, Definition } from '../api/types'
import NodeRow from './NodeRow.vue'
import NodePicker from './NodePicker.vue'

const props = defineProps<{
  nodes: PromptNode[]
  definitions: Definition[]
}>()

const emit = defineEmits<{
  toggleEnabled: [nodeId: string]
  addNode: [parentId: string | null, definitionId: string]
  deleteNode: [nodeId: string]
}>()

const expanded = ref<Set<string>>(new Set())
const activePickerParent = ref<string | null>(null)

const defMap = computed<Record<string, Definition>>(() => {
  const m: Record<string, Definition> = {}
  for (const d of props.definitions) {
    m[d.id] = d
  }
  return m
})

function getChildren(parentId: string | null): PromptNode[] {
  return props.nodes
    .filter((n) => n.parent_id === parentId)
    .sort((a, b) => a.sort_order - b.sort_order)
}

function isExpanded(nodeId: string): boolean {
  return expanded.value.has(nodeId)
}

function toggleExpand(nodeId: string) {
  if (expanded.value.has(nodeId)) {
    expanded.value.delete(nodeId)
  } else {
    expanded.value.add(nodeId)
  }
}

function openPicker(parentId: string | null) {
  activePickerParent.value = parentId
}

function handleSelectDef(definitionId: string) {
  if (activePickerParent.value !== null || activePickerParent.value === null) {
    emit('addNode', activePickerParent.value, definitionId)
  }
  activePickerParent.value = null
}
</script>

<template>
  <div data-test="prompt-tree" class="border border-line rounded-xl p-3 bg-white">
    <!-- Root-level nodes (parent_id = null) -->
    <template v-for="node in getChildren(null)" :key="node.id">
      <NodeRow
        :node="node"
        :definitions="defMap"
        :depth="0"
        :is-expanded="isExpanded(node.id)"
        @toggle-enabled="emit('toggleEnabled', node.id)"
        @toggle-expand="toggleExpand(node.id)"
        @add-child="openPicker(node.id)"
      />

      <!-- Children (if expanded) -->
      <template v-if="node.kind === 'folder' && isExpanded(node.id)">
        <template v-for="child in getChildren(node.id)" :key="child.id">
          <NodeRow
            :node="child"
            :definitions="defMap"
            :depth="1"
            :is-expanded="false"
            @toggle-enabled="emit('toggleEnabled', child.id)"
          />
        </template>
      </template>

      <!-- Inline picker for this folder -->
      <div v-if="activePickerParent === node.id" class="ml-5 mt-1 mb-2">
        <NodePicker
          ref="pickerRef"
          :definitions="definitions"
          :filter-type="node.tag"
          @select="handleSelectDef"
          @create-new="() => {}"
          @change-type="() => {}"
        />
      </div>
    </template>

    <!-- Root + button -->
    <div class="mt-2">
      <button
        data-test="root-add-btn"
        class="flex items-center gap-1 text-[13px] text-muted hover:text-primary"
        @click="openPicker(null)"
      >
        + Add node
      </button>
      <div v-if="activePickerParent === null" class="mt-1">
        <NodePicker
          :definitions="definitions"
          :filter-type="null"
          @select="handleSelectDef"
          @create-new="() => {}"
          @change-type="() => {}"
        />
      </div>
    </div>
  </div>
</template>
```

- [ ] **Step 4: Write the PromptTree test `shirita-ui/src/components/PromptTree.test.ts`**

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import PromptTree from './PromptTree.vue'
import type { PromptNode, Definition } from '../api/types'

function makeNode(overrides: Partial<PromptNode> = {}): PromptNode {
  return {
    id: 'n1', owner_kind: 'template', owner_id: 't1', parent_id: null,
    sort_order: 0, kind: 'folder', tag: 'char', definition_id: null,
    enabled: true, created_at: '',
    ...overrides,
  }
}

function makeDef(overrides: Partial<Definition> = {}): Definition {
  return {
    id: 'd1', type: 'char', name: 'Alice', content: '...', meta: {},
    ...overrides,
  }
}

describe('PromptTree', () => {
  it('renders nodes from props', () => {
    const nodes = [
      makeNode({ id: 'n1', kind: 'folder', tag: 'char' }),
    ]
    const wrapper = mount(PromptTree, {
      props: { nodes, definitions: [] },
    })
    expect(wrapper.text()).toContain('char')
  })

  it('shows ref node label as definition name', () => {
    const nodes = [
      makeNode({ id: 'r1', kind: 'ref', definition_id: 'd1', tag: null }),
    ]
    const defs = [makeDef({ id: 'd1', name: 'Alice' })]
    const wrapper = mount(PromptTree, {
      props: { nodes, definitions: defs },
    })
    expect(wrapper.text()).toContain('Alice')
  })

  it('emits toggleEnabled when checkbox clicked', async () => {
    const nodes = [makeNode({ id: 'n1' })]
    const wrapper = mount(PromptTree, {
      props: { nodes, definitions: [] },
    })
    await wrapper.find('[data-test="enable-checkbox"]').trigger('change')
    expect(wrapper.emitted('toggleEnabled')).toBeTruthy()
    expect(wrapper.emitted('toggleEnabled')![0]).toEqual(['n1'])
  })

  it('shows root add button', () => {
    const wrapper = mount(PromptTree, {
      props: { nodes: [], definitions: [] },
    })
    expect(wrapper.find('[data-test="root-add-btn"]').exists()).toBe(true)
  })
})
```

- [ ] **Step 5: Run tests**

Run: `cd shirita-ui && npx vitest run src/components/PromptTree.test.ts`
Expected: PASS (4 passed).

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/components/PromptTree.vue shirita-ui/src/components/PromptTree.test.ts shirita-ui/src/components/NodeRow.vue shirita-ui/src/components/NodePicker.vue
git commit -m "feat(m3): PromptTree component — node tree + inline definition picker"
```

---

## Task 9: NewChatView step 2 — template picker + PromptTree + create (TDD)

**Files:**
- Create: `shirita-ui/src/views/NewChatPromptView.vue`
- Create: `shirita-ui/src/stores/library.ts`

- [ ] **Step 1: Create `shirita-ui/src/stores/library.ts`** (definitions + templates cache)

```ts
import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Definition, Template, PromptNode } from '../api/types'
import { listDefinitions } from '../api/client'
import { listTemplates, listNodes } from '../api/client'

export const useLibraryStore = defineStore('library', () => {
  const definitions = ref<Definition[]>([])
  const templates = ref<Template[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function loadDefinitions() {
    try {
      definitions.value = await listDefinitions()
    } catch (e) {
      error.value = (e as Error).message
    }
  }

  async function loadTemplates() {
    try {
      templates.value = await listTemplates()
    } catch (e) {
      error.value = (e as Error).message
    }
  }

  async function loadAll() {
    loading.value = true
    error.value = null
    try {
      await Promise.all([loadDefinitions(), loadTemplates()])
    } catch (e) {
      error.value = (e as Error).message
    } finally {
      loading.value = false
    }
  }

  return { definitions, templates, loading, error, loadDefinitions, loadTemplates, loadAll }
})
```

Note: `listDefinitions` needs to be added to the API client. In `client.ts`, add:

```ts
export function listDefinitions(type?: string): Promise<Definition[]> {
  const qs = type ? `?type=${encodeURIComponent(type)}` : ''
  return apiGet<Definition[]>(`/definitions${qs}`)
}
```

- [ ] **Step 2: Create `shirita-ui/src/views/NewChatPromptView.vue`**

```vue
<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useLibraryStore } from '../stores/library'
import { createSession, createNode } from '../api/client'
import PromptTree from '../components/PromptTree.vue'
import type { PromptNode, Template } from '../api/types'

const route = useRoute()
const router = useRouter()
const library = useLibraryStore()

const sessionName = (route.query.name as string) || 'Untitled'
const selectedTemplateId = ref<string | null>(null)
const nodes = ref<PromptNode[]>([])
const creating = ref(false)
const error = ref<string | null>(null)

onMounted(async () => {
  await library.loadAll()
})

async function selectTemplate(templateId: string) {
  selectedTemplateId.value = templateId
  try {
    nodes.value = await (await import('../api/client')).listNodes('template', templateId)
  } catch {
    nodes.value = []
  }
}

async function handleAddNode(parentId: string | null, definitionId: string) {
  if (!selectedTemplateId.value) return
  try {
    const node = await createNode('template', selectedTemplateId.value, {
      parent_id: parentId,
      kind: 'ref',
      definition_id: definitionId,
    })
    nodes.value = [...nodes.value, node]
  } catch (e) {
    error.value = (e as Error).message
  }
}

async function handleToggleEnabled(nodeId: string) {
  const node = nodes.value.find((n) => n.id === nodeId)
  if (!node) return
  const { updateNode } = await import('../api/client')
  try {
    const updated = await updateNode(nodeId, { enabled: !node.enabled })
    const idx = nodes.value.findIndex((n) => n.id === nodeId)
    if (idx !== -1) {
      nodes.value = [...nodes.value.slice(0, idx), updated, ...nodes.value.slice(idx + 1)]
    }
  } catch (e) {
    error.value = (e as Error).message
  }
}

async function createChat() {
  creating.value = true
  error.value = null
  try {
    const session = await createSession(sessionName, selectedTemplateId.value)
    router.push(`/chat/${session.id}`)
  } catch (e) {
    error.value = (e as Error).message
  } finally {
    creating.value = false
  }
}
</script>

<template>
  <div class="max-w-[560px] mx-auto px-5 pt-8 pb-12">
    <!-- Breadcrumb -->
    <div class="flex items-center gap-1.5 text-[13px] text-muted mb-6">
      <router-link to="/" class="hover:text-ink">Chat</router-link>
      <span>/</span>
      <router-link to="/new" class="hover:text-ink">New</router-link>
      <span>/</span>
      <span class="text-ink">Prompt</span>
    </div>

    <h2 class="text-lg font-semibold mb-1">{{ sessionName }}</h2>
    <p class="text-[13px] text-muted mb-6">Choose a prompt template and configure the tree.</p>

    <!-- Template selector -->
    <div class="mb-4">
      <label class="text-[13px] text-muted mb-1.5 block">Template</label>
      <select
        v-model="selectedTemplateId"
        class="w-full border border-line rounded-lg px-3 py-2 text-[14px] bg-white outline-none focus:border-primary/50"
        @change="selectTemplate(($event.target as HTMLSelectElement).value)"
      >
        <option :value="null">None (start empty)</option>
        <option v-for="t in library.templates" :key="t.id" :value="t.id">
          {{ t.name }}
        </option>
      </select>
    </div>

    <!-- Prompt tree -->
    <PromptTree
      v-if="selectedTemplateId"
      :nodes="nodes"
      :definitions="library.definitions"
      @add-node="handleAddNode"
      @toggle-enabled="handleToggleEnabled"
    />

    <p v-if="error" class="text-coral text-sm mt-3">{{ error }}</p>

    <!-- Create button -->
    <div class="mt-8">
      <button
        :disabled="creating"
        class="w-full py-2.5 rounded-full font-medium bg-primary text-white hover:bg-primary-strong transition-colors disabled:opacity-50"
        @click="createChat"
      >
        {{ creating ? 'Creating…' : 'Create conversation' }}
      </button>
    </div>
  </div>
</template>
```

Note: `createSession` needs to be added to the API client. In `client.ts`, update the existing function or add:

```ts
export async function createSession(name: string, templateId?: string | null): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, template_id: templateId || undefined }),
  })
  if (!res.ok) throw new Error(`Create session failed: ${res.status}`)
  return res.json()
}
```

Replace the old `createSession` in the existing sessions routes handler too (backend already updated in Task 5 Step 3).

- [ ] **Step 3: Run full frontend test suite**

Run: `cd shirita-ui && npm run test`
Expected: all tests pass (previous + PromptTree tests).

- [ ] **Step 4: Commit**

```bash
git add shirita-ui/src/views/NewChatPromptView.vue shirita-ui/src/stores/library.ts shirita-ui/src/api/client.ts
git commit -m "feat(m3): NewChatView step 2 — template picker + PromptTree + create conversation"
```

---

## Task 10: Full verification

**Files:**
- (none — run suite, fix issues)

- [ ] **Step 1: Run all backend tests**

Run: `cargo test -p shirita-core && cargo test -p shirita-web`
Expected: all tests pass (existing + new migration/storage/assembly tests).

- [ ] **Step 2: Run all frontend tests**

Run: `cd shirita-ui && npm run test`
Expected: all tests pass (previous plans + new template/client/PromptTree/NewChatView tests).

- [ ] **Step 3: Type-check + build**

Run: `cd shirita-ui && npm run build`
Expected: no type errors; production build succeeds.

- [ ] **Step 4: Integration smoke test**

With the backend running, create a template, add nodes, create a session from the template, and verify nodes are deep-copied:

```bash
# Start backend
cargo run -p shirita-web &
sleep 2

# Create a definition
curl -s -X POST http://127.0.0.1:8787/api/definitions \
  -H "Authorization: Bearer test" \
  -H "Content-Type: application/json" \
  -d '{"type":"char","name":"Alice","content":"I am Alice"}'

# Create a template
TID=$(curl -s -X POST http://127.0.0.1:8787/api/templates \
  -H "Authorization: Bearer test" \
  -H "Content-Type: application/json" \
  -d '{"name":"RP Template"}' | jq -r '.id')

# Create session from template
SID=$(curl -s -X POST http://127.0.0.1:8787/api/sessions \
  -H "Authorization: Bearer test" \
  -H "Content-Type: application/json" \
  -d "{\"name\":\"Test Chat\",\"template_id\":\"$TID\"}" | jq -r '.id')

echo "Template: $TID, Session: $SID"
# Verify: session.template_id should be set
```

- [ ] **Step 5: Commit any fixes**

```bash
git add -A && git commit -m "chore(m3): full test + type-check pass — templates/nodes/new-chat slice"
```

---

## Self-review notes

- **Spec coverage:** §4.3 new-chat step 1 (avatar + name + Skip/Next) ✓; §4.4 step 2 (template picker + PromptTree + Create conversation) ✓; §5 PromptTree component (folder/ref nodes, indentation, enable toggles, `+` inline picker) ✓; §6 backend tables (`templates`, `prompt_nodes` with `owner_kind`, `chat_sessions.template_id`) ✓; §6.2 session creation deep-copies template nodes ✓; §6.3 assembly upgraded to tree walk ✓; §7 API endpoints (templates CRUD, nodes CRUD, reorder, session with template_id) ✓.
- **Backend changes:** New tables `templates` + `prompt_nodes` via migration; `chat_sessions.template_id` column; extended `Storage` trait with 10 new methods; upgraded `assemble_system_prompt` to walk a node tree; new route handlers for `/api/templates`, `/api/templates/{id}/nodes`, `/api/nodes/{id}`.
- **Frontend changes:** New `Template` + `PromptNode` types; extended API client; `library` store (definitions + templates cache); `PromptTree` + `NodeRow` + `NodePicker` components; `AvatarPicker` component; `NewChatView` replaced (step 1); `NewChatPromptView` created (step 2); `/new/prompt` route.
- **Deferred to later plans:** Overrides (local vs global — Plan 4), book editor PromptTree reuse (Plan 4), avatar library backend (Plan 4/5), drag-and-drop reordering (M3 uses API reorder, DnD is future polish), definition editing inside PromptTree nodes (Plan 4).
- **Type consistency:** `PromptNode.kind` is `'folder' | 'ref'` matching backend `NodeKind`; `owner_kind` is `'template' | 'session'` matching `OwnerKind`. `createSession` now accepts optional `templateId` matching the updated backend handler.
- **Migration safety:** `mounted_definitions` remains on `chat_sessions` (not dropped) for backward compatibility until Plan 4 fully migrates conversation assembly.
