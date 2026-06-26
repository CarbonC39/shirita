//! Template tree ↔ ST preset JSON (prompt order + container/history tags).

use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, PromptNode};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct PresetItem {
    pub kind: String, // "container" | "ref" | "history"
    pub tag: Option<String>,
    pub name: Option<String>,
    pub content: Option<String>,
    pub def_type: Option<String>,
}

/// Serializes the template root-level tree into an ordered sequence of preset items (subitems within a container follow immediately after it, with depth determined by the parent).
pub fn tree_to_preset(nodes: &[PromptNode], defs: &HashMap<String, Definition>) -> serde_json::Value {
    // Group children by parent in one pass (O(N)) rather than re-scanning every
    // node for each folder (O(folders × N)).
    let mut by_parent: HashMap<&str, Vec<&PromptNode>> = HashMap::new();
    let mut roots: Vec<&PromptNode> = Vec::new();
    for n in nodes {
        match n.parent_id.as_deref() {
            Some(pid) => by_parent.entry(pid).or_default().push(n),
            None => roots.push(n),
        }
    }
    roots.sort_by_key(|n| n.sort_order);
    let mut items: Vec<serde_json::Value> = Vec::new();
    for r in roots {
        match r.kind {
            NodeKind::History => items.push(serde_json::json!({ "kind": "history" })),
            NodeKind::Content => items.push(serde_json::json!({ "kind": "content" })),
            NodeKind::Folder => {
                let tag = r.tag.clone().unwrap_or_default();
                let mut kids = by_parent.get(r.id.as_str()).cloned().unwrap_or_default();
                kids.sort_by_key(|n| n.sort_order);
                let children: Vec<serde_json::Value> = kids.iter().filter_map(|k| ref_item(k, defs)).collect();
                items.push(serde_json::json!({ "kind": "container", "tag": tag, "children": children }));
            }
            NodeKind::Ref => {
                if let Some(it) = ref_item(r, defs) { items.push(it); }
            }
        }
    }
    serde_json::json!({ "version": 1, "items": items })
}

fn ref_item(n: &PromptNode, defs: &HashMap<String, Definition>) -> Option<serde_json::Value> {
    let def_id = n.definition_id.as_ref()?; // structural nodes (folders) carry no def
    let Some(def) = defs.get(def_id) else {
        tracing::warn!(node_id = %n.id, %def_id, "tree_to_preset: ref points to a missing definition, omitting from export");
        return None;
    };
    Some(serde_json::json!({
        "kind": "ref", "name": def.name, "content": def.content,
        "def_type": def.def_type, "meta": def.meta,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::prompt_node::OwnerKind;

    #[test]
    fn serializes_container_then_history() {
        let neo = Definition::new("char", "Neo", "body");
        let cf = PromptNode::new_folder(OwnerKind::Template, "t", None, 0, "char");
        let cref = PromptNode::new_ref(OwnerKind::Template, "t", Some(cf.id.clone()), 0, &neo.id);
        let mut hist = PromptNode::new_folder(OwnerKind::Template, "t", None, 1, "history");
        hist.kind = NodeKind::History; hist.tag = None;

        let mut defs = HashMap::new();
        defs.insert(neo.id.clone(), neo.clone());
        let out = tree_to_preset(&[cf.clone(), cref, hist], &defs);
        let items = out["items"].as_array().unwrap();
        assert_eq!(items[0]["kind"], "container");
        assert_eq!(items[0]["tag"], "char");
        assert_eq!(items[0]["children"][0]["name"], "Neo");
        assert_eq!(items[1]["kind"], "history");
    }

    #[test]
    fn ref_to_missing_definition_is_omitted() {
        // A dangling ref (its def isn't in the map) is dropped from the export
        // rather than emitted as a half-populated item.
        let dangling = PromptNode::new_ref(OwnerKind::Template, "t", None, 0, "ghost-def-id");
        let out = tree_to_preset(&[dangling], &HashMap::new());
        assert!(out["items"].as_array().unwrap().is_empty());
    }
}
