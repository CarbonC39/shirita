use async_trait::async_trait;

use crate::models::definition::Definition;
use crate::Result;

pub mod sqlite;

/// 存储抽象层。M0 仅覆盖 definitions；后续里程碑扩展 sessions/messages。
#[async_trait]
pub trait Storage: Send + Sync {
    async fn create_definition(&self, def: &Definition) -> Result<()>;
    async fn get_definition(&self, id: &str) -> Result<Option<Definition>>;
    async fn list_definitions(&self) -> Result<Vec<Definition>>;
    async fn update_definition(&self, def: &Definition) -> Result<()>;
    async fn delete_definition(&self, id: &str) -> Result<()>;
}
