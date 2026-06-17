use std::sync::Arc;

use shirita_core::{Config, ModelProvider, Storage, TokenCounter};

use crate::generations::Generations;

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage>,
    pub config: Arc<Config>,
    /// env 兜底 provider（settings 未配置时用）。运行期由 `resolve_provider` 决定实际 provider。
    pub provider: Arc<dyn ModelProvider>,
    pub token_counter: Arc<dyn TokenCounter>,
    pub model: String,
    pub generations: Arc<Generations>,
    /// 全进程共享的 HTTP 客户端（克隆即共享连接池），所有 provider 复用，杜绝 per-call Client::new()。
    pub http_client: reqwest::Client,
}
