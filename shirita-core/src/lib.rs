//! shirita-core: 业务核心库（存储、模型、上下文工程……）

pub mod config;
pub mod error;
pub mod models;
pub mod storage;

pub use config::Config;
pub use error::{Error, Result};
pub use models::definition::{Definition, DefinitionType};
pub use storage::{sqlite::SqliteStorage, Storage};
