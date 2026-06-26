use std::sync::Arc;

use shirita_core::{Config, ModelProvider, Storage, TokenCounter};

use crate::generations::Generations;

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage>,
    pub config: Arc<Config>,
    /// Keep the temporary directory alive for the duration of the test process to prevent it from being deleted during the connection.
    pub provider: Arc<dyn ModelProvider>,
    pub token_counter: Arc<dyn TokenCounter>,
    pub model: String,
    pub generations: Arc<Generations>,
    /// An HTTP client shared across all processes (cloning it shares the connection pool), reused by all providers, eliminating the need for `Client::new()` on every call.
    pub http_client: reqwest::Client,
}
