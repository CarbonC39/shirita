//! shirita-core: 业务核心库（存储、模型、上下文工程……）

pub mod assembly;
pub mod config;
pub mod conversation;
pub mod error;
pub mod keyword;
pub mod model;
pub mod models;
pub mod storage;
pub mod tokenizer;

pub use assembly::{
    apply_regex_rules, assemble_from_nodes, assemble_system_prompt, render_vars, AssembledPlan,
    Placement, PromptSegment,
};
pub use config::Config;
pub use conversation::{send_message, SendEvent};
pub use error::{Error, Result};
pub use model::{ChatMessage, ChatRequest, EchoProvider, ModelProvider, OpenAiProvider};
pub use models::definition::{Definition, DefinitionType};
pub use models::message::{Message, Role};
pub use models::prompt_node::{NodeKind, OwnerKind, PromptNode};
pub use models::session::Session;
pub use models::template::Template;
pub use storage::{sqlite::SqliteStorage, Storage};
pub use tokenizer::{tiktoken::TiktokenCounter, TokenCounter};
