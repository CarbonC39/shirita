use async_trait::async_trait;

use std::collections::HashMap;

use crate::models::asset::Asset;
use crate::models::auth_session::AuthSessionRecord;
use crate::models::def_type::DefType;
use crate::models::definition::Definition;
use crate::models::pack::Pack;
use crate::models::message::Message;
use crate::models::prompt_node::{OwnerKind, PromptNode};
use crate::models::session::Session;
use crate::models::summary::Summary;
use crate::models::template::Template;
use crate::models::user::User;
use crate::Result;

pub mod sqlite;

/// Storage abstraction layer.
#[async_trait]
pub trait Storage: Send + Sync {
    async fn create_definition(&self, def: &Definition) -> Result<()>;
    async fn get_definition(&self, id: &str) -> Result<Option<Definition>>;
    async fn list_definitions(&self) -> Result<Vec<Definition>>;
    /// Definitions of a single `def_type`, filtered in SQL (no full-table load).
    async fn list_definitions_by_type(&self, def_type: &str) -> Result<Vec<Definition>>;
    /// Distinct `definition_id`s referenced by any prompt node (all owners).
    /// NOTE: a `regex_rule` Definition with `meta.is_global == true` is, by
    /// design, never referenced by any node — "unreferenced" is NOT the same
    /// as "safe to treat as dead/orphaned" for that one def_type. Any new
    /// code built on this method that reasons about orphans/cleanup must
    /// explicitly exclude `def_type == "regex_rule" && meta.is_global == true`
    /// rows, the way `effective_regex_rules` and `list_regex_scopes` do.
    async fn referenced_definition_ids(&self) -> Result<Vec<String>>;
    /// `(template name, definition id)` for every template-owned ref node, via a
    /// single JOIN — replaces a per-template `list_nodes` N+1.
    async fn template_definition_refs(&self) -> Result<Vec<(String, String)>>;
    /// Update a definition's fields; returns whether a row existed, so callers
    /// can report 404 without a separate existence read.
    async fn update_definition(&self, def: &Definition) -> Result<bool>;
    async fn delete_definition(&self, id: &str) -> Result<()>;

    // --- sessions ---
    async fn create_session(&self, session: &Session) -> Result<()>;
    async fn get_session(&self, id: &str) -> Result<Option<Session>>;
    async fn list_sessions(&self) -> Result<Vec<Session>>;
    /// Deletes a session, its messages, and its associated node tree.
    async fn delete_session(&self, id: &str) -> Result<()>;
    /// Replaces the entire list of session mount IDs.
    async fn set_mounted_definitions(&self, session_id: &str, ids: &[String]) -> Result<()>;
    /// Replace the session's ordered mounted-pack id list wholesale.
    async fn set_mounted_packs(&self, session_id: &str, ids: &[String]) -> Result<()>;
    /// Update a session's editable profile (title + avatar).
    async fn update_session_profile(&self, session_id: &str, name: &str, avatar: Option<&str>) -> Result<()>;
    /// Persists the manual sorting of the session in the given order (with the first item at the top).
    async fn reorder_sessions(&self, ordered_ids: &[String]) -> Result<()>;
    /// Set (or clear with `None`) the session's active branch leaf.
    async fn set_session_active_leaf(&self, session_id: &str, leaf_id: Option<&str>) -> Result<()>;

    // --- messages ---
    async fn create_message(&self, message: &Message) -> Result<()>;
    /// Insert many messages in a single transaction (all-or-nothing). Callers
    /// guarantee parent-before-child order for the self-referential FK.
    async fn create_messages(&self, messages: &[Message]) -> Result<()>;
    /// Insert `message` and advance the session's active leaf to it atomically,
    /// so a panel/state-carrier append can't leave a node the leaf never reaches.
    async fn create_message_and_advance_leaf(&self, message: &Message) -> Result<()>;
    /// Returns all messages for a given session in ascending order by `created_at` (using `id` as a tiebreaker).
    async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>>;
    async fn get_message(&self, id: &str) -> Result<Option<Message>>;
    /// Update an existing message's editable fields (raw/display content, hidden).
    async fn update_message(&self, message: &Message) -> Result<()>;
    /// Delete `message_id` and its entire subtree (all descendants). If the
    /// session's active leaf falls inside the deleted subtree, reset it to the
    /// deleted root's parent (or `None` if the root had no parent). Returns the
    /// new active leaf id for the session (may be `None`).
    async fn delete_message_subtree(&self, message_id: &str) -> Result<Option<String>>;

    // --- templates ---
    async fn create_template(&self, template: &Template) -> Result<()>;
    /// Create a template together with its initial nodes in one transaction, so a
    /// failure can't leave a template missing its required magic nodes.
    async fn create_template_with_nodes(&self, template: &Template, nodes: &[PromptNode]) -> Result<()>;
    async fn get_template(&self, id: &str) -> Result<Option<Template>>;
    /// The template with this exact name, if any — for import conflict checks
    /// (one lookup instead of loading every template).
    async fn get_template_by_name(&self, name: &str) -> Result<Option<Template>>;
    async fn list_templates(&self) -> Result<Vec<Template>>;
    async fn update_template(&self, template: &Template) -> Result<()>;
    /// Definitions referenced by this template's nodes that no other template or
    /// session references — i.e. would become unreachable if the template is deleted.
    async fn orphaned_definitions_for_template(&self, template_id: &str) -> Result<Vec<Definition>>;
    /// Delete a template and its node tree. If `delete_orphans` is true, also delete
    /// the definitions reported by `orphaned_definitions_for_template`.
    async fn delete_template(&self, id: &str, delete_orphans: bool) -> Result<()>;

    // --- prompt nodes ---
    async fn list_nodes(&self, owner_kind: &OwnerKind, owner_id: &str) -> Result<Vec<PromptNode>>;
    async fn create_node(&self, node: &PromptNode) -> Result<()>;
    async fn get_node(&self, id: &str) -> Result<Option<PromptNode>>;
    async fn update_node(&self, node: &PromptNode) -> Result<()>;
    async fn delete_node(&self, id: &str) -> Result<()>;
    async fn reorder_nodes(&self, owner_kind: &OwnerKind, owner_id: &str, ordered_ids: &[String]) -> Result<()>;
    async fn copy_nodes(
        &self, from_kind: &OwnerKind, from_id: &str, to_kind: &OwnerKind, to_id: &str,
    ) -> Result<HashMap<String, String>>;
    /// If the session owns no nodes yet, deep-copy `template_id`'s tree into it,
    /// all in one transaction. Returns whether it materialized. The check and the
    /// copy share a transaction so two concurrent calls can't both copy.
    async fn materialize_session_nodes(&self, session_id: &str, template_id: &str) -> Result<bool>;
    /// Same as above but materializes nodes from a pack.
    async fn materialize_session_pack_nodes(&self, session_id: &str, pack_id: &str) -> Result<bool>;

    // --- override config ---
    async fn update_session_override_config(&self, session_id: &str, config: &serde_json::Value) -> Result<()>;
    /// Atomic merge sessions override local definitions `{local_definitions: {def_id: patch}}` (to prevent loss of updates caused by read-rewrite).
    async fn set_local_definition(&self, session_id: &str, def_id: &str, patch: &serde_json::Value) -> Result<()>;
    /// Atomically removes the local coverage of a given def (RFC7396: setting the value to null deletes the key).
    async fn clear_local_definition(&self, session_id: &str, def_id: &str) -> Result<()>;
    /// Fold a session's local override into the global `def` and clear that local
    /// patch in one transaction (the merge is the caller's; this persists both
    /// writes atomically so a promote can't half-apply).
    async fn promote_local_definition(&self, session_id: &str, def_id: &str, def: &Definition) -> Result<()>;
    /// Replaces session-local variable declarations (`override_config.local_variables`) in a single operation.
    async fn set_local_variables(&self, session_id: &str, variables: &serde_json::Value) -> Result<()>;
    /// Atomically replaces `override_config.agent`, preserving every sibling key.
    async fn set_session_agent_override(&self, session_id: &str, agent: &serde_json::Value) -> Result<()>;
    /// Atomically removes `override_config.agent`, preserving every sibling key.
    async fn clear_session_agent_override(&self, session_id: &str) -> Result<()>;

    // --- summaries (M6 rolling context summaries) ---
    async fn create_summary(&self, summary: &Summary) -> Result<()>;
    async fn list_summaries(&self, session_id: &str) -> Result<Vec<Summary>>;

    // --- settings ---
    async fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>>;
    async fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()>;
    /// Upsert many settings in a single transaction (all-or-nothing).
    async fn set_settings(&self, pairs: &[(String, serde_json::Value)]) -> Result<()>;
    async fn list_settings(&self) -> Result<Vec<(String, serde_json::Value)>>;
    async fn delete_setting(&self, key: &str) -> Result<()>;
    /// Delete several settings in one transaction.
    async fn delete_settings(&self, keys: &[String]) -> Result<()>;

    // --- packs ---
    async fn create_pack(&self, pack: &Pack) -> Result<()>;
    async fn get_pack(&self, id: &str) -> Result<Option<Pack>>;
    /// The pack with this exact name, if any — for import conflict checks (one
    /// lookup instead of loading every pack).
    async fn get_pack_by_name(&self, name: &str) -> Result<Option<Pack>>;
    async fn list_packs(&self) -> Result<Vec<Pack>>;
    async fn update_pack(&self, pack: &Pack) -> Result<()>;
    /// Definitions referenced by this pack's nodes that no other pack/template/
    /// session references — i.e. would become unreachable (or, for regex_rule
    /// defs, silently promoted to a global orphan rule — see
    /// `effective_regex_rules`) if the pack is deleted.
    async fn orphaned_definitions_for_pack(&self, pack_id: &str) -> Result<Vec<Definition>>;
    /// Delete a pack and its node tree (`owner_kind='pack'`). If `delete_orphans`
    /// is true, also delete the definitions reported by
    /// `orphaned_definitions_for_pack` (mirrors `delete_template`).
    async fn delete_pack(&self, id: &str, delete_orphans: bool) -> Result<()>;
    /// Atomically persist an imported pack bundle in a single transaction:
    /// new asset rows, the pack, its definitions, then its nodes (which MUST be
    /// pre-ordered parent-before-child). Any failure rolls the whole import back.
    async fn import_pack(
        &self,
        pack: &Pack,
        defs: &[Definition],
        nodes: &[PromptNode],
        assets: &[Asset],
    ) -> Result<()>;
    /// Atomically persist an imported template in a single transaction: the
    /// template row, its definitions, then its nodes (which MUST be
    /// pre-ordered parent-before-child). Any failure rolls the whole import
    /// back — mirrors `import_pack`'s atomicity for the template/preset import
    /// paths, which previously looped individual non-transactional inserts.
    async fn import_template(
        &self,
        template: &Template,
        defs: &[Definition],
        nodes: &[PromptNode],
    ) -> Result<()>;

    // --- def types (container type registry) ---
    /// Lists the container types (sorted in ascending order by sort).
    async fn list_container_types(&self) -> Result<Vec<DefType>>;
    async fn create_def_type(&self, ty: &DefType) -> Result<()>;
    async fn delete_def_type(&self, id: &str) -> Result<()>;

    // --- assets (named media library) ---
    /// Lists resources (sorted in descending order by `created_at`, with the most recent at the top); `kind` filters the library (avatar/background); `None` returns all.
    async fn list_assets(&self, kind: Option<&str>) -> Result<Vec<Asset>>;
    async fn get_asset(&self, id: &str) -> Result<Option<Asset>>;
    async fn create_asset(&self, asset: &Asset) -> Result<()>;
    /// Rename a resource (name only).
    async fn rename_asset(&self, id: &str, name: &str) -> Result<()>;
    async fn delete_asset(&self, id: &str) -> Result<()>;
    /// First asset whose content hash matches, if any (dedup lookup).
    async fn find_asset_by_hash(&self, hash: &str) -> Result<Option<Asset>>;
    /// The asset whose stored `path` matches, if any.
    async fn get_asset_by_path(&self, path: &str) -> Result<Option<Asset>>;
    /// Whether any pack identity, definition meta, or session still points at this
    /// avatar `path` — one EXISTS query instead of scanning those three tables.
    async fn is_avatar_referenced(&self, path: &str) -> Result<bool>;
    /// Set/replace an asset's content hash (used by the startup backfill).
    async fn set_asset_hash(&self, id: &str, hash: &str) -> Result<()>;

    // --- users / sessions (auth) ---
    async fn create_user(&self, user: &User) -> Result<()>;
    /// All users, oldest first. Used by the desktop shell to find the local
    /// account (and, later, a user-management UI).
    async fn list_users(&self) -> Result<Vec<User>>;
    async fn get_user_by_username(&self, username: &str) -> Result<Option<User>>;
    async fn get_user(&self, id: &str) -> Result<Option<User>>;
    /// Replace a user's password hash. Returns whether a row existed.
    async fn update_user_password(&self, id: &str, password_hash: &str) -> Result<bool>;
    async fn count_users(&self) -> Result<i64>;
    async fn create_auth_session(&self, session: &AuthSessionRecord) -> Result<()>;
    /// Look up a session by token. Caller checks `expires_at` (the row is
    /// returned even when expired so callers can distinguish "not found" from
    /// "expired" if they want; `require_session` treats both as 401).
    async fn get_auth_session(&self, token: &str) -> Result<Option<AuthSessionRecord>>;
    async fn delete_auth_session(&self, token: &str) -> Result<()>;
    /// Delete a user's sessions, optionally keeping one token alive (used by
    /// change-password to invalidate other sessions but keep the current one).
    async fn delete_sessions_for_user(
        &self,
        user_id: &str,
        except_token: Option<&str>,
    ) -> Result<()>;
    /// Delete all sessions whose `expires_at` has passed (piggybacked on login).
    async fn delete_expired_sessions(&self) -> Result<()>;
    /// The most recent non-expired session for a user, if any. Used by the
    /// desktop shell to reuse a live session across launches instead of minting
    /// a new one each time (and to fall through to "create" when the user logged
    /// out and the row is gone).
    async fn get_active_session_for_user(&self, user_id: &str) -> Result<Option<AuthSessionRecord>>;
}
