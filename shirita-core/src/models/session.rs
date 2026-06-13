//! 会话模型。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    #[serde(default)]
    pub template_id: Option<String>,
    #[serde(default)]
    pub override_config: serde_json::Value,
    #[serde(default)]
    pub current_state: serde_json::Value,
    #[serde(default)]
    pub mounted_definitions: Vec<String>,
}

impl Session {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            avatar: None,
            template_id: None,
            override_config: serde_json::json!({}),
            current_state: serde_json::json!({}),
            mounted_definitions: Vec::new(),
        }
    }
}
