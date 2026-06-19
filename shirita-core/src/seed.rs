//! First-launch seeding helpers.

use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::models::template::Template;
use crate::storage::Storage;
use crate::Result;

/// 状态变量更新协议说明（注入给模型；内容随 builtin 定义可被用户编辑）。
const STATE_PROTOCOL_TEXT: &str = "\
You can update tracked story variables by emitting self-closing <state_update> \
tags inline in your reply. They are folded into the running state and stripped \
from what the reader sees.

Syntax: <state_update action=\"ACTION\" key=\"VAR\" value=\"VALUE\"/>
Actions:
- SET — set VAR to VALUE
- ADD — add numeric VALUE to VAR
- SUB — subtract numeric VALUE from VAR
- TOGGLE — flip a boolean VAR (omit value)
- APPEND — append VALUE to a list/string VAR
- REMOVE — remove VALUE from a list VAR
Only emit updates for variables that actually change; keep narrative prose separate from the tags.";

/// (id, name, content, kind) for each seeded builtin `protocol` definition.
const BUILTIN_PROTOCOLS: [(&str, &str, &str, &str); 2] = [
    ("builtin-protocol-state-update", "Variable Update Protocol", STATE_PROTOCOL_TEXT, "state_update"),
    ("builtin-protocol-html-patch", "HTML Card Patch Protocol", crate::html_patch::INSTRUCTION, "html_patch"),
];

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

/// Seed the builtin `protocol` definitions (fixed ids, create-if-absent so it is
/// idempotent and self-heals if one was deleted). Their content is the static
/// protocol text the engine injects (see conversation::assemble_request).
pub async fn ensure_builtin_definitions<S: Storage + ?Sized>(storage: &S) -> Result<()> {
    for (id, name, content, kind) in BUILTIN_PROTOCOLS {
        if storage.get_definition(id).await?.is_none() {
            let mut d = Definition::new("protocol", name, content);
            d.id = id.to_string();
            d.meta = serde_json::json!({ "kind": kind });
            storage.create_definition(&d).await?;
        }
    }
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
    async fn seeds_protocol_definitions_idempotently() {
        let storage = mem_storage().await;
        ensure_builtin_definitions(&storage).await.unwrap();
        ensure_builtin_definitions(&storage).await.unwrap(); // idempotent

        let protos: Vec<_> = storage
            .list_definitions().await.unwrap()
            .into_iter().filter(|d| d.def_type == "protocol").collect();
        assert_eq!(protos.len(), 2, "exactly two builtin protocols, no duplicates");
        let su = protos.iter().find(|d| d.id == "builtin-protocol-state-update").unwrap();
        assert_eq!(su.meta["kind"], "state_update");
        assert!(su.content.contains("<state_update"));
        let hp = protos.iter().find(|d| d.id == "builtin-protocol-html-patch").unwrap();
        assert_eq!(hp.meta["kind"], "html_patch");
        assert!(hp.content.contains("<<<<<<< SEARCH"));
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
