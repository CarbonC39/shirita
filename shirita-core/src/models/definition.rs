//! Definition 模型：type 为可扩展字符串（见 models::def_type）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    pub id: String,
    #[serde(rename = "type")]
    pub def_type: String,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub meta: serde_json::Value,
}

impl Definition {
    pub fn new(
        def_type: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            def_type: def_type.into(),
            name: name.into(),
            content: content.into(),
            meta: serde_json::json!({}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_definition_has_uuid_and_empty_meta() {
        let d = Definition::new("char", "Alice", "<char>...</char>");
        assert_eq!(d.def_type, "char");
        assert_eq!(d.name, "Alice");
        assert_eq!(d.meta, serde_json::json!({}));
        assert_eq!(d.id.len(), 36, "uuid v4 string is 36 chars");
    }
}
