//! 模板树 ↔ 类 ST preset JSON（prompt 顺序 + 容器/历史标记）。

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

/// 把模板根级树序列化为有序 preset 项（容器内子项跟随其后，深度由 parent 决定）。
pub fn tree_to_preset(nodes: &[PromptNode], defs: &HashMap<String, Definition>) -> serde_json::Value {
    let mut roots: Vec<&PromptNode> = nodes.iter().filter(|n| n.parent_id.is_none()).collect();
    roots.sort_by_key(|n| n.sort_order);
    let mut items: Vec<serde_json::Value> = Vec::new();
    for r in roots {
        match r.kind {
            NodeKind::History => items.push(serde_json::json!({ "kind": "history" })),
            NodeKind::Folder => {
                let tag = r.tag.clone().unwrap_or_default();
                let mut kids: Vec<&PromptNode> =
                    nodes.iter().filter(|n| n.parent_id.as_deref() == Some(r.id.as_str())).collect();
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
    let def = n.definition_id.as_ref().and_then(|id| defs.get(id))?;
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
}
