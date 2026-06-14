//! shirita-core: 业务核心库（存储、模型、上下文工程……）

pub mod adapters;
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
    apply_regex_rules, assemble_from_nodes, build_chat_messages, render_vars, AssembledPlan,
    Placement, PromptSegment,
};
pub use config::Config;
pub use conversation::{send_message, SendEvent};
pub use error::{Error, Result};
pub use model::{ChatMessage, ChatRequest, EchoProvider, ModelProvider, OpenAiProvider};
pub use adapters::charcard::{charcard_to_defs, def_to_charcard};
pub use adapters::preset::tree_to_preset;
pub use adapters::worldinfo::{defs_to_worldinfo, worldinfo_to_defs};
pub use models::def_type::{is_prompt, is_reserved, DefType};
pub use models::definition::Definition;
pub use models::message::{Message, Role};
pub use models::prompt_node::{NodeKind, OwnerKind, PromptNode};
pub use models::session::Session;
pub use models::template::Template;
pub use storage::{sqlite::SqliteStorage, Storage};
pub use tokenizer::{tiktoken::TiktokenCounter, TokenCounter};
