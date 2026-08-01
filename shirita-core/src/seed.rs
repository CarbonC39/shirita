//! First-launch seeding helpers.

use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::models::template::Template;
use crate::models::user::User;
use crate::storage::Storage;
use crate::Result;

/// Description of the state variable update protocol (injected into the model; content can be edited by the user as defined by `builtin`).
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
    let mut content = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "content");
    content.kind = NodeKind::Content;
    content.tag = None;
    storage.create_node(&content).await?;
    let mut hist = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 1, "history");
    hist.kind = NodeKind::History;
    hist.tag = None;
    storage.create_node(&hist).await?;
    Ok(())
}

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

/// Backfill the content hash of any asset whose file exists but whose hash is
/// NULL (pre-`0020` rows). Idempotent; run at startup after migrations.
pub async fn ensure_asset_hashes<S: Storage + ?Sized>(storage: &S, assets_dir: &str) -> Result<()> {
    for asset in storage.list_assets(None).await? {
        if asset.hash.is_some() {
            continue;
        }
        let path = std::path::Path::new(assets_dir).join(&asset.path);
        match tokio::fs::read(&path).await {
            Ok(bytes) => storage.set_asset_hash(&asset.id, &crate::sha256_hex(&bytes)).await?,
            Err(_) => tracing::warn!(asset = %asset.id, path = %asset.path, "ensure_asset_hashes: file missing, skipping"),
        }
    }
    Ok(())
}

/// One-time backfill for the `is_global` regex-rule flag (added after global
/// rules were inferred from "zero references" instead of stored explicitly —
/// see `conversation::effective_regex_rules`). Every `regex_rule` Definition
/// that is currently unreferenced gets `is_global: true` so already-relied-
/// upon global rules keep applying after the flag becomes the source of
/// truth. Idempotent — a rule that already has `is_global` set (true or
/// false) is left untouched, so this never re-flips a rule a user has since
/// explicitly turned off.
pub async fn ensure_global_regex_flag<S: Storage + ?Sized>(storage: &S) -> Result<()> {
    let referenced: std::collections::HashSet<String> =
        storage.referenced_definition_ids().await?.into_iter().collect();
    for def in storage.list_definitions().await? {
        if def.def_type != "regex_rule" {
            continue;
        }
        if def.meta.get("is_global").is_some() {
            continue;
        }
        if referenced.contains(&def.id) {
            continue;
        }
        let mut updated = def.clone();
        if let Some(obj) = updated.meta.as_object_mut() {
            obj.insert("is_global".to_string(), serde_json::json!(true));
        }
        storage.update_definition(&updated).await?;
    }
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

/// How the first account should be provisioned when the `users` table is empty.
pub struct BootstrapCreds {
    /// `SHIRITA_BOOTSTRAP_USER` / `SHIRITA_BOOTSTRAP_PASSWORD` from the
    /// environment (web). `None` on desktop, which always auto-provisions a
    /// local account.
    pub user: Option<String>,
    pub password: Option<String>,
    /// Whether an auto-generated account gets an interactive password.
    /// `true` (web): hash a random password and return its plaintext for the
    /// entry point to log once. `false` (desktop): leave `password_hash` empty —
    /// the account can't be used to log in interactively until the user sets a
    /// password in Settings (and until then desktop auto-authenticates).
    pub interactive: bool,
}

/// The plaintext credentials of an auto-generated account, returned so the entry
/// point can log them once (to stdout / `docker logs`). Only populated for the
/// web auto-generate path; env-provided and desktop paths return `None`.
pub struct GeneratedCreds {
    pub username: String,
    pub password: String,
}

/// Ensure exactly one bootstrap account exists on a fresh database. Idempotent —
/// if any user already exists this is a no-op (it never clobbers an account,
/// matching every other `ensure_*`). Resolution order:
/// 1. `SHIRITA_BOOTSTRAP_USER` + `SHIRITA_BOOTSTRAP_PASSWORD` both set → create
///    that account (password hashed). Returns `None` (the password came from
///    the environment, not generated, so nothing to log).
/// 2. Otherwise auto-generate. Web (`interactive`) → random password (hashed),
///    returned for logging. Desktop (`!interactive`) → empty `password_hash`.
///
/// On desktop the returned `None` still means "an account was created if needed"
/// is not signalled here — the desktop shell doesn't need to log a password
/// because it auto-authenticates.
pub async fn ensure_bootstrap_user<S: Storage + ?Sized>(
    storage: &S,
    creds: &BootstrapCreds,
) -> Result<Option<GeneratedCreds>> {
    if storage.count_users().await? > 0 {
        return Ok(None);
    }
    // Env-provided takes priority.
    if let (Some(user), Some(password)) = (creds.user.as_ref(), creds.password.as_ref()) {
        let hash = crate::auth_password::hash_password(password)?;
        storage.create_user(&User::new(user.clone(), hash)).await?;
        return Ok(None);
    }
    let username = creds
        .user
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "admin".to_string());
    if creds.interactive {
        let password = random_password();
        let hash = crate::auth_password::hash_password(&password)?;
        storage.create_user(&User::new(username.clone(), hash)).await?;
        Ok(Some(GeneratedCreds { username, password }))
    } else {
        // Desktop: no interactive password until the user sets one in Settings.
        storage
            .create_user(&User::new(username.clone(), String::new()))
            .await?;
        Ok(Some(GeneratedCreds { username, password: String::new() }))
    }
}

/// 16 random bytes → base64url (no pad). Used only for the one-time
/// auto-generated bootstrap password.
fn random_password() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
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

    #[tokio::test]
    async fn is_idempotent_and_skips_when_templates_exist() {
        let storage = mem_storage().await;
        ensure_default_template(&storage).await.unwrap();
        ensure_default_template(&storage).await.unwrap();
        assert_eq!(storage.list_templates().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn ensure_asset_hashes_backfills_missing() {
        use crate::models::asset::Asset;
        let dir = tempfile::tempdir().unwrap();
        let assets_dir = dir.path().join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::write(assets_dir.join("img.png"), b"hello-bytes").unwrap();

        let storage = SqliteStorage::connect(dir.path().join("s.db").to_str().unwrap()).await.unwrap();
        storage.run_migrations().await.unwrap();
        let a = Asset::new("img", "img.png"); // hash None
        storage.create_asset(&a).await.unwrap();

        crate::ensure_asset_hashes(&storage, assets_dir.to_str().unwrap()).await.unwrap();
        crate::ensure_asset_hashes(&storage, assets_dir.to_str().unwrap()).await.unwrap(); // idempotent

        let got = storage.get_asset(&a.id).await.unwrap().unwrap();
        assert_eq!(got.hash.as_deref(), Some(crate::sha256_hex(b"hello-bytes").as_str()));
    }

    #[tokio::test]
    async fn ensure_global_regex_flag_backfills_unreferenced_rules_without_clobbering_explicit_false() {
        let storage = mem_storage().await;

        // No is_global key at all, unreferenced -> should become true.
        let mut unflagged = Definition::new("regex_rule", "unflagged", "");
        unflagged.meta = serde_json::json!({ "pattern": "a", "replacement": "" });
        storage.create_definition(&unflagged).await.unwrap();

        // Already explicitly false -> must stay false, not be overwritten.
        let mut explicit_false = Definition::new("regex_rule", "explicit-false", "");
        explicit_false.meta = serde_json::json!({ "pattern": "b", "replacement": "", "is_global": false });
        storage.create_definition(&explicit_false).await.unwrap();

        crate::ensure_global_regex_flag(&storage).await.unwrap();
        crate::ensure_global_regex_flag(&storage).await.unwrap(); // idempotent

        let got_unflagged = storage.get_definition(&unflagged.id).await.unwrap().unwrap();
        assert_eq!(got_unflagged.meta["is_global"], true);

        let got_explicit_false = storage.get_definition(&explicit_false.id).await.unwrap().unwrap();
        assert_eq!(got_explicit_false.meta["is_global"], false);
    }

    fn web_env(user: &str, pass: &str) -> BootstrapCreds {
        BootstrapCreds {
            user: Some(user.into()),
            password: Some(pass.into()),
            interactive: true,
        }
    }

    #[tokio::test]
    async fn bootstrap_env_creates_user_and_is_idempotent() {
        let storage = mem_storage().await;
        assert_eq!(storage.count_users().await.unwrap(), 0);

        ensure_bootstrap_user(&storage, &web_env("alice", "s3cret")).await.unwrap();
        let got = storage.get_user_by_username("alice").await.unwrap().unwrap();
        assert!(got.has_password());
        assert!(crate::verify_password("s3cret", &got.password_hash));

        // Second call must not clobber (idempotent) — env path returns None and
        // leaves the existing hash intact.
        let second = ensure_bootstrap_user(&storage, &web_env("alice", "different"))
            .await
            .unwrap();
        assert!(second.is_none());
        let got2 = storage.get_user_by_username("alice").await.unwrap().unwrap();
        assert!(crate::verify_password("s3cret", &got2.password_hash), "original password preserved");
        assert_eq!(storage.count_users().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn bootstrap_web_autogen_returns_password() {
        let storage = mem_storage().await;
        let creds = ensure_bootstrap_user(
            &storage,
            &BootstrapCreds { user: None, password: None, interactive: true },
        )
        .await
        .unwrap()
        .expect("web autogen returns generated creds");
        assert_eq!(creds.username, "admin");
        let got = storage.get_user_by_username("admin").await.unwrap().unwrap();
        assert!(got.has_password());
        assert!(crate::verify_password(&creds.password, &got.password_hash));
    }

    #[tokio::test]
    async fn bootstrap_desktop_autogen_has_empty_password() {
        let storage = mem_storage().await;
        let creds = ensure_bootstrap_user(
            &storage,
            &BootstrapCreds { user: None, password: None, interactive: false },
        )
        .await
        .unwrap()
        .expect("desktop autogen returns generated creds");
        assert_eq!(creds.username, "admin");
        assert!(creds.password.is_empty(), "desktop path does not mint a password");
        let got = storage.get_user_by_username("admin").await.unwrap().unwrap();
        assert!(!got.has_password(), "desktop account has no interactive password yet");
    }

    #[tokio::test]
    async fn bootstrap_partial_env_autogens_password_for_provided_username() {
        let storage = mem_storage().await;
        // Only the username is provided (no password). The real config flow
        // (apply_bootstrap_env) clears both on a partial config, so this only
        // arises from a hand-built BootstrapCreds — but it should still create
        // the named account with an auto-generated password rather than fail.
        let creds = ensure_bootstrap_user(
            &storage,
            &BootstrapCreds { user: Some("alice".to_string()), password: None, interactive: true },
        )
        .await
        .unwrap()
        .expect("autogen path returns generated creds");
        assert_eq!(creds.username, "alice");
        let got = storage.get_user_by_username("alice").await.unwrap().unwrap();
        assert!(got.has_password());
        assert!(crate::verify_password(&creds.password, &got.password_hash));
        assert!(storage.get_user_by_username("admin").await.unwrap().is_none());
    }
}
