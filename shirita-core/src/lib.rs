//! shirita-core: 业务核心库（存储、模型、上下文工程……）

pub mod adapters;
pub mod assembly;
pub mod attachments;
pub mod budget;
pub mod config;
pub mod conversation;
pub mod error;
pub mod identity;
pub mod keyword;
pub mod model;
pub mod models;
pub mod pngcard;
pub mod portable;
pub mod seed;
pub mod state;
pub mod storage;
pub mod summarize;
pub mod tokenizer;
pub mod tree;

pub use assembly::{
    apply_regex_rules, assemble_from_nodes, build_chat_messages, is_valid_regex, render_vars,
    sanitize_tag, AssembledPlan, Placement, PromptSegment,
};
pub use budget::{over_threshold, trim_history};
pub use config::{apply_provider_env, Config};
pub use conversation::{regenerate, send_message, SendEvent};
pub use error::{Error, Result};
pub use model::{
    AnthropicProvider, ChatMessage, ChatRequest, EchoProvider, ModelProvider, OpenAiProvider,
};
pub use adapters::charcard::{charcard_to_loreset, LoreSet};
pub use adapters::preset::tree_to_preset;
pub use adapters::worldinfo::worldinfo_to_defs;
pub use models::asset::Asset;
pub use models::def_type::{is_prompt, is_reserved, DefType};
pub use models::definition::Definition;
pub use models::message::{Message, Role};
pub use models::prompt_node::{NodeKind, OwnerKind, PromptNode};
pub use models::session::Session;
pub use models::summary::Summary;
pub use models::template::Template;
pub use pngcard::read_card_json;
pub use portable::{
    export_definition, export_template, parse_portable, PortableDef, PortableDoc, PortableNode,
};
pub use seed::ensure_default_template;
pub use state::{
    apply_updates, effective_state, parse_state_updates, resolve_schema, schema_initials,
    strip_state_tags, system_variables, Update, VarDecl, VarType,
};
pub use storage::{sqlite::SqliteStorage, Storage};
pub use summarize::fold_range;
pub use summarize::run as run_summary;
pub use tokenizer::{tiktoken::TiktokenCounter, TokenCounter};
