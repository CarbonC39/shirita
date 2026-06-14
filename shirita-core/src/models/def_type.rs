//! def_type：可扩展「容器类型」注册表行 + 保留类型常量。

use serde::{Deserialize, Serialize};

/// 保留类型（代码常量，永不入 def_types 表，不进节点树容器）。
pub const RESERVED: [&str; 3] = ["prompt", "regex_rule", "tool"];

/// 是否保留类型（prompt / regex_rule / tool）。
pub fn is_reserved(t: &str) -> bool {
    RESERVED.contains(&t)
}

/// 是否根级裸文本的 prompt 类型。
pub fn is_prompt(t: &str) -> bool {
    t == "prompt"
}

/// 容器类型注册表的一行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefType {
    pub id: String,
    pub label: String,
    pub sort: i64,
    pub builtin: bool,
    pub created_at: String,
}

impl DefType {
    /// 新建一个用户自定义容器类型（builtin = false）。
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
    fn new_custom_is_not_builtin() {
        let t = DefType::new("faction", "Faction", 5);
        assert_eq!(t.id, "faction");
        assert!(!t.builtin);
        assert_eq!(t.created_at.len() > 0, true);
    }
}
