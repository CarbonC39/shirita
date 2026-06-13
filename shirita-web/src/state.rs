use std::sync::Arc;

use shirita_core::{Config, ModelProvider, Storage, TokenCounter};

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage>,
    pub config: Arc<Config>,
    pub provider: Arc<dyn ModelProvider>,
    pub token_counter: Arc<dyn TokenCounter>,
    pub model: String,
}
