use async_trait::async_trait;

use crate::models::definition::Definition;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::Result;

pub mod sqlite;

/// 存储抽象层。M0 覆盖 definitions；M1 起扩展 sessions/messages。
#[async_trait]
pub trait Storage: Send + Sync {
    async fn create_definition(&self, def: &Definition) -> Result<()>;
    async fn get_definition(&self, id: &str) -> Result<Option<Definition>>;
    async fn list_definitions(&self) -> Result<Vec<Definition>>;
    async fn update_definition(&self, def: &Definition) -> Result<()>;
    async fn delete_definition(&self, id: &str) -> Result<()>;

    // --- sessions ---
    async fn create_session(&self, session: &Session) -> Result<()>;
    async fn get_session(&self, id: &str) -> Result<Option<Session>>;
    async fn list_sessions(&self) -> Result<Vec<Session>>;
    /// 整体替换会话的挂载定义 ID 列表。
    async fn set_mounted_definitions(&self, session_id: &str, ids: &[String]) -> Result<()>;

    // --- messages ---
    async fn create_message(&self, message: &Message) -> Result<()>;
    /// 按 created_at（再以 id 为 tiebreak）升序返回某会话的全部消息。
    async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>>;
}
