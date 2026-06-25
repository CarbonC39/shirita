//! def_type: Extensible “container type” registry entries + reserved type constants.

use serde::{Deserialize, Serialize};

/// Reserved types (code constants that are never added to the def_types table or included in the node tree container).。
pub const RESERVED: [&str; 8] =
    ["prompt", "regex_rule", "tool", "first_message", "protocol", "html", "css", "variables"];

/// Whether a type is reserved (prompt / regex_rule / tool).
pub fn is_reserved(t: &str) -> bool {
    RESERVED.contains(&t)
}

/// Whether the root-level raw text is of type `prompt`.
pub fn is_prompt(t: &str) -> bool {
    t == "prompt"
}

/// A row in the container type registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefType {
    pub id: String,
    pub label: String,
    pub sort: i64,
    pub builtin: bool,
    pub created_at: String,
}

impl DefType {
    /// Create a user-defined container type (builtin = false).
    pub fn new(id: impl Into<String>, label: impl Into<String>, sort: i64) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            sort,
            builtin: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_classification() {
        assert!(is_reserved("prompt"));
        assert!(is_reserved("regex_rule"));
        assert!(is_reserved("tool"));
        assert!(!is_reserved("char"));
        assert!(is_prompt("prompt"));
        assert!(!is_prompt("char"));
    }

    #[test]
    fn first_message_is_reserved() {
        assert!(is_reserved("first_message"));
        assert!(!is_prompt("first_message"));
    }

    #[test]
    fn protocol_is_reserved() {
        assert!(is_reserved("protocol"));
        assert!(!is_prompt("protocol"));
    }

    #[test]
    fn html_css_are_reserved() {
        assert!(is_reserved("html"));
        assert!(is_reserved("css"));
        assert!(!is_reserved("char"));
    }

    #[test]
    fn variables_is_reserved() {
        assert!(is_reserved("variables"));
    }

    #[test]
    fn new_custom_is_not_builtin() {
        let t = DefType::new("faction", "Faction", 5);
        assert_eq!(t.id, "faction");
        assert!(!t.builtin);
        assert_eq!(t.created_at.len() > 0, true);
    }
}
