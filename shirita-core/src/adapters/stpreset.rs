//! SillyTavern chat-completion preset -> Shirita loreset (Template + Definitions + Nodes).
//! Lossy: only the enabled, ordered prompts of the default group (character_id
//! 100000) are mapped. Authored text -> `prompt` def + `Ref`; `chatHistory` ->
//! `History`; the first other marker -> one `Content` mount (later markers
//! dropped). Samplers, depth injection, and per-prompt roles are out of scope.

use crate::adapters::charcard::LoreSet;
use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::models::template::Template;
use crate::state::{VarDecl, VarType};

/// Type-infer an ST setvar value: f64-parseable -> Number, bool-parseable ->
/// Bool, else String. Uses the standard parses (no hand-rolled numeric regex).
fn infer_var(value: &str) -> (VarType, serde_json::Value) {
    // Only treat as Number when the value is in canonical numeric form, so a
    // numeric-looking string the author meant as text ("007", "1.0", "1e3", an
    // ID/version) keeps its string form instead of being silently renumbered.
    if let Ok(n) = value.parse::<f64>() {
        if format!("{n}") == value {
            return (VarType::Number, serde_json::json!(n));
        }
    }
    if let Ok(b) = value.parse::<bool>() {
        return (VarType::Bool, serde_json::json!(b));
    }
    (VarType::String, serde_json::json!(value))
}

/// Strip `{{setvar::name::value}}` macros (collecting them as VarDecls in
/// first-seen order) and rewrite `{{getvar::name}}` -> `{{name}}`. Any other
/// `{{...}}` (e.g. `{{trim}}`, `{{// comment}}`, `{{user}}`) is kept literal.
/// Linear scan; nested macros inside a value are not parsed (first `}}` wins).
fn extract_variables(content: &str) -> (String, Vec<VarDecl>) {
    let mut out = String::with_capacity(content.len());
    let mut decls: Vec<VarDecl> = Vec::new();
    let mut rest = content;
    while let Some(pos) = rest.find("{{") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[pos..]); // unterminated -> keep literal, stop
            return (out, decls);
        };
        let inner = &after[..end];
        let literal = &rest[pos..pos + 2 + end + 2]; // the whole {{...}}
        if let Some(body) = inner.strip_prefix("setvar::") {
            if let Some((name, value)) = body.split_once("::") {
                let name = name.trim();
                if !name.is_empty() {
                    let (var_type, initial) = infer_var(value);
                    decls.push(VarDecl { name: name.to_string(), var_type, initial, scope: None });
                }
                // strip entirely (emit nothing)
            } else {
                out.push_str(literal); // malformed setvar -> keep literal
            }
        } else if let Some(name) = inner.strip_prefix("getvar::") {
            out.push_str("{{");
            out.push_str(name.trim());
            out.push_str("}}");
        } else {
            out.push_str(literal); // not a var macro -> keep literal
        }
        rest = &rest[pos + 2 + end + 2..];
    }
    out.push_str(rest);
    (out, decls)
}

/// (is_close, name) for each XML-ish tag. The name is the first token after
/// `<`/`</`; attributes are ignored; self-closing `<x/>` is skipped.
fn scan_tags(s: &str) -> Vec<(bool, String)> {
    static TAG_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"<\s*(/?)\s*([^\s<>/]+)[^<>]*?(/?)\s*>").unwrap());
    TAG_RE
        .captures_iter(s)
        .filter(|c| &c[3] != "/") // skip self-closing
        .map(|c| (&c[1] == "/", c[2].to_string()))
        .collect()
}

/// Net open count per tag name (open +1, close -1); balanced entries removed.
fn tag_balance(s: &str) -> std::collections::HashMap<String, i32> {
    let mut bal: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    for (is_close, name) in scan_tags(s) {
        *bal.entry(name).or_insert(0) += if is_close { -1 } else { 1 };
    }
    bal.retain(|_, v| *v != 0);
    bal
}

fn is_balanced(s: &str) -> bool {
    tag_balance(s).is_empty()
}

/// First cross-node span in a contiguous slice: the earliest element that
/// leaves some tag T net-open, paired with the earliest later element that
/// leaves T net-closed. Returns (opener_idx, closer_idx, tag).
///
/// Balances are computed once per element (not once per *pair*, which made
/// the naive version O(n^2) regex scans — a slow import on a preset with a
/// long run of prompts), and the lookup for "first closer after i" uses a
/// per-tag sorted index instead of rescanning the tail of the slice.
fn find_first_span(contents: &[String]) -> Option<(usize, usize, String)> {
    let balances: Vec<std::collections::HashMap<String, i32>> = contents.iter().map(|c| tag_balance(c)).collect();

    let mut closes_by_tag: std::collections::HashMap<&str, Vec<usize>> = std::collections::HashMap::new();
    for (idx, bal) in balances.iter().enumerate() {
        for (tag, v) in bal {
            if *v < 0 {
                closes_by_tag.entry(tag.as_str()).or_default().push(idx);
            }
        }
    }

    for (i, bal) in balances.iter().enumerate() {
        let Some(tag) = bal.iter().find(|(_, v)| **v > 0).map(|(t, _)| t.clone()) else {
            continue;
        };
        let Some(closers) = closes_by_tag.get(tag.as_str()) else { continue };
        // `closers` was built by ascending idx, so it's already sorted.
        let pos = closers.partition_point(|&x| x <= i);
        if let Some(&j) = closers.get(pos) {
            return Some((i, j, tag));
        }
    }
    None
}

/// Remove the first `<tag …>` (any attributes) and trim.
fn strip_open_tag(content: &str, tag: &str) -> String {
    let re = regex::Regex::new(&format!(r"<\s*{}(?:\s[^<>]*)?\s*>", regex::escape(tag))).unwrap();
    re.replacen(content, 1, "").trim().to_string()
}

/// Remove the first `</tag>` and trim.
fn strip_close_tag(content: &str, tag: &str) -> String {
    let re = regex::Regex::new(&format!(r"</\s*{}\s*>", regex::escape(tag))).unwrap();
    re.replacen(content, 1, "").trim().to_string()
}

pub fn stpreset_to_loreset(preset: &serde_json::Value, name: &str) -> LoreSet {
    use std::collections::{HashMap, HashSet};

    let tname = if name.trim().is_empty() {
        format!("Imported preset ({})", &uuid::Uuid::new_v4().to_string()[..4])
    } else {
        name.trim().to_string()
    };
    let tmpl = Template::new(tname);
    let mut defs: Vec<Definition> = Vec::new();
    let mut nodes: Vec<PromptNode> = Vec::new();

    // Index prompts by identifier, and keep their array order for the library tail.
    let prompts: HashMap<String, &serde_json::Value> = preset
        .get("prompts")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| p.get("identifier").and_then(|i| i.as_str()).map(|id| (id.to_string(), p)))
                .collect()
        })
        .unwrap_or_default();
    let all_ids: Vec<String> = preset
        .get("prompts")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|p| p.get("identifier").and_then(|i| i.as_str()).map(String::from)).collect())
        .unwrap_or_default();

    // Active order: pick the prompt_order group with the most entries (the
    // user-curated, character-specific order), not hardcoded character_id
    // 100000 — that id is ST's empty default skeleton and is frequently a
    // strict subset of the real order saved under another character_id.
    // Ties (including the common single-group case) favor 100000.
    let order: Vec<(String, bool)> = preset
        .get("prompt_order")
        .and_then(|v| v.as_array())
        .and_then(|gs| {
            gs.iter().max_by_key(|g| {
                let len = g.get("order").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
                let is_default = g.get("character_id").and_then(|c| c.as_i64()) == Some(100000);
                (len, is_default)
            })
        })
        .and_then(|g| g.get("order"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let id = e.get("identifier")?.as_str()?.to_string();
                    let en = e.get("enabled").and_then(|b| b.as_bool()).unwrap_or(false);
                    Some((id, en))
                })
                .collect()
        })
        .unwrap_or_default();
    let order_ids: HashSet<&str> = order.iter().map(|(id, _)| id.as_str()).collect();

    let is_marker = |id: &str| {
        prompts.get(id).and_then(|p| p.get("marker")).and_then(|m| m.as_bool()) == Some(true)
    };
    let name_of = |id: &str| {
        prompts
            .get(id)
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(id)
            .to_string()
    };

    // --- Part C: variables (global, first-wins) + cleaned-content cache. ---
    // Walk active order first, then library ids (array order), so order values win.
    let lib_ids: Vec<String> =
        all_ids.iter().filter(|id| !order_ids.contains(id.as_str())).cloned().collect();
    let mut vars: Vec<VarDecl> = Vec::new();
    let mut cleaned: HashMap<String, String> = HashMap::new();
    for id in order.iter().map(|(id, _)| id.clone()).chain(lib_ids.iter().cloned()) {
        if cleaned.contains_key(&id) || !prompts.contains_key(&id) || is_marker(&id) {
            continue;
        }
        let raw = prompts.get(&id).and_then(|p| p.get("content")).and_then(|v| v.as_str()).unwrap_or("");
        let (clean, decls) = extract_variables(raw);
        for d in decls {
            if !vars.iter().any(|e| e.name == d.name) {
                vars.push(d);
            }
        }
        cleaned.insert(id, clean);
    }

    // --- Phase 1: build ordered elements for the active order. ---
    struct Authored {
        name: String,
        content: String,
        enabled: bool,
    }
    enum Elem {
        Content,
        History,
        Prompt(usize),
    }
    let mut authored: Vec<Authored> = Vec::new();
    let mut elems: Vec<Elem> = Vec::new();
    let mut emitted_content = false;
    let mut has_history = false;
    for (id, enabled) in &order {
        if !prompts.contains_key(id) {
            tracing::warn!(identifier = %id, "st preset import: identifier missing from prompts, skipping");
            continue;
        }
        if is_marker(id) {
            if id == "chatHistory" {
                elems.push(Elem::History);
                has_history = true;
            } else if !emitted_content {
                elems.push(Elem::Content);
                emitted_content = true;
            }
            continue;
        }
        let content = cleaned.get(id).cloned().unwrap_or_default();
        if content.trim().is_empty() {
            continue; // originally empty or emptied by setvar-stripping
        }
        authored.push(Authored { name: name_of(id), content, enabled: *enabled });
        elems.push(Elem::Prompt(authored.len() - 1));
    }

    // --- Phase 2: detect cross-node spans within maximal runs of consecutive
    //     ENABLED authored elements (a marker or disabled prompt breaks a run). ---
    let mut spans: Vec<(usize, usize, String)> = Vec::new(); // (start_elem, end_elem, tag)
    let mut e = 0;
    while e < elems.len() {
        let run_enabled = matches!(elems[e], Elem::Prompt(k) if authored[k].enabled);
        if !run_enabled {
            e += 1;
            continue;
        }
        let start = e;
        while e + 1 < elems.len() && matches!(elems[e + 1], Elem::Prompt(k) if authored[k].enabled) {
            e += 1;
        }
        let end = e; // run is elems[start..=end]
        let contents: Vec<String> = (start..=end)
            .map(|x| match elems[x] {
                Elem::Prompt(k) => authored[k].content.clone(),
                _ => unreachable!(),
            })
            .collect();
        if let Some((ri, rj, tag)) = find_first_span(&contents) {
            spans.push((start + ri, start + rj, tag));
        }
        e += 1;
    }

    // --- Phase 3: emit nodes from elements, applying spans + wrap_in_tag. ---
    let folder_tag = |tag: &str| {
        let s = crate::assembly::sanitize_tag(tag);
        if s.is_empty() {
            "prompt".to_string()
        } else {
            s
        }
    };
    let mut root_sort: i64 = 0;
    let mut x = 0;
    while x < elems.len() {
        if let Some((s, t, tag)) = spans.iter().find(|(s, _, _)| *s == x).cloned() {
            let folder = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, root_sort, folder_tag(&tag));
            root_sort += 1;
            let fid = folder.id.clone();
            nodes.push(folder);
            let mut child_sort: i64 = 0;
            for ei in s..=t {
                let k = match elems[ei] {
                    Elem::Prompt(k) => k,
                    _ => unreachable!(),
                };
                let mut content = authored[k].content.clone();
                if ei == s {
                    content = strip_open_tag(&content, &tag);
                }
                if ei == t {
                    content = strip_close_tag(&content, &tag);
                }
                let d = Definition::new("prompt", &authored[k].name, content);
                // children are not individually wrapped — the folder emits <tag>…</tag>
                nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, Some(fid.clone()), child_sort, &d.id));
                child_sort += 1;
                defs.push(d);
            }
            x = t + 1;
            continue;
        }
        match &elems[x] {
            Elem::Content => {
                let mut c = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, root_sort, "content");
                c.kind = NodeKind::Content;
                c.tag = None;
                nodes.push(c);
                root_sort += 1;
            }
            Elem::History => {
                let mut h = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, root_sort, "history");
                h.kind = NodeKind::History;
                h.tag = None;
                nodes.push(h);
                root_sort += 1;
            }
            Elem::Prompt(k) => {
                let a = &authored[*k];
                let mut d = Definition::new("prompt", &a.name, &a.content);
                if is_balanced(&a.content) {
                    d.meta = serde_json::json!({ "wrap_in_tag": true });
                }
                let mut r = PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, root_sort, &d.id);
                r.enabled = a.enabled;
                nodes.push(r);
                defs.push(d);
                root_sort += 1;
            }
        }
        x += 1;
    }

    // A template needs a history mount.
    if !has_history {
        let mut hist = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, root_sort, "history");
        hist.kind = NodeKind::History;
        hist.tag = None;
        nodes.push(hist);
        root_sort += 1;
    }

    // --- Part A tail: not-in-order authored prompts -> one disabled `inactive`
    //     folder. Reuses the same cross-entry tag-span detection as the active
    //     order (Phase 2/3) so a multi-part tag spanning two prompts isn't
    //     flattened into two broken, unwrapped halves. ---
    let inactive: Vec<(String, String)> = lib_ids
        .iter()
        .filter(|id| !is_marker(id))
        .filter_map(|id| {
            let c = cleaned.get(id).cloned().unwrap_or_default();
            if c.trim().is_empty() {
                None
            } else {
                Some((name_of(id), c))
            }
        })
        .collect();
    if !inactive.is_empty() {
        let mut folder = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, root_sort, "inactive");
        folder.enabled = false;
        let fid = folder.id.clone();
        nodes.push(folder);
        let contents: Vec<String> = inactive.iter().map(|(_, c)| c.clone()).collect();
        let span = find_first_span(&contents);
        let mut cs: i64 = 0;
        let mut i = 0;
        while i < inactive.len() {
            if let Some((s, t, tag)) = &span {
                if i == *s {
                    let mut sub =
                        PromptNode::new_folder(OwnerKind::Template, &tmpl.id, Some(fid.clone()), cs, folder_tag(tag));
                    sub.enabled = false;
                    cs += 1;
                    let sid = sub.id.clone();
                    nodes.push(sub);
                    let mut child_sort: i64 = 0;
                    for ei in *s..=*t {
                        let (nm, content) = &inactive[ei];
                        let mut content = content.clone();
                        if ei == *s {
                            content = strip_open_tag(&content, tag);
                        }
                        if ei == *t {
                            content = strip_close_tag(&content, tag);
                        }
                        let d = Definition::new("prompt", nm, content);
                        let mut r = PromptNode::new_ref(
                            OwnerKind::Template,
                            &tmpl.id,
                            Some(sid.clone()),
                            child_sort,
                            &d.id,
                        );
                        r.enabled = false;
                        child_sort += 1;
                        nodes.push(r);
                        defs.push(d);
                    }
                    i = *t + 1;
                    continue;
                }
            }
            let (nm, content) = &inactive[i];
            let mut d = Definition::new("prompt", nm, content);
            if is_balanced(content) {
                d.meta = serde_json::json!({ "wrap_in_tag": true });
            }
            let mut r = PromptNode::new_ref(OwnerKind::Template, &tmpl.id, Some(fid.clone()), cs, &d.id);
            r.enabled = false;
            cs += 1;
            nodes.push(r);
            defs.push(d);
            i += 1;
        }
    }

    // Variables declared via {{setvar}} macros -> one root `variables` brick.
    if !vars.is_empty() {
        let mut vdef = Definition::new("variables", "Variables", "");
        vdef.meta = serde_json::json!({ "decls": vars });
        let sort = nodes.iter().map(|n| n.sort_order).max().unwrap_or(-1) + 1;
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, sort, &vdef.id));
        defs.push(vdef);
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
    fn imports_disabled_in_order_as_disabled_ref_skips_unknown() {
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
        // both authored prompts imported; ghost (unknown id) skipped
        assert_eq!(prompt_defs(&ls).len(), 2);
        let def_of = |nm: &str| ls.definitions.iter().find(|d| d.name == nm).unwrap();
        let ref_of = |nm: &str| {
            let id = &def_of(nm).id;
            ls.nodes.iter().find(|n| n.definition_id.as_deref() == Some(id.as_str())).unwrap()
        };
        assert!(ref_of("Main").enabled);
        assert!(!ref_of("Off").enabled, "disabled-in-order imported as a disabled Ref");
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
    fn reads_default_group_others_go_inactive() {
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
        // "main" is the active (enabled, root) prompt
        let main = ls.definitions.iter().find(|d| d.name == "Main").unwrap();
        let main_ref = ls.nodes.iter().find(|n| n.definition_id.as_deref() == Some(main.id.as_str())).unwrap();
        assert!(main_ref.enabled && main_ref.parent_id.is_none());
        // "other" (not in group 100000) -> disabled Ref under the inactive folder
        let inactive = ls.nodes.iter().find(|n| n.tag.as_deref() == Some("inactive")).expect("inactive folder");
        assert!(!inactive.enabled);
        let other = ls.definitions.iter().find(|d| d.name == "Other").unwrap();
        let other_ref = ls.nodes.iter().find(|n| n.definition_id.as_deref() == Some(other.id.as_str())).unwrap();
        assert_eq!(other_ref.parent_id.as_deref(), Some(inactive.id.as_str()));
        assert!(!other_ref.enabled);
    }

    #[test]
    fn larger_non_default_group_wins_over_empty_default_skeleton() {
        // Real ST exports often carry the empty/skeleton default order under
        // character_id 100000 alongside the user's actually-curated, larger
        // order under another character_id. The larger one should be used.
        let preset = json!({
            "prompts": [
                { "identifier": "main", "name": "Main", "content": "default" },
                { "identifier": "jb", "name": "Jailbreak", "content": "stay in character" },
                { "identifier": "nsfw", "name": "NSFW", "content": "be explicit" }
            ],
            "prompt_order": [
                { "character_id": 100000, "order": [ { "identifier": "main", "enabled": true } ] },
                { "character_id": 100001, "order": [
                    { "identifier": "main", "enabled": true },
                    { "identifier": "jb", "enabled": true },
                    { "identifier": "nsfw", "enabled": false }
                ] }
            ]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        let prompts = prompt_defs(&ls);
        assert_eq!(prompts.len(), 3, "all three entries from the larger group are imported, none dumped to inactive");
        assert!(ls.nodes.iter().all(|n| n.tag.as_deref() != Some("inactive")), "no inactive folder needed");
        let nsfw = ls.definitions.iter().find(|d| d.name == "NSFW").unwrap();
        let nsfw_ref = ls.nodes.iter().find(|n| n.definition_id.as_deref() == Some(nsfw.id.as_str())).unwrap();
        assert!(!nsfw_ref.enabled, "disabled-in-order stays a disabled ref in place, not inactive");
        assert!(nsfw_ref.parent_id.is_none());
    }

    #[test]
    fn inactive_tail_preserves_cross_prompt_tag_span_as_a_folder() {
        // Two not-in-order prompts whose contents form a cross-node tag span
        // must still be wrapped in a sub-folder under `inactive`, not
        // flattened into two unwrapped halves.
        let preset = json!({
            "prompts": [
                { "identifier": "main", "name": "Main", "content": "hi" },
                { "identifier": "open", "name": "Open", "content": "<Rule depth=\"0\">first" },
                { "identifier": "close", "name": "Close", "content": "second</Rule>" }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "main", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        let inactive = ls.nodes.iter().find(|n| n.tag.as_deref() == Some("inactive")).expect("inactive folder");
        let span_folder = ls
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Folder && n.tag.as_deref() == Some("Rule"))
            .expect("span sub-folder under inactive");
        assert_eq!(span_folder.parent_id.as_deref(), Some(inactive.id.as_str()));
        assert!(!span_folder.enabled);
        let open = ls.definitions.iter().find(|d| d.name == "Open").unwrap();
        let close = ls.definitions.iter().find(|d| d.name == "Close").unwrap();
        assert_eq!(open.content, "first");
        assert_eq!(close.content, "second");
        assert!(open.meta.get("wrap_in_tag").is_none(), "span children aren't individually wrapped");
        let open_ref = ls.nodes.iter().find(|n| n.definition_id.as_deref() == Some(open.id.as_str())).unwrap();
        assert_eq!(open_ref.parent_id.as_deref(), Some(span_folder.id.as_str()));
        assert!(!open_ref.enabled);
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

    #[test]
    fn setvar_registers_variables_and_emits_no_node_when_emptied() {
        let preset = json!({
            "prompts": [
                { "identifier": "vars", "name": "Vars", "content": "{{setvar::hp::100}}{{setvar::tone::soft}}" },
                { "identifier": "main", "name": "Main", "content": "use {{getvar::hp}}" }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "vars", "enabled": true },
                { "identifier": "main", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        // "vars" emptied by stripping -> no def; its variables registered on the template
        assert!(ls.definitions.iter().all(|d| d.name != "Vars"));
        // Variables register as a root `variables` brick, not template meta.
        let vbrick = ls
            .definitions
            .iter()
            .find(|d| d.def_type == "variables")
            .expect("a variables brick");
        let decls = vbrick.meta["decls"].as_array().unwrap();
        assert!(decls.iter().any(|v| v["name"] == "hp" && v["type"] == "number"));
        assert!(decls.iter().any(|v| v["name"] == "tone" && v["type"] == "string"));
        assert!(ls.template.meta.get("variables").is_none());
        // getvar rewritten to {{hp}} in main's content
        let main = ls.definitions.iter().find(|d| d.name == "Main").unwrap();
        assert_eq!(main.content, "use {{hp}}");
    }

    #[test]
    fn balanced_prompt_wraps_stray_tag_stays_raw() {
        let preset = json!({
            "prompts": [
                { "identifier": "a", "name": "Clean", "content": "just text" },
                { "identifier": "b", "name": "Stray", "content": "see <最新互动> here" }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "a", "enabled": true },
                { "identifier": "b", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        let wrap = |nm: &str| {
            ls.definitions.iter().find(|d| d.name == nm).unwrap()
                .meta.get("wrap_in_tag").and_then(|v| v.as_bool()).unwrap_or(false)
        };
        assert!(wrap("Clean"), "balanced/no-tag prompt is wrapped");
        assert!(!wrap("Stray"), "stray unclosed tag -> left raw");
    }

    #[test]
    fn cross_node_span_becomes_a_folder() {
        let preset = json!({
            "prompts": [
                { "identifier": "open", "name": "Open", "content": "<Rule depth=\"0\">first" },
                { "identifier": "close", "name": "Close", "content": "second</Rule>" }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "open", "enabled": true },
                { "identifier": "close", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        let folder = ls.nodes.iter().find(|n| n.kind == NodeKind::Folder && n.tag.as_deref() == Some("Rule")).expect("span folder");
        // both prompts are children of the folder, literal tags stripped, not individually wrapped
        let children: Vec<_> = ls.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(folder.id.as_str())).collect();
        assert_eq!(children.len(), 2);
        let open = ls.definitions.iter().find(|d| d.name == "Open").unwrap();
        let close = ls.definitions.iter().find(|d| d.name == "Close").unwrap();
        assert_eq!(open.content, "first");
        assert_eq!(close.content, "second");
        assert!(open.meta.get("wrap_in_tag").is_none(), "span children not individually wrapped");
    }

    #[test]
    fn infer_var_keeps_noncanonical_numeric_strings_as_string() {
        // Values that look numeric but aren't in canonical form (leading zeros,
        // trailing-zero decimals, scientific notation) are kept as strings so an
        // author's ID / version / code isn't silently renumbered.
        assert_eq!(infer_var("007").0, VarType::String);
        assert_eq!(infer_var("1.0").0, VarType::String);
        assert_eq!(infer_var("1e3").0, VarType::String);
        // canonical numbers and bools still infer correctly
        assert_eq!(infer_var("100").0, VarType::Number);
        assert_eq!(infer_var("3.14").0, VarType::Number);
        assert_eq!(infer_var("-5").0, VarType::Number);
        assert_eq!(infer_var("true").0, VarType::Bool);
    }

    #[test]
    fn extract_variables_handles_setvar_getvar_and_literals() {
        let (clean, decls) = extract_variables(
            "a{{setvar::hp::100}}b{{setvar::ok::true}}c{{setvar::note:: }}{{getvar::hp}}{{trim}}{{// c}}",
        );
        // setvar macros stripped; getvar rewritten; other macros kept literal
        assert_eq!(clean, "abc{{hp}}{{trim}}{{// c}}");
        assert_eq!(decls.len(), 3);
        let by = |n: &str| decls.iter().find(|d| d.name == n).unwrap();
        assert_eq!(by("hp").var_type, VarType::Number);
        assert_eq!(by("hp").initial, serde_json::json!(100.0));
        assert_eq!(by("ok").var_type, VarType::Bool);
        assert_eq!(by("ok").initial, serde_json::json!(true));
        assert_eq!(by("note").var_type, VarType::String);
        assert_eq!(by("note").initial, serde_json::json!(" "));
    }

    #[test]
    fn extract_variables_all_setvar_yields_empty_content() {
        let (clean, decls) = extract_variables("{{setvar::a:: }}{{setvar::b::继续}}");
        assert!(clean.trim().is_empty());
        assert_eq!(decls.len(), 2);
    }

    #[test]
    fn extract_variables_keeps_unterminated_and_malformed_literal() {
        assert_eq!(extract_variables("x{{setvar::y").0, "x{{setvar::y");
        assert_eq!(extract_variables("x{{setvar::nosep}}y").0, "x{{setvar::nosep}}y");
    }

    #[test]
    fn scan_tags_ignores_attributes_and_self_closing() {
        assert_eq!(
            scan_tags("<Rule depth=\"0\">x</Rule><br/><最新互动>"),
            vec![(false, "Rule".to_string()), (true, "Rule".to_string()), (false, "最新互动".to_string())]
        );
        assert!(is_balanced("<a>x</a> plain"));
        assert!(!is_balanced("<a>x"));
        assert!(is_balanced("no tags here"));
    }

    #[test]
    fn find_first_span_pairs_open_and_close() {
        let c = vec!["<rules>foo".to_string(), "mid".to_string(), "bar</rules>".to_string()];
        assert_eq!(find_first_span(&c), Some((0, 2, "rules".to_string())));
        let none = vec!["<x>unclosed".to_string(), "plain".to_string()];
        assert_eq!(find_first_span(&none), None);
    }

    #[test]
    fn find_first_span_scales_linearly_on_a_long_unmatched_run() {
        // A preset with thousands of unbalanced-tag prompts and no match is
        // the worst case for the naive O(n^2) scan (every element re-scans
        // every later element). This must stay fast — a few ms, not seconds —
        // so an imported preset can't be used to hang the import endpoint.
        let contents: Vec<String> = (0..4000).map(|i| format!("<tag{i}>no close here")).collect();
        let start = std::time::Instant::now();
        assert_eq!(find_first_span(&contents), None);
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() < 200, "find_first_span took {elapsed:?}, expected well under 200ms");
    }

    #[test]
    fn strip_tag_helpers_remove_first_occurrence() {
        assert_eq!(strip_open_tag("<Rule depth=\"0\">body", "Rule"), "body");
        assert_eq!(strip_close_tag("body</rules>", "rules"), "body");
    }
}
