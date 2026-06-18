use async_trait::async_trait;

use std::collections::HashMap;

use crate::models::asset::Asset;
use crate::models::def_type::DefType;
use crate::models::definition::Definition;
use crate::models::message::Message;
use crate::models::prompt_node::{OwnerKind, PromptNode};
use crate::models::session::Session;
use crate::models::summary::Summary;
use crate::models::template::Template;
use crate::Result;

pub mod sqlite;

/// 存储抽象层。M0 覆盖 definitions；M1 起扩展 sessions/messages。
#[async_trait]
pub trait Storage: Send + Sync {
    async fn create_definition(&self, def: &Definition) -> Result<()>;
    async fn get_definition(&self, id: &str) -> Result<Option<Definition>>;
    async fn list_definitions(&self) -> Result<Vec<Definition>>;
    /// Distinct `definition_id`s referenced by any prompt node (all owners).
    /// Lets callers tell orphan ("global") defs from tree-mounted ones.
    async fn referenced_definition_ids(&self) -> Result<Vec<String>>;
    async fn update_definition(&self, def: &Definition) -> Result<()>;
    async fn delete_definition(&self, id: &str) -> Result<()>;

    // --- sessions ---
    async fn create_session(&self, session: &Session) -> Result<()>;
    async fn get_session(&self, id: &str) -> Result<Option<Session>>;
    async fn list_sessions(&self) -> Result<Vec<Session>>;
    /// 删除会话及其消息与自有节点树。
    async fn delete_session(&self, id: &str) -> Result<()>;
    /// 整体替换会话的挂载定义 ID 列表。
    async fn set_mounted_definitions(&self, session_id: &str, ids: &[String]) -> Result<()>;
    /// Update a session's editable profile (title + avatar).
    async fn update_session_profile(&self, session_id: &str, name: &str, avatar: Option<&str>) -> Result<()>;
    /// 按给定顺序（首项置顶）持久化会话的手动排序。
    async fn reorder_sessions(&self, ordered_ids: &[String]) -> Result<()>;
    /// Set (or clear with `None`) the session's active branch leaf.
    async fn set_session_active_leaf(&self, session_id: &str, leaf_id: Option<&str>) -> Result<()>;

    // --- messages ---
    async fn create_message(&self, message: &Message) -> Result<()>;
    /// 按 created_at（再以 id 为 tiebreak）升序返回某会话的全部消息。
    async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>>;
    async fn get_message(&self, id: &str) -> Result<Option<Message>>;
    /// Update an existing message's editable fields (raw/display content, hidden).
    async fn update_message(&self, message: &Message) -> Result<()>;

    // --- templates ---
    async fn create_template(&self, template: &Template) -> Result<()>;
    async fn get_template(&self, id: &str) -> Result<Option<Template>>;
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

    // --- override config ---
    async fn update_session_override_config(&self, session_id: &str, config: &serde_json::Value) -> Result<()>;
    /// 原子合并会话局部定义覆盖 `{local_definitions: {def_id: patch}}`（消除读改写丢更新）。
    async fn set_local_definition(&self, session_id: &str, def_id: &str, patch: &serde_json::Value) -> Result<()>;
    /// 原子清除某 def 的局部覆盖（RFC7396：值置 null 即删键）。
    async fn clear_local_definition(&self, session_id: &str, def_id: &str) -> Result<()>;
    /// 原子整列替换会话局部变量声明（`override_config.local_variables`）。
    async fn set_local_variables(&self, session_id: &str, variables: &serde_json::Value) -> Result<()>;

    // --- summaries (M6 rolling context summaries) ---
    async fn create_summary(&self, summary: &Summary) -> Result<()>;
    async fn list_summaries(&self, session_id: &str) -> Result<Vec<Summary>>;

    // --- settings ---
    async fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>>;
    async fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()>;
    async fn list_settings(&self) -> Result<Vec<(String, serde_json::Value)>>;
    async fn delete_setting(&self, key: &str) -> Result<()>;

    // --- def types (container type registry) ---
    /// 列出容器类型（按 sort 升序）。
    async fn list_container_types(&self) -> Result<Vec<DefType>>;
    async fn create_def_type(&self, ty: &DefType) -> Result<()>;
    async fn delete_def_type(&self, id: &str) -> Result<()>;

    // --- assets (named media library) ---
    /// 列出资源（按 created_at 降序，最新在前）。
    async fn list_assets(&self) -> Result<Vec<Asset>>;
    async fn get_asset(&self, id: &str) -> Result<Option<Asset>>;
    async fn create_asset(&self, asset: &Asset) -> Result<()>;
    /// 重命名资源（仅 name）。
    async fn rename_asset(&self, id: &str, name: &str) -> Result<()>;
    async fn delete_asset(&self, id: &str) -> Result<()>;
}
