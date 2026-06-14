//! First-launch seeding helpers.

use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::models::template::Template;
use crate::storage::Storage;
use crate::Result;

/// Ensure at least one template exists (first-launch convenience). When the
/// templates table is empty, create a "Default" template carrying the
/// mandatory, undeletable chat-history node — so the Book picker and the
/// new-chat flow are never empty on a fresh database. Idempotent.
pub async fn ensure_default_template<S: Storage + ?Sized>(storage: &S) -> Result<()> {
    if !storage.list_templates().await?.is_empty() {
        return Ok(());
    }
    let t = Template::new("Default");
    storage.create_template(&t).await?;
    let mut hist = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "history");
    hist.kind = NodeKind::History;
    hist.tag = None;
    storage.create_node(&hist).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::SqliteStorage;

    async fn mem_storage() -> SqliteStorage {
        // a pooled `:memory:` db gives each connection its own database, so use
        // a temp file (matching the other storage tests).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seed_test.db");
        std::mem::forget(dir);
        let storage = SqliteStorage::connect(path.to_str().unwrap()).await.unwrap();
        storage.run_migrations().await.unwrap();
        storage
    }

    #[tokio::test]
    async fn seeds_a_default_template_with_history() {
        let storage = mem_storage().await;
        ensure_default_template(&storage).await.unwrap();
        let templates = storage.list_templates().await.unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0].name, "Default");
        let nodes = storage
            .list_nodes(&OwnerKind::Template, &templates[0].id)
            .await
            .unwrap();
        assert!(nodes.iter().any(|n| n.kind == NodeKind::History));
    }

    #[tokio::test]
    async fn is_idempotent_and_skips_when_templates_exist() {
        let storage = mem_storage().await;
        ensure_default_template(&storage).await.unwrap();
        ensure_default_template(&storage).await.unwrap();
        assert_eq!(storage.list_templates().await.unwrap().len(), 1);
    }
}
