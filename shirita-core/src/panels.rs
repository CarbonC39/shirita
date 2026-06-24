//! Panel resolution: gather `panel` folders from a session's effective trees
//! (template/session + mounted packs) into rendered html/css/caps payloads for
//! the chat UI. Panel bricks (`html`/`css`) are non-rendering — they never enter
//! the LLM prompt; this is the separate path that surfaces them to the UI.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::models::session::Session;
use crate::storage::Storage;

/// One renderable panel: a `panel` folder's combined html/css plus its caps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderedPanel {
    pub id: String,
    pub name: String,
    pub html: String,
    pub css: String,
    pub caps: Value,
}

/// Pure: collect `panel` folders from one node tree into RenderedPanels. A panel
/// folder = an enabled Folder tagged "panel"; its enabled `html`/`css` child refs
/// are `"\n"`-joined in tree order. Name = folder `meta.name`, else the first
/// html brick's name, else "Panel". Caps = folder `meta.caps` (or `{}`).
pub fn collect_panels(
    nodes: &[PromptNode],
    defs: &HashMap<String, Definition>,
) -> Vec<RenderedPanel> {
    let mut folders: Vec<&PromptNode> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Folder && n.enabled && n.tag.as_deref() == Some("panel"))
        .collect();
    folders.sort_by_key(|n| n.sort_order);

    let mut out = Vec::new();
    for folder in folders {
        let mut kids: Vec<&PromptNode> = nodes
            .iter()
            .filter(|n| {
                n.kind == NodeKind::Ref
                    && n.enabled
                    && n.parent_id.as_deref() == Some(folder.id.as_str())
            })
            .collect();
        kids.sort_by_key(|n| n.sort_order);

        let mut html_parts: Vec<String> = Vec::new();
        let mut css_parts: Vec<String> = Vec::new();
        let mut first_html_name: Option<String> = None;
        for k in &kids {
            let Some(def) = k.definition_id.as_deref().and_then(|id| defs.get(id)) else {
                continue;
            };
            match def.def_type.as_str() {
                "html" => {
                    if first_html_name.is_none() {
                        first_html_name = Some(def.name.clone());
                    }
                    html_parts.push(def.content.clone());
                }
                "css" => css_parts.push(def.content.clone()),
                _ => {}
            }
        }

        let name = folder
            .meta
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or(first_html_name)
            .unwrap_or_else(|| "Panel".to_string());
        let caps = folder.meta.get("caps").cloned().unwrap_or_else(|| json!({}));

        out.push(RenderedPanel {
            id: folder.id.clone(),
            name,
            html: html_parts.join("\n"),
            css: css_parts.join("\n"),
            caps,
        });
    }
    out
}

async fn load_defs(
    storage: &dyn Storage,
    nodes: &[PromptNode],
) -> crate::Result<HashMap<String, Definition>> {
    let mut defs = HashMap::new();
    for n in nodes {
        if let Some(did) = &n.definition_id {
            if !defs.contains_key(did) {
                if let Ok(Some(d)) = storage.get_definition(did).await {
                    defs.insert(did.clone(), d);
                }
            }
        }
    }
    Ok(defs)
}

/// Async: all panels for a session — effective template/session tree first, then
/// each mounted pack's tree (mount order).
pub async fn resolve_session_panels(
    storage: &dyn Storage,
    session: &Session,
) -> crate::Result<Vec<RenderedPanel>> {
    let mut out = Vec::new();

    let nodes = crate::conversation::effective_nodes(storage, session).await?;
    let defs = load_defs(storage, &nodes).await?;
    out.extend(collect_panels(&nodes, &defs));

    for pid in &session.mounted_packs {
        let pnodes = storage.list_nodes(&OwnerKind::Pack, pid).await?;
        let pdefs = load_defs(storage, &pnodes).await?;
        out.extend(collect_panels(&pnodes, &pdefs));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::prompt_node::OwnerKind;
    use serde_json::json;

    fn html_def(id: &str, name: &str, content: &str) -> Definition {
        let mut d = Definition::new("html", name, content);
        d.id = id.to_string();
        d
    }
    fn css_def(id: &str, content: &str) -> Definition {
        let mut d = Definition::new("css", "style", content);
        d.id = id.to_string();
        d
    }

    #[test]
    fn collect_joins_html_and_css_with_newline() {
        let mut folder = PromptNode::new_folder(OwnerKind::Pack, "p1", None, 0, "panel");
        folder.id = "F".into();
        folder.meta = json!({ "name": "Status", "caps": { "write": true } });
        let h1 = PromptNode::new_ref(OwnerKind::Pack, "p1", Some("F".into()), 0, "h1");
        let h2 = PromptNode::new_ref(OwnerKind::Pack, "p1", Some("F".into()), 1, "h2");
        let c1 = PromptNode::new_ref(OwnerKind::Pack, "p1", Some("F".into()), 2, "c1");
        let nodes = vec![folder, h1, h2, c1];
        let mut defs = HashMap::new();
        defs.insert("h1".into(), html_def("h1", "A", "<div id=\"a\"></div>"));
        defs.insert("h2".into(), html_def("h2", "B", "<div id=\"b\"></div>"));
        defs.insert("c1".into(), css_def("c1", ".a{}"));

        let panels = collect_panels(&nodes, &defs);
        assert_eq!(panels.len(), 1);
        assert_eq!(panels[0].name, "Status");
        assert_eq!(panels[0].html, "<div id=\"a\"></div>\n<div id=\"b\"></div>");
        assert_eq!(panels[0].css, ".a{}");
        assert_eq!(panels[0].caps, json!({ "write": true }));
    }

    #[test]
    fn collect_ignores_non_panel_and_disabled_folders() {
        let mut other = PromptNode::new_folder(OwnerKind::Pack, "p1", None, 0, "char");
        other.id = "O".into();
        let mut disabled = PromptNode::new_folder(OwnerKind::Pack, "p1", None, 1, "panel");
        disabled.id = "D".into();
        disabled.enabled = false;
        let nodes = vec![other, disabled];
        let defs = HashMap::new();
        assert!(collect_panels(&nodes, &defs).is_empty());
    }

    #[test]
    fn collect_name_falls_back_to_first_html_then_default() {
        let mut folder = PromptNode::new_folder(OwnerKind::Pack, "p1", None, 0, "panel");
        folder.id = "F".into();
        let h1 = PromptNode::new_ref(OwnerKind::Pack, "p1", Some("F".into()), 0, "h1");
        let nodes = vec![folder, h1];
        let mut defs = HashMap::new();
        defs.insert("h1".into(), html_def("h1", "Markup", "<b/>"));
        let panels = collect_panels(&nodes, &defs);
        assert_eq!(panels[0].name, "Markup");
        assert_eq!(panels[0].caps, json!({}));
    }
}
