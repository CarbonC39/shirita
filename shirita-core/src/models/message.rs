//! 消息模型与角色标签。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }

    pub fn from_db(s: &str) -> crate::Result<Self> {
        Ok(match s {
            "system" => Role::System,
            "user" => Role::User,
            "assistant" => Role::Assistant,
            other => return Err(crate::Error::InvalidDefinitionType(other.to_string())),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub role: Role,
    pub raw_content: String,
    pub display_content: Option<String>,
    pub is_hidden: bool,
    /// Synthetic anchoring user turn: kept in the prompt, omitted from the UI
    /// (opposite of `is_hidden`, which drops the message from the prompt but
    /// still shows it dimmed in the UI). Used to seed a leading user turn before
    /// an assistant first message so generation isn't assistant-first (API 400).
    #[serde(default)]
    pub is_anchor: bool,
    /// Asset ids attached to this message (currently images), uploaded ahead of
    /// send via `POST /api/assets`. Resolved to provider-native image content
    /// at request-build time (see `attachments::resolve_images`); never inlined here.
    #[serde(default)]
    pub attachments: Vec<String>,
    #[serde(default)]
    pub snapshot_state: serde_json::Value,
    pub created_at: String,
}

impl Message {
    pub fn new(
        session_id: impl Into<String>,
        parent_id: Option<String>,
        role: Role,
        raw_content: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            parent_id,
            role,
            raw_content: raw_content.into(),
            display_content: None,
            is_hidden: false,
            is_anchor: false,
            attachments: Vec::new(),
            snapshot_state: serde_json::json!({}),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_db_roundtrip() {
        for (variant, s) in [
            (Role::System, "system"),
            (Role::User, "user"),
            (Role::Assistant, "assistant"),
        ] {
            assert_eq!(variant.as_str(), s);
            assert_eq!(Role::from_db(s).unwrap(), variant);
        }
        assert!(Role::from_db("nope").is_err());
    }

    #[test]
    fn new_message_defaults() {
        let m = Message::new("sess-1", Some("parent-1".into()), Role::User, "hi");
        assert_eq!(m.session_id, "sess-1");
        assert_eq!(m.parent_id.as_deref(), Some("parent-1"));
        assert_eq!(m.role, Role::User);
        assert_eq!(m.raw_content, "hi");
        assert_eq!(m.display_content, None);
        assert!(!m.is_hidden);
        assert_eq!(m.snapshot_state, serde_json::json!({}));
        assert_eq!(m.id.len(), 36);
        assert!(!m.created_at.is_empty());
    }
}
