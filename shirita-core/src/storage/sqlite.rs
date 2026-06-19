//! SqliteStorage：连接、迁移与 definitions CRUD。

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};

use std::collections::HashMap;

use crate::models::asset::Asset;
use crate::models::def_type::DefType;
use crate::models::definition::Definition;
use crate::models::message::{Message, Role};
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::models::session::Session;
use crate::models::summary::Summary;
use crate::models::template::Template;
use crate::{Result, Storage};

#[derive(Clone)]
pub struct SqliteStorage {
    pool: SqlitePool,
}

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

fn row_to_definition(row: &SqliteRow) -> Result<Definition> {
    let meta_str: String = row.try_get("meta")?;
    Ok(Definition {
        id: row.try_get("id")?,
        def_type: row.try_get("type")?,
        name: row.try_get("name")?,
        content: row.try_get("content")?,
        meta: serde_json::from_str(&meta_str)?,
    })
}

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
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        sort_order: row.try_get("sort_order")?,
        active_leaf_id: row.try_get("active_leaf_id")?,
        preview: None,
    })
}

/// Strip markdown/reasoning markup down to plain text, for contexts (like the
/// home-list snippet) that show a short excerpt rather than rendering it.
fn to_plain_text(text: &str) -> String {
    let think_re = regex::Regex::new(r"(?is)<think>.*?</think>").unwrap();
    let tag_re = regex::Regex::new(r"(?s)<[^>]+>").unwrap();
    let fence_re = regex::Regex::new(r"(?s)```[^\n]*\n?(.*?)```").unwrap();
    let inline_code_re = regex::Regex::new(r"`([^`]*)`").unwrap();
    let image_re = regex::Regex::new(r"!\[[^\]]*\]\([^)]*\)").unwrap();
    let link_re = regex::Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap();
    let heading_re = regex::Regex::new(r"(?m)^\s{0,3}#{1,6}\s+").unwrap();
    let quote_re = regex::Regex::new(r"(?m)^\s{0,3}>\s?").unwrap();
    let list_re = regex::Regex::new(r"(?m)^\s*(?:[-*+]|\d+\.)\s+").unwrap();
    let emphasis_re = regex::Regex::new(r"(\*{1,3}|_{1,3})").unwrap();

    let s = think_re.replace_all(text, "");
    let s = tag_re.replace_all(&s, "");
    let s = fence_re.replace_all(&s, "$1");
    let s = inline_code_re.replace_all(&s, "$1");
    let s = image_re.replace_all(&s, "");
    let s = link_re.replace_all(&s, "$1");
    let s = heading_re.replace_all(&s, "");
    let s = quote_re.replace_all(&s, "");
    let s = list_re.replace_all(&s, "");
    emphasis_re.replace_all(&s, "").into_owned()
}

/// One-line snippet of a message for the home list: newlines collapsed, trimmed,
/// capped so the payload stays small.
fn message_preview(text: &str) -> String {
    let flat = to_plain_text(text).split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 140 {
        format!("{}…", flat.chars().take(140).collect::<String>())
    } else {
        flat
    }
}

fn row_to_message(row: &SqliteRow) -> Result<Message> {
    let role_str: String = row.try_get("role")?;
    let snapshot: String = row.try_get("snapshot_state")?;
    let attachments: String = row.try_get("attachments")?;
    let is_hidden: i64 = row.try_get("is_hidden")?;
    let is_anchor: i64 = row.try_get("is_anchor")?;
    Ok(Message {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        parent_id: row.try_get("parent_id")?,
        role: Role::from_db(&role_str)?,
        raw_content: row.try_get("raw_content")?,
        display_content: row.try_get("display_content")?,
        is_hidden: is_hidden != 0,
        is_anchor: is_anchor != 0,
        attachments: serde_json::from_str(&attachments)?,
        snapshot_state: serde_json::from_str(&snapshot)?,
        created_at: row.try_get("created_at")?,
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

    async fn referenced_definition_ids(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT DISTINCT definition_id FROM prompt_nodes WHERE definition_id IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| r.get::<String, _>("definition_id")).collect())
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

    async fn create_session(&self, session: &Session) -> Result<()> {
        let override_config = serde_json::to_string(&session.override_config)?;
        let current_state = serde_json::to_string(&session.current_state)?;
        let mounted = serde_json::to_string(&session.mounted_definitions)?;
        sqlx::query(
            "INSERT INTO chat_sessions (id, name, avatar, template_id, override_config, current_state, mounted_definitions, created_at, updated_at, sort_order) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&session.id)
        .bind(&session.name)
        .bind(&session.avatar)
        .bind(&session.template_id)
        .bind(override_config)
        .bind(current_state)
        .bind(mounted)
        .bind(&session.created_at)
        .bind(&session.updated_at)
        .bind(session.sort_order)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let row = sqlx::query(
            "SELECT id, name, avatar, template_id, override_config, current_state, mounted_definitions, created_at, updated_at, sort_order, active_leaf_id FROM chat_sessions WHERE id = ?",
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
        // Correlated subquery grabs the latest visible message per session so the
        // home cards can show a recent-activity snippet without an N+1 round-trip.
        let rows = sqlx::query(
            "SELECT id, name, avatar, template_id, override_config, current_state, mounted_definitions, created_at, updated_at, sort_order, active_leaf_id, \
             (SELECT COALESCE(display_content, raw_content) FROM messages \
                WHERE session_id = chat_sessions.id AND is_hidden = 0 \
                ORDER BY created_at DESC LIMIT 1) AS preview \
             FROM chat_sessions ORDER BY sort_order DESC, updated_at DESC, name",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                let mut s = row_to_session(row)?;
                let raw: Option<String> = row.try_get("preview").ok().flatten();
                s.preview = raw.map(|t| message_preview(&t));
                Ok(s)
            })
            .collect()
    }

    async fn delete_session(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM messages WHERE session_id = ?").bind(id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM summaries WHERE session_id = ?").bind(id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM prompt_nodes WHERE owner_kind = 'session' AND owner_id = ?").bind(id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM chat_sessions WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    async fn set_mounted_definitions(&self, session_id: &str, ids: &[String]) -> Result<()> {
        let json = serde_json::to_string(ids)?;
        sqlx::query("UPDATE chat_sessions SET mounted_definitions = ? WHERE id = ?")
            .bind(json)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_session_profile(&self, session_id: &str, name: &str, avatar: Option<&str>) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE chat_sessions SET name = ?, avatar = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(avatar)
            .bind(now)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn reorder_sessions(&self, ordered_ids: &[String]) -> Result<()> {
        // Assign descending sort keys around "now" so the manual order persists
        // (top item largest) while later activity still floats a chat above it.
        let base = chrono::Utc::now().timestamp_millis();
        let mut tx = self.pool.begin().await?;
        for (i, id) in ordered_ids.iter().enumerate() {
            sqlx::query("UPDATE chat_sessions SET sort_order = ? WHERE id = ?")
                .bind(base - i as i64)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn create_message(&self, message: &Message) -> Result<()> {
        let snapshot = serde_json::to_string(&message.snapshot_state)?;
        let attachments = serde_json::to_string(&message.attachments)?;
        sqlx::query(
            "INSERT INTO messages \
             (id, session_id, parent_id, role, raw_content, display_content, is_hidden, is_anchor, attachments, snapshot_state, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&message.id)
        .bind(&message.session_id)
        .bind(&message.parent_id)
        .bind(message.role.as_str())
        .bind(&message.raw_content)
        .bind(&message.display_content)
        .bind(message.is_hidden as i64)
        .bind(message.is_anchor as i64)
        .bind(attachments)
        .bind(snapshot)
        .bind(&message.created_at)
        .execute(&self.pool)
        .await?;
        // Bump the session's activity so it floats to the top of the home list
        // (default ordering is by recency). Manual reorders use the same key.
        let now = chrono::Utc::now();
        sqlx::query("UPDATE chat_sessions SET updated_at = ?, sort_order = ? WHERE id = ?")
            .bind(now.to_rfc3339())
            .bind(now.timestamp_millis())
            .bind(&message.session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let rows = sqlx::query(
            "SELECT id, session_id, parent_id, role, raw_content, display_content, is_hidden, is_anchor, attachments, snapshot_state, created_at \
             FROM messages WHERE session_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_message).collect()
    }

    async fn get_message(&self, id: &str) -> Result<Option<Message>> {
        let row = sqlx::query(
            "SELECT id, session_id, parent_id, role, raw_content, display_content, is_hidden, is_anchor, attachments, snapshot_state, created_at \
             FROM messages WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some(row_to_message(&r)?)),
            None => Ok(None),
        }
    }

    async fn update_message(&self, message: &Message) -> Result<()> {
        let snapshot = serde_json::to_string(&message.snapshot_state)?;
        sqlx::query(
            "UPDATE messages SET raw_content = ?, display_content = ?, is_hidden = ?, snapshot_state = ? \
             WHERE id = ?",
        )
        .bind(&message.raw_content)
        .bind(&message.display_content)
        .bind(message.is_hidden as i64)
        .bind(snapshot)
        .bind(&message.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_session_active_leaf(&self, session_id: &str, leaf_id: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE chat_sessions SET active_leaf_id = ? WHERE id = ?")
            .bind(leaf_id)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // --- templates ---
    async fn create_template(&self, template: &Template) -> Result<()> {
        let meta = serde_json::to_string(&template.meta)?;
        sqlx::query("INSERT INTO templates (id, name, meta, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&template.id).bind(&template.name).bind(meta)
            .bind(&template.created_at).bind(&template.updated_at)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_template(&self, id: &str) -> Result<Option<Template>> {
        let row = sqlx::query("SELECT id, name, meta, created_at, updated_at FROM templates WHERE id = ?")
            .bind(id).fetch_optional(&self.pool).await?;
        match row { Some(r) => Ok(Some(row_to_template(&r)?)), None => Ok(None) }
    }

    async fn list_templates(&self) -> Result<Vec<Template>> {
        let rows = sqlx::query("SELECT id, name, meta, created_at, updated_at FROM templates ORDER BY name")
            .fetch_all(&self.pool).await?;
        rows.iter().map(row_to_template).collect()
    }

    async fn update_template(&self, template: &Template) -> Result<()> {
        let meta = serde_json::to_string(&template.meta)?;
        sqlx::query("UPDATE templates SET name = ?, meta = ?, updated_at = ? WHERE id = ?")
            .bind(&template.name).bind(meta).bind(&template.updated_at).bind(&template.id)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn orphaned_definitions_for_template(&self, template_id: &str) -> Result<Vec<Definition>> {
        let rows = sqlx::query(
            "SELECT DISTINCT definition_id FROM prompt_nodes \
             WHERE owner_kind = 'template' AND owner_id = ? AND definition_id IS NOT NULL \
             AND definition_id NOT IN ( \
                 SELECT definition_id FROM prompt_nodes \
                 WHERE definition_id IS NOT NULL AND NOT (owner_kind = 'template' AND owner_id = ?) \
             )",
        )
        .bind(template_id)
        .bind(template_id)
        .fetch_all(&self.pool)
        .await?;
        let mut defs = Vec::new();
        for row in &rows {
            let def_id: String = row.get("definition_id");
            if let Some(d) = self.get_definition(&def_id).await? {
                defs.push(d);
            }
        }
        Ok(defs)
    }

    async fn delete_template(&self, id: &str, delete_orphans: bool) -> Result<()> {
        if delete_orphans {
            let orphans = self.orphaned_definitions_for_template(id).await?;
            for def in orphans {
                self.delete_definition(&def.id).await?;
            }
        }
        sqlx::query("DELETE FROM prompt_nodes WHERE owner_kind = 'template' AND owner_id = ?").bind(id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM templates WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    // --- prompt nodes ---
    async fn list_nodes(&self, owner_kind: &OwnerKind, owner_id: &str) -> Result<Vec<PromptNode>> {
        let rows = sqlx::query("SELECT id, owner_kind, owner_id, parent_id, sort_order, kind, tag, definition_id, enabled, created_at, meta FROM prompt_nodes WHERE owner_kind = ? AND owner_id = ? ORDER BY sort_order ASC, id ASC")
            .bind(owner_kind.as_str()).bind(owner_id).fetch_all(&self.pool).await?;
        rows.iter().map(row_to_prompt_node).collect()
    }

    async fn create_node(&self, node: &PromptNode) -> Result<()> {
        let meta = serde_json::to_string(&node.meta)?;
        sqlx::query("INSERT INTO prompt_nodes (id, owner_kind, owner_id, parent_id, sort_order, kind, tag, definition_id, enabled, created_at, meta) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&node.id).bind(node.owner_kind.as_str()).bind(&node.owner_id)
            .bind(&node.parent_id).bind(node.sort_order).bind(node.kind.as_str())
            .bind(&node.tag).bind(&node.definition_id).bind(node.enabled as i64).bind(&node.created_at)
            .bind(meta)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn get_node(&self, id: &str) -> Result<Option<PromptNode>> {
        let row = sqlx::query("SELECT id, owner_kind, owner_id, parent_id, sort_order, kind, tag, definition_id, enabled, created_at, meta FROM prompt_nodes WHERE id = ?")
            .bind(id).fetch_optional(&self.pool).await?;
        match row { Some(r) => Ok(Some(row_to_prompt_node(&r)?)), None => Ok(None) }
    }

    async fn update_node(&self, node: &PromptNode) -> Result<()> {
        let meta = serde_json::to_string(&node.meta)?;
        sqlx::query("UPDATE prompt_nodes SET parent_id = ?, sort_order = ?, kind = ?, tag = ?, definition_id = ?, enabled = ?, meta = ? WHERE id = ?")
            .bind(&node.parent_id).bind(node.sort_order).bind(node.kind.as_str())
            .bind(&node.tag).bind(&node.definition_id).bind(node.enabled as i64).bind(meta).bind(&node.id)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn delete_node(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM prompt_nodes WHERE parent_id = ?").bind(id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM prompt_nodes WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    async fn reorder_nodes(&self, owner_kind: &OwnerKind, owner_id: &str, ordered_ids: &[String]) -> Result<()> {
        for (i, nid) in ordered_ids.iter().enumerate() {
            sqlx::query("UPDATE prompt_nodes SET sort_order = ? WHERE id = ? AND owner_kind = ? AND owner_id = ?")
                .bind(i as i64).bind(nid).bind(owner_kind.as_str()).bind(owner_id)
                .execute(&self.pool).await?;
        }
        Ok(())
    }

    async fn copy_nodes(&self, from_kind: &OwnerKind, from_id: &str, to_kind: &OwnerKind, to_id: &str) -> Result<HashMap<String, String>> {
        let source = self.list_nodes(from_kind, from_id).await?;
        let mut id_map = HashMap::new();
        let mut sorted = source.clone();
        sorted.sort_by_key(|n| (n.parent_id.is_some(), n.sort_order));
        for node in &sorted {
            let new_id = uuid::Uuid::new_v4().to_string();
            let new_parent_id = node.parent_id.as_ref().and_then(|pid| id_map.get(pid).cloned());
            let copy = PromptNode {
                id: new_id.clone(), owner_kind: to_kind.clone(), owner_id: to_id.to_string(),
                parent_id: new_parent_id, sort_order: node.sort_order, kind: node.kind.clone(),
                tag: node.tag.clone(), definition_id: node.definition_id.clone(),
                enabled: node.enabled, created_at: chrono::Utc::now().to_rfc3339(),
                meta: node.meta.clone(),
            };
            self.create_node(&copy).await?;
            id_map.insert(node.id.clone(), new_id);
        }
        Ok(id_map)
    }

    // --- override config ---
    async fn update_session_override_config(&self, session_id: &str, config: &serde_json::Value) -> Result<()> {
        let json = serde_json::to_string(config)?;
        sqlx::query("UPDATE chat_sessions SET override_config = ? WHERE id = ?")
            .bind(json).bind(session_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn set_local_definition(&self, session_id: &str, def_id: &str, patch: &serde_json::Value) -> Result<()> {
        // 合并 {"local_definitions": {"<def_id>": <patch>}}；键经 json_object 绑参构造，不拼 path。
        let patch_str = serde_json::to_string(patch)?;
        sqlx::query(
            "UPDATE chat_sessions SET override_config = json_patch(\
                COALESCE(override_config, '{}'), \
                json_object('local_definitions', json_object(?, json(?)))) \
             WHERE id = ?",
        )
        .bind(def_id)
        .bind(patch_str)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn clear_local_definition(&self, session_id: &str, def_id: &str) -> Result<()> {
        // RFC7396：把该键置 JSON null → json_patch 删除之，不动同对象其它 def。
        sqlx::query(
            "UPDATE chat_sessions SET override_config = json_patch(\
                COALESCE(override_config, '{}'), \
                json_object('local_definitions', json_object(?, json('null')))) \
             WHERE id = ?",
        )
        .bind(def_id)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_local_variables(&self, session_id: &str, variables: &serde_json::Value) -> Result<()> {
        // RFC7396 对数组是整体替换，正合「整列替换变量声明」语义。
        let vars_str = serde_json::to_string(variables)?;
        sqlx::query(
            "UPDATE chat_sessions SET override_config = json_patch(\
                COALESCE(override_config, '{}'), \
                json_object('local_variables', json(?))) \
             WHERE id = ?",
        )
        .bind(vars_str)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // --- summaries ---
    async fn create_summary(&self, summary: &Summary) -> Result<()> {
        sqlx::query(
            "INSERT INTO summaries (id, session_id, cutoff_message_id, content, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&summary.id)
        .bind(&summary.session_id)
        .bind(&summary.cutoff_message_id)
        .bind(&summary.content)
        .bind(&summary.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_summaries(&self, session_id: &str) -> Result<Vec<Summary>> {
        let rows = sqlx::query(
            "SELECT id, session_id, cutoff_message_id, content, created_at \
             FROM summaries WHERE session_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                use sqlx::Row;
                Ok(Summary {
                    id: r.try_get("id")?,
                    session_id: r.try_get("session_id")?,
                    cutoff_message_id: r.try_get("cutoff_message_id")?,
                    content: r.try_get("content")?,
                    created_at: r.try_get("created_at")?,
                })
            })
            .collect()
    }

    // --- settings ---
    async fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let row = sqlx::query("SELECT value FROM settings WHERE key = ?").bind(key).fetch_optional(&self.pool).await?;
        match row { Some(r) => { let raw: String = r.try_get("value")?; Ok(Some(serde_json::from_str(&raw)?)) } None => Ok(None) }
    }

    async fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        let raw = serde_json::to_string(value)?;
        sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
            .bind(key).bind(raw).execute(&self.pool).await?;
        Ok(())
    }

    async fn list_settings(&self) -> Result<Vec<(String, serde_json::Value)>> {
        let rows = sqlx::query("SELECT key, value FROM settings ORDER BY key").fetch_all(&self.pool).await?;
        rows.iter().map(|r| { let key: String = r.try_get("key")?; let raw: String = r.try_get("value")?; Ok((key, serde_json::from_str(&raw)?)) }).collect()
    }

    async fn delete_setting(&self, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM settings WHERE key = ?").bind(key).execute(&self.pool).await?;
        Ok(())
    }

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

    async fn list_assets(&self) -> Result<Vec<Asset>> {
        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT id, name, path, created_at FROM assets ORDER BY created_at DESC, id DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, name, path, created_at)| Asset { id, name, path, created_at })
            .collect())
    }

    async fn get_asset(&self, id: &str) -> Result<Option<Asset>> {
        let row = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT id, name, path, created_at FROM assets WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id, name, path, created_at)| Asset { id, name, path, created_at }))
    }

    async fn create_asset(&self, asset: &Asset) -> Result<()> {
        sqlx::query("INSERT INTO assets (id, name, path, created_at) VALUES (?, ?, ?, ?)")
            .bind(&asset.id)
            .bind(&asset.name)
            .bind(&asset.path)
            .bind(&asset.created_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn rename_asset(&self, id: &str, name: &str) -> Result<()> {
        sqlx::query("UPDATE assets SET name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_asset(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM assets WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// --- Row mappers ---
fn row_to_template(row: &SqliteRow) -> Result<Template> {
    let meta_str: String = row.try_get("meta")?;
    Ok(Template {
        id: row.try_get("id")?, name: row.try_get("name")?, meta: serde_json::from_str(&meta_str)?,
        created_at: row.try_get("created_at")?, updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_prompt_node(row: &SqliteRow) -> Result<PromptNode> {
    let owner_kind_str: String = row.try_get("owner_kind")?;
    let kind_str: String = row.try_get("kind")?;
    let enabled: i64 = row.try_get("enabled")?;
    let meta_str: String = row.try_get("meta")?;
    Ok(PromptNode {
        id: row.try_get("id")?, owner_kind: OwnerKind::from_db(&owner_kind_str)?,
        owner_id: row.try_get("owner_id")?, parent_id: row.try_get("parent_id")?,
        sort_order: row.try_get("sort_order")?, kind: NodeKind::from_db(&kind_str)?,
        tag: row.try_get("tag")?, definition_id: row.try_get("definition_id")?,
        enabled: enabled != 0, created_at: row.try_get("created_at")?,
        meta: serde_json::from_str(&meta_str)?,
    })
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
    async fn template_delete_reports_and_optionally_deletes_orphans() {
        let s = temp_storage().await;
        let t1 = Template::new("T1");
        let t2 = Template::new("T2");
        s.create_template(&t1).await.unwrap();
        s.create_template(&t2).await.unwrap();
        let solo = Definition::new("world", "Solo", "only in T1");
        let shared = Definition::new("world", "Shared", "in both templates");
        s.create_definition(&solo).await.unwrap();
        s.create_definition(&shared).await.unwrap();
        s.create_node(&PromptNode::new_ref(OwnerKind::Template, &t1.id, None, 0, &solo.id)).await.unwrap();
        s.create_node(&PromptNode::new_ref(OwnerKind::Template, &t1.id, None, 1, &shared.id)).await.unwrap();
        s.create_node(&PromptNode::new_ref(OwnerKind::Template, &t2.id, None, 0, &shared.id)).await.unwrap();

        let orphans = s.orphaned_definitions_for_template(&t1.id).await.unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].id, solo.id);

        s.delete_template(&t1.id, true).await.unwrap();
        assert!(s.get_definition(&solo.id).await.unwrap().is_none());
        assert!(s.get_definition(&shared.id).await.unwrap().is_some());
        assert!(s.get_template(&t1.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn summaries_roundtrip() {
        let s = temp_storage().await;
        let sess = crate::models::session::Session::new("s");
        s.create_session(&sess).await.unwrap();
        let sum = crate::models::summary::Summary::new(&sess.id, "msg-7", "earlier summary");
        s.create_summary(&sum).await.unwrap();
        let got = s.list_summaries(&sess.id).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].cutoff_message_id, "msg-7");
        assert_eq!(got[0].content, "earlier summary");
        // 其他会话不串
        assert!(s.list_summaries("other").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn message_is_anchor_roundtrips() {
        let store = temp_storage().await;
        let s = Sess::new("anchor");
        store.create_session(&s).await.unwrap();
        let mut m = Msg::new(&s.id, None, Role::User, "<start>");
        m.is_anchor = true;
        store.create_message(&m).await.unwrap();
        let got = store.get_message(&m.id).await.unwrap().unwrap();
        assert!(got.is_anchor);
        // a plain message defaults to false
        let m2 = Msg::new(&s.id, None, Role::User, "hi");
        store.create_message(&m2).await.unwrap();
        assert!(!store.get_message(&m2.id).await.unwrap().unwrap().is_anchor);
    }

    #[test]
    fn message_preview_strips_markdown_and_thinking() {
        let raw = "<think>pondering...</think>**Hello** _world_, see [docs](https://x) and `code` and ```\nblock\n```\n# Heading\n> quoted\n- item one";
        let got = message_preview(raw);
        assert!(!got.contains("<think>"));
        assert!(!got.contains("pondering"));
        assert!(!got.contains('*'));
        assert!(!got.contains('_'));
        assert!(!got.contains('`'));
        assert!(!got.contains('#'));
        assert!(!got.contains('>'));
        assert!(got.contains("Hello world"));
        assert!(got.contains("docs"));
        assert!(got.contains("code"));
        assert!(got.contains("block"));
        assert!(got.contains("Heading"));
        assert!(got.contains("quoted"));
        assert!(got.contains("item one"));
    }

    #[tokio::test]
    async fn message_attachments_roundtrip() {
        let store = temp_storage().await;
        let s = Sess::new("attach");
        store.create_session(&s).await.unwrap();
        let mut m = Msg::new(&s.id, None, Role::User, "look at this");
        m.attachments = vec!["asset-1".into(), "asset-2".into()];
        store.create_message(&m).await.unwrap();
        let got = store.get_message(&m.id).await.unwrap().unwrap();
        assert_eq!(got.attachments, vec!["asset-1".to_string(), "asset-2".to_string()]);
        // a plain message defaults to no attachments
        let m2 = Msg::new(&s.id, None, Role::User, "hi");
        store.create_message(&m2).await.unwrap();
        assert!(store.get_message(&m2.id).await.unwrap().unwrap().attachments.is_empty());
        // list_messages goes through the same row mapping
        let listed = store.list_messages(&s.id).await.unwrap();
        assert_eq!(listed.iter().find(|x| x.id == m.id).unwrap().attachments.len(), 2);
    }

    #[tokio::test]
    async fn active_leaf_and_message_updates_roundtrip() {
        let store = temp_storage().await;
        let s = Sess::new("Tree");
        store.create_session(&s).await.unwrap();
        let m = Msg::new(&s.id, None, Role::User, "hello");
        store.create_message(&m).await.unwrap();

        store.set_session_active_leaf(&s.id, Some(&m.id)).await.unwrap();
        let got = store.get_session(&s.id).await.unwrap().unwrap();
        assert_eq!(got.active_leaf_id.as_deref(), Some(m.id.as_str()));

        let fetched = store.get_message(&m.id).await.unwrap().unwrap();
        assert_eq!(fetched.raw_content, "hello");

        let mut edited = fetched.clone();
        edited.raw_content = "edited".into();
        edited.display_content = Some("EDITED".into());
        edited.is_hidden = true;
        store.update_message(&edited).await.unwrap();
        let after = store.get_message(&m.id).await.unwrap().unwrap();
        assert_eq!(after.raw_content, "edited");
        assert_eq!(after.display_content.as_deref(), Some("EDITED"));
        assert!(after.is_hidden);
    }

    #[tokio::test]
    async fn migrations_create_tables() {
        let storage = temp_storage().await;
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

    #[tokio::test]
    async fn definition_crud_roundtrip() {
        let storage = temp_storage().await;

        // create
        let mut def = Definition::new("char", "Alice", "<char>hi</char>");
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
        updated.def_type = "persona".into();
        storage.update_definition(&updated).await.unwrap();
        let got = storage.get_definition(&def.id).await.unwrap().unwrap();
        assert_eq!(got.name, "Alicia");
        assert_eq!(got.def_type, "persona");

        // delete
        storage.delete_definition(&def.id).await.unwrap();
        assert!(storage.get_definition(&def.id).await.unwrap().is_none());
        assert!(storage.list_definitions().await.unwrap().is_empty());
    }

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

    use crate::models::message::Message as Msg;
    use crate::models::session::Session as Sess;

    #[tokio::test]
    async fn session_and_message_roundtrip() {
        let storage = temp_storage().await;

        let session = Sess::new("Chat 1");
        storage.create_session(&session).await.unwrap();

        let got = storage.get_session(&session.id).await.unwrap().unwrap();
        assert_eq!(got, session);
        assert_eq!(storage.list_sessions().await.unwrap().len(), 1);

        let m1 = Msg::new(&session.id, None, Role::User, "hello");
        storage.create_message(&m1).await.unwrap();
        let m2 = Msg::new(&session.id, Some(m1.id.clone()), Role::Assistant, "hi there");
        storage.create_message(&m2).await.unwrap();

        let msgs = storage.list_messages(&session.id).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].id, m1.id);
        assert_eq!(msgs[1].id, m2.id);
        assert_eq!(msgs[1].parent_id.as_deref(), Some(m1.id.as_str()));
        assert_eq!(msgs[1].role, Role::Assistant);
    }

    #[tokio::test]
    async fn sessions_order_by_recency_then_manual_reorder() {
        let storage = temp_storage().await;

        // Three sessions with explicit, increasing sort keys → newest on top.
        let mut a = Sess::new("A");
        a.sort_order = 100;
        let mut b = Sess::new("B");
        b.sort_order = 200;
        let mut c = Sess::new("C");
        c.sort_order = 300;
        for s in [&a, &b, &c] {
            storage.create_session(s).await.unwrap();
        }
        let ids: Vec<_> = storage.list_sessions().await.unwrap().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![c.id.clone(), b.id.clone(), a.id.clone()]);

        // A new message in the oldest (A) floats it to the top by recency.
        let m = Msg::new(&a.id, None, Role::User, "hi");
        storage.create_message(&m).await.unwrap();
        let top = storage.list_sessions().await.unwrap()[0].id.clone();
        assert_eq!(top, a.id);

        // Manual reorder pins the given order (top-to-bottom).
        storage.reorder_sessions(&[b.id.clone(), c.id.clone(), a.id.clone()]).await.unwrap();
        let ids: Vec<_> = storage.list_sessions().await.unwrap().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![b.id, c.id, a.id]);
    }

    #[tokio::test]
    async fn assets_crud_roundtrip() {
        let storage = temp_storage().await;
        assert!(storage.list_assets().await.unwrap().is_empty());

        let a = crate::models::asset::Asset::new("Sunset", "abc.png");
        storage.create_asset(&a).await.unwrap();
        let listed = storage.list_assets().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Sunset");
        assert_eq!(listed[0].path, "abc.png");

        storage.rename_asset(&a.id, "Dawn").await.unwrap();
        assert_eq!(storage.get_asset(&a.id).await.unwrap().unwrap().name, "Dawn");

        storage.delete_asset(&a.id).await.unwrap();
        assert!(storage.get_asset(&a.id).await.unwrap().is_none());
        assert!(storage.list_assets().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn override_config_atomic_json_ops() {
        let storage = temp_storage().await;
        let s = Sess::new("ov");
        storage.create_session(&s).await.unwrap();

        // set 在无 local_definitions 时由合并创建并写键
        storage
            .set_local_definition(&s.id, "def-a", &serde_json::json!({ "content": "A" }))
            .await
            .unwrap();
        let got = storage.get_session(&s.id).await.unwrap().unwrap();
        assert_eq!(got.override_config["local_definitions"]["def-a"]["content"], "A");

        // 第二个 def 不互相覆盖
        storage
            .set_local_definition(&s.id, "def-b", &serde_json::json!({ "content": "B" }))
            .await
            .unwrap();
        // 局部变量整列替换，且与 local_definitions 共存
        storage
            .set_local_variables(&s.id, &serde_json::json!([{ "name": "hp", "type": "number", "initial": 100 }]))
            .await
            .unwrap();
        let got = storage.get_session(&s.id).await.unwrap().unwrap();
        assert_eq!(got.override_config["local_definitions"]["def-a"]["content"], "A");
        assert_eq!(got.override_config["local_definitions"]["def-b"]["content"], "B");
        assert_eq!(got.override_config["local_variables"][0]["name"], "hp");

        // clear 仅删该键，不动其它
        storage.clear_local_definition(&s.id, "def-a").await.unwrap();
        let got = storage.get_session(&s.id).await.unwrap().unwrap();
        assert!(got.override_config["local_definitions"].get("def-a").is_none());
        assert_eq!(got.override_config["local_definitions"]["def-b"]["content"], "B");
    }

    #[tokio::test]
    async fn session_mounts_roundtrip() {
        let storage = temp_storage().await;
        let mut s = Sess::new("m");
        s.mounted_definitions = vec!["a".into(), "b".into()];
        storage.create_session(&s).await.unwrap();
        assert_eq!(
            storage.get_session(&s.id).await.unwrap().unwrap().mounted_definitions,
            vec!["a", "b"]
        );

        storage.set_mounted_definitions(&s.id, &["x".into()]).await.unwrap();
        assert_eq!(
            storage.get_session(&s.id).await.unwrap().unwrap().mounted_definitions,
            vec!["x"]
        );
    }
}
