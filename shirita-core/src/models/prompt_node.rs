//! PromptNode: part of the template/session node tree.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Folder,
    Ref,
    History,
    Content,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Folder => "folder",
            NodeKind::Ref => "ref",
            NodeKind::History => "history",
            NodeKind::Content => "content",
        }
    }

    pub fn from_db(s: &str) -> crate::Result<Self> {
        Ok(match s {
            "folder" => NodeKind::Folder,
            "ref" => NodeKind::Ref,
            "history" => NodeKind::History,
            "content" => NodeKind::Content,
            other => return Err(crate::Error::InvalidDefinitionType(other.to_string())),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerKind {
    Template,
    Session,
    Pack,
}

impl OwnerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            OwnerKind::Template => "template",
            OwnerKind::Session => "session",
            OwnerKind::Pack => "pack",
        }
    }

    pub fn from_db(s: &str) -> crate::Result<Self> {
        Ok(match s {
            "template" => OwnerKind::Template,
            "session" => OwnerKind::Session,
            "pack" => OwnerKind::Pack,
            other => return Err(crate::Error::InvalidDefinitionType(other.to_string())),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptNode {
    pub id: String,
    pub owner_kind: OwnerKind,
    pub owner_id: String,
    pub parent_id: Option<String>,
    pub sort_order: i64,
    pub kind: NodeKind,
    pub tag: Option<String>,
    pub definition_id: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    /// Per-use override patch for this node's referenced definition (e.g.
    /// `{"wrap_in_tag": true}`), mirroring the session-level
    /// `local_definitions` override pattern but scoped to this one template
    /// placement instead of every place the definition is used.
    #[serde(default)]
    pub meta: serde_json::Value,
}

impl PromptNode {
    pub fn new_folder(
        owner_kind: OwnerKind,
        owner_id: impl Into<String>,
        parent_id: Option<String>,
        sort_order: i64,
        tag: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            owner_kind,
            owner_id: owner_id.into(),
            parent_id,
            sort_order,
            kind: NodeKind::Folder,
            tag: Some(tag.into()),
            definition_id: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
            meta: serde_json::json!({}),
        }
    }

    pub fn new_ref(
        owner_kind: OwnerKind,
        owner_id: impl Into<String>,
        parent_id: Option<String>,
        sort_order: i64,
        definition_id: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            owner_kind,
            owner_id: owner_id.into(),
            parent_id,
            sort_order,
            kind: NodeKind::Ref,
            tag: None,
            definition_id: Some(definition_id.into()),
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
            meta: serde_json::json!({}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_folder_node() {
        let n = PromptNode::new_folder(OwnerKind::Template, "t1", None, 0, "char");
        assert_eq!(n.kind, NodeKind::Folder);
        assert_eq!(n.tag.as_deref(), Some("char"));
        assert!(n.definition_id.is_none());
        assert!(n.enabled);
        assert_eq!(n.id.len(), 36);
    }

    #[test]
    fn new_ref_node() {
        let n = PromptNode::new_ref(OwnerKind::Session, "s1", Some("parent".into()), 1, "def-1");
        assert_eq!(n.kind, NodeKind::Ref);
        assert_eq!(n.definition_id.as_deref(), Some("def-1"));
        assert!(n.tag.is_none());
    }

    #[test]
    fn node_kind_roundtrip() {
        assert_eq!(NodeKind::Folder.as_str(), "folder");
        assert_eq!(NodeKind::Ref.as_str(), "ref");
        assert_eq!(NodeKind::from_db("folder").unwrap(), NodeKind::Folder);
        assert_eq!(NodeKind::from_db("ref").unwrap(), NodeKind::Ref);
        assert!(NodeKind::from_db("nope").is_err());
    }

    #[test]
    fn owner_kind_roundtrip() {
        assert_eq!(OwnerKind::Template.as_str(), "template");
        assert_eq!(OwnerKind::Session.as_str(), "session");
    }

    #[test]
    fn node_kind_history_roundtrip() {
        assert_eq!(NodeKind::History.as_str(), "history");
        assert_eq!(NodeKind::from_db("history").unwrap(), NodeKind::History);
    }

    #[test]
    fn content_kind_roundtrip() {
        assert_eq!(NodeKind::Content.as_str(), "content");
        assert_eq!(NodeKind::from_db("content").unwrap(), NodeKind::Content);
    }

    #[test]
    fn owner_kind_pack_roundtrip() {
        assert_eq!(OwnerKind::Pack.as_str(), "pack");
        assert_eq!(OwnerKind::from_db("pack").unwrap(), OwnerKind::Pack);
    }
}
