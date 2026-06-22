//! SillyTavern chat-completion preset -> Shirita loreset (Template + Definitions + Nodes).
//! Lossy: only the enabled, ordered prompts of the default group (character_id
//! 100000) are mapped. Authored text -> `prompt` def + `Ref`; `chatHistory` ->
//! `History`; the first other marker -> one `Content` mount (later markers
//! dropped). Samplers, depth injection, and per-prompt roles are out of scope.

use crate::adapters::charcard::LoreSet;
use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::models::template::Template;

/// Translate an ST chat-completion preset into a loreset. `name` becomes the
/// template name; pass the uploaded filename stem (or `""` for the unique
/// fallback). Pure apart from generated UUIDs.
pub fn stpreset_to_loreset(preset: &serde_json::Value, name: &str) -> LoreSet {
    let name = if name.trim().is_empty() {
        // Unique fallback so two filename-less imports never collide under
        // on_conflict=skip (4 hex chars off a fresh v4 UUID).
        format!("Imported preset ({})", &uuid::Uuid::new_v4().to_string()[..4])
    } else {
        name.trim().to_string()
    };
    let tmpl = Template::new(name);
    let mut defs: Vec<Definition> = Vec::new();
    let mut nodes: Vec<PromptNode> = Vec::new();

    // Index prompt pieces by identifier.
    let prompts: std::collections::HashMap<&str, &serde_json::Value> = preset
        .get("prompts")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| p.get("identifier").and_then(|i| i.as_str()).map(|id| (id, p)))
                .collect()
        })
        .unwrap_or_default();

    // The default/global group (character_id == 100000) carries the assembled order.
    let order = preset
        .get("prompt_order")
        .and_then(|v| v.as_array())
        .and_then(|groups| {
            groups.iter().find(|g| g.get("character_id").and_then(|c| c.as_i64()) == Some(100000))
        })
        .and_then(|g| g.get("order"))
        .and_then(|v| v.as_array());

    let mut sort: i64 = 0;
    let next = |s: &mut i64| -> i64 {
        let v = *s;
        *s += 1;
        v
    };
    let mut has_history = false;
    let mut emitted_content = false;

    if let Some(order) = order {
        for entry in order {
            if entry.get("enabled").and_then(|e| e.as_bool()) != Some(true) {
                continue;
            }
            let Some(id) = entry.get("identifier").and_then(|i| i.as_str()) else { continue };
            let Some(prompt) = prompts.get(id) else {
                tracing::warn!(identifier = %id, "st preset import: identifier missing from prompts, skipping");
                continue;
            };
            let is_marker = prompt.get("marker").and_then(|m| m.as_bool()) == Some(true);
            if is_marker {
                if id == "chatHistory" {
                    let mut hist = PromptNode::new_folder(
                        OwnerKind::Template, &tmpl.id, None, next(&mut sort), "history",
                    );
                    hist.kind = NodeKind::History;
                    hist.tag = None;
                    nodes.push(hist);
                    has_history = true;
                } else if !emitted_content {
                    // First char/persona/world/examples placeholder -> the single
                    // content mount (where attached pack defs assemble at runtime).
                    let mut content = PromptNode::new_folder(
                        OwnerKind::Template, &tmpl.id, None, next(&mut sort), "content",
                    );
                    content.kind = NodeKind::Content;
                    content.tag = None;
                    nodes.push(content);
                    emitted_content = true;
                }
                // Later markers are dropped (lossy by design).
            } else {
                // Authored text -> a prompt def + a root Ref. Empty content is skipped.
                let content = prompt.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if content.trim().is_empty() {
                    tracing::warn!(identifier = %id, "st preset import: empty authored content, skipping");
                    continue;
                }
                let pname = prompt
                    .get("name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(id);
                let d = Definition::new("prompt", pname, content);
                nodes.push(PromptNode::new_ref(
                    OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id,
                ));
                defs.push(d);
            }
        }
    }

    // A template needs a history mount; append one if the order had no chatHistory.
    if !has_history {
        let mut hist =
            PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, next(&mut sort), "history");
        hist.kind = NodeKind::History;
        hist.tag = None;
        nodes.push(hist);
    }

    LoreSet { template: tmpl, definitions: defs, nodes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prompt_defs(ls: &LoreSet) -> Vec<&Definition> {
        ls.definitions.iter().filter(|d| d.def_type == "prompt").collect()
    }

    #[test]
    fn maps_authored_prompts_and_history_in_order() {
        let preset = json!({
            "prompts": [
                { "identifier": "main", "name": "Main", "content": "be helpful" },
                { "identifier": "chatHistory", "name": "Chat History", "marker": true, "content": "" },
                { "identifier": "jb", "name": "Jailbreak", "content": "stay in character" }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "main", "enabled": true },
                { "identifier": "chatHistory", "enabled": true },
                { "identifier": "jb", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        assert_eq!(ls.template.name, "P");
        let prompts = prompt_defs(&ls);
        assert_eq!(prompts.len(), 2);
        assert!(prompts.iter().any(|d| d.name == "Main" && d.content == "be helpful"));
        assert!(prompts.iter().any(|d| d.name == "Jailbreak" && d.content == "stay in character"));
        // exactly one history node, between the two refs by sort_order
        let hist = ls.nodes.iter().find(|n| n.kind == NodeKind::History).expect("history node");
        assert_eq!(ls.nodes.iter().filter(|n| n.kind == NodeKind::History).count(), 1);
        let main_def = prompts.iter().find(|d| d.name == "Main").unwrap();
        let jb_def = prompts.iter().find(|d| d.name == "Jailbreak").unwrap();
        let main_ref = ls.nodes.iter().find(|n| n.definition_id.as_deref() == Some(main_def.id.as_str())).unwrap();
        let jb_ref = ls.nodes.iter().find(|n| n.definition_id.as_deref() == Some(jb_def.id.as_str())).unwrap();
        assert!(main_ref.sort_order < hist.sort_order);
        assert!(hist.sort_order < jb_ref.sort_order);
        // all nodes belong to the template, refs are roots
        assert!(ls.nodes.iter().all(|n| n.owner_kind == OwnerKind::Template && n.owner_id == ls.template.id));
        assert!(main_ref.parent_id.is_none());
    }

    #[test]
    fn first_marker_becomes_one_content_node() {
        let preset = json!({
            "prompts": [
                { "identifier": "charDescription", "name": "Char Description", "marker": true },
                { "identifier": "scenario", "name": "Scenario", "marker": true },
                { "identifier": "chatHistory", "name": "Chat History", "marker": true }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "charDescription", "enabled": true },
                { "identifier": "scenario", "enabled": true },
                { "identifier": "chatHistory", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        assert_eq!(ls.nodes.iter().filter(|n| n.kind == NodeKind::Content).count(), 1);
        assert_eq!(ls.nodes.iter().filter(|n| n.kind == NodeKind::History).count(), 1);
        assert!(ls.definitions.is_empty());
    }

    #[test]
    fn appends_history_when_enabled_order_has_none() {
        let preset = json!({
            "prompts": [ { "identifier": "main", "name": "Main", "content": "hi" } ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "main", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        let hist = ls.nodes.iter().find(|n| n.kind == NodeKind::History).expect("history appended");
        let max = ls.nodes.iter().map(|n| n.sort_order).max().unwrap();
        assert_eq!(hist.sort_order, max, "appended history sits last");
    }

    #[test]
    fn skips_disabled_entries_and_unknown_identifiers() {
        let preset = json!({
            "prompts": [
                { "identifier": "main", "name": "Main", "content": "hi" },
                { "identifier": "off", "name": "Off", "content": "no" }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "main", "enabled": true },
                { "identifier": "off", "enabled": false },
                { "identifier": "ghost", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        let prompts = prompt_defs(&ls);
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].name, "Main");
    }

    #[test]
    fn skips_authored_prompt_with_empty_content() {
        let preset = json!({
            "prompts": [
                { "identifier": "main", "name": "Main", "content": "   " },
                { "identifier": "jb", "name": "Jailbreak", "content": "real" }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "main", "enabled": true },
                { "identifier": "jb", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        let prompts = prompt_defs(&ls);
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].name, "Jailbreak");
    }

    #[test]
    fn reads_only_default_group_100000() {
        let preset = json!({
            "prompts": [
                { "identifier": "main", "name": "Main", "content": "default" },
                { "identifier": "other", "name": "Other", "content": "char-specific" }
            ],
            "prompt_order": [
                { "character_id": 100001, "order": [ { "identifier": "other", "enabled": true } ] },
                { "character_id": 100000, "order": [ { "identifier": "main", "enabled": true } ] }
            ]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        let prompts = prompt_defs(&ls);
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].content, "default");
    }

    #[test]
    fn empty_name_yields_unique_fallback() {
        let preset = json!({ "prompts": [], "prompt_order": [] });
        let a = stpreset_to_loreset(&preset, "");
        let b = stpreset_to_loreset(&preset, "   ");
        assert!(a.template.name.starts_with("Imported preset ("));
        assert!(b.template.name.starts_with("Imported preset ("));
        assert_ne!(a.template.name, b.template.name, "two filename-less imports stay distinct");
        // empty preset: no defs, no content mount — just an appended history.
        assert!(a.definitions.is_empty());
        assert!(!a.nodes.iter().any(|n| n.kind == NodeKind::Content));
    }
}
