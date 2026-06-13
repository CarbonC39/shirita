//! SqliteStorage：连接、迁移与 definitions CRUD。

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};

use crate::models::definition::{Definition, DefinitionType};
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
}
