//! Pack model: a content bundle — a node tree (owner=pack) plus an optional
//! identity and (via bound regex/var/html definitions) scoped behaviors.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PackIdentity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub identity: PackIdentity,
    #[serde(default)]
    pub meta: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

impl Pack {
    pub fn new(name: impl Into<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            identity: PackIdentity::default(),
            meta: serde_json::json!({}),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pack_has_uuid_empty_identity_and_meta() {
        let p = Pack::new("Alice");
        assert_eq!(p.name, "Alice");
        assert_eq!(p.id.len(), 36);
        assert_eq!(p.identity, PackIdentity::default());
        assert_eq!(p.meta, serde_json::json!({}));
        assert_eq!(p.created_at, p.updated_at);
    }
}
