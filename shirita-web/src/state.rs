use std::sync::Arc;

use shirita_core::{Config, Storage};

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage>,
    pub config: Arc<Config>,
}
