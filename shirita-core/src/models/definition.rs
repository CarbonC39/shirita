//! Definition 模型与类型标签。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionType {
    Char,
    Prompt,
    World,
    Item,
    Persona,
    RegexRule,
    Tool,
}

impl DefinitionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DefinitionType::Char => "char",
            DefinitionType::Prompt => "prompt",
            DefinitionType::World => "world",
            DefinitionType::Item => "item",
            DefinitionType::Persona => "persona",
            DefinitionType::RegexRule => "regex_rule",
            DefinitionType::Tool => "tool",
        }
    }

    pub fn from_db(s: &str) -> crate::Result<Self> {
        Ok(match s {
            "char" => DefinitionType::Char,
            "prompt" => DefinitionType::Prompt,
            "world" => DefinitionType::World,
            "item" => DefinitionType::Item,
            "persona" => DefinitionType::Persona,
            "regex_rule" => DefinitionType::RegexRule,
            "tool" => DefinitionType::Tool,
            other => return Err(crate::Error::InvalidDefinitionType(other.to_string())),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    pub id: String,
    #[serde(rename = "type")]
    pub def_type: DefinitionType,
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub meta: serde_json::Value,
}

impl Definition {
    pub fn new(
        def_type: DefinitionType,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            def_type,
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
    fn type_db_roundtrip() {
        for (variant, s) in [
            (DefinitionType::Char, "char"),
            (DefinitionType::Prompt, "prompt"),
            (DefinitionType::World, "world"),
            (DefinitionType::Item, "item"),
            (DefinitionType::Persona, "persona"),
            (DefinitionType::RegexRule, "regex_rule"),
            (DefinitionType::Tool, "tool"),
        ] {
            assert_eq!(variant.as_str(), s);
            assert_eq!(DefinitionType::from_db(s).unwrap(), variant);
        }
    }

    #[test]
    fn unknown_type_errors() {
        assert!(DefinitionType::from_db("nope").is_err());
    }

    #[test]
    fn new_definition_has_uuid_and_empty_meta() {
        let d = Definition::new(DefinitionType::Char, "Alice", "<char>...</char>");
        assert_eq!(d.def_type, DefinitionType::Char);
        assert_eq!(d.name, "Alice");
        assert_eq!(d.content, "<char>...</char>");
        assert_eq!(d.meta, serde_json::json!({}));
        assert_eq!(d.id.len(), 36, "uuid v4 string is 36 chars");
    }
}
