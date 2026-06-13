//! Template 模型。
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub meta: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

impl Template {
    pub fn new(name: impl Into<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
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
    fn new_template_has_uuid_and_timestamps() {
        let t = Template::new("My Template");
        assert_eq!(t.name, "My Template");
        assert_eq!(t.id.len(), 36);
        assert_eq!(t.meta, serde_json::json!({}));
        assert!(!t.created_at.is_empty());
        assert_eq!(t.created_at, t.updated_at);
    }
}
