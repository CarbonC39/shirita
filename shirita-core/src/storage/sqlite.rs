//! SqliteStorage：连接、迁移与 definitions CRUD。

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};

use crate::models::definition::{Definition, DefinitionType};
use crate::models::message::{Message, Role};
use crate::models::session::Session;
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

fn row_to_session(row: &SqliteRow) -> Result<Session> {
    let override_config: String = row.try_get("override_config")?;
    let current_state: String = row.try_get("current_state")?;
    let mounted: String = row.try_get("mounted_definitions")?;
    Ok(Session {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        avatar: row.try_get("avatar")?,
        override_config: serde_json::from_str(&override_config)?,
        current_state: serde_json::from_str(&current_state)?,
        mounted_definitions: serde_json::from_str(&mounted)?,
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

    async fn create_session(&self, session: &Session) -> Result<()> {
        let override_config = serde_json::to_string(&session.override_config)?;
        let current_state = serde_json::to_string(&session.current_state)?;
        let mounted = serde_json::to_string(&session.mounted_definitions)?;
        sqlx::query(
            "INSERT INTO chat_sessions (id, name, avatar, override_config, current_state, mounted_definitions) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&session.id)
        .bind(&session.name)
        .bind(&session.avatar)
        .bind(override_config)
        .bind(current_state)
        .bind(mounted)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let row = sqlx::query(
            "SELECT id, name, avatar, override_config, current_state, mounted_definitions FROM chat_sessions WHERE id = ?",
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
            "SELECT id, name, avatar, override_config, current_state, mounted_definitions FROM chat_sessions ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_session).collect()
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
