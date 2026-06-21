//! Shirita 原创导入导出格式编解码（纯数据变换，不触库）。
//! 节点/定义间引用用 `local_id`（与真实 UUID 解耦），导入侧再重映射为新 UUID。

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, PromptNode};
use crate::models::pack::{Pack, PackIdentity};
use crate::models::template::Template;
use crate::{Error, Result};

/// 单定义 → 原创信封。
pub fn export_definition(def: &Definition) -> Value {
    json!({
        "format": "shirita.definition",
        "version": 1,
        "definition": {
            "type": def.def_type,
            "name": def.name,
            "content": def.content,
            "meta": def.meta,
        }
    })
}

/// 仅保留自身及全部祖先都 enabled 的节点（排除 disabled 子树）。
fn filter_enabled(nodes: &[PromptNode]) -> Vec<&PromptNode> {
    let by_id: HashMap<&str, &PromptNode> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    nodes
        .iter()
        .filter(|n| {
            let mut cur: &PromptNode = n;
            loop {
                if !cur.enabled {
                    return false;
                }
                match cur.parent_id.as_deref().and_then(|p| by_id.get(p)) {
                    Some(p) => cur = p,
                    None => return true,
                }
            }
        })
        .collect()
}

/// Pack a selected node list + the defs they reference into local_id-keyed
/// `(nodes, definitions)` JSON arrays. Shared by template + pack export.
/// Refs with a dangling `definition_id` are skipped (+ warn) for referential safety.
fn inline_subtree(
    kept: &[&PromptNode],
    defs: &HashMap<String, Definition>,
) -> (Vec<Value>, Vec<Value>) {
    let node_lid: HashMap<&str, String> =
        kept.iter().enumerate().map(|(i, n)| (n.id.as_str(), format!("n{i}"))).collect();
    let mut def_lid: HashMap<String, String> = HashMap::new();
    let mut out_defs: Vec<Value> = Vec::new();
    let mut out_nodes: Vec<Value> = Vec::new();

    for n in kept {
        let mut def_local: Option<String> = None;
        if n.kind == NodeKind::Ref {
            match n.definition_id.as_ref().and_then(|id| defs.get(id)) {
                Some(d) => {
                    let lid = def_lid
                        .entry(d.id.clone())
                        .or_insert_with(|| {
                            let l = format!("d{}", out_defs.len());
                            out_defs.push(json!({
                                "local_id": l,
                                "type": d.def_type,
                                "name": d.name,
                                "content": d.content,
                                "meta": d.meta,
                            }));
                            l
                        })
                        .clone();
                    def_local = Some(lid);
                }
                None => {
                    tracing::warn!(node_id = %n.id, "inline_subtree: ref has dangling definition_id, skipping");
                    continue;
                }
            }
        }
        out_nodes.push(json!({
            "local_id": node_lid[n.id.as_str()],
            "parent_local_id": n.parent_id.as_deref().and_then(|p| node_lid.get(p)).cloned(),
            "kind": n.kind.as_str(),
            "tag": n.tag,
            "def_local_id": def_local,
            "enabled": n.enabled,
            "sort_order": n.sort_order,
            "meta": n.meta,
        }));
    }
    (out_nodes, out_defs)
}

/// 模板「启用部分」→ 原创信封：排除 disabled 子树；defs 只含被保留 ref 实际引用者（去重）；
/// 悬空 definition_id 的 ref 节点跳过 + warn，保证产出引用完整。
pub fn export_template(
    template: &Template,
    nodes: &[PromptNode],
    defs: &HashMap<String, Definition>,
) -> Value {
    let kept = filter_enabled(nodes);
    let (out_nodes, out_defs) = inline_subtree(&kept, defs);
    json!({
        "format": "shirita.template",
        "version": 1,
        "template": { "name": template.name, "meta": template.meta },
        "nodes": out_nodes,
        "definitions": out_defs,
    })
}

/// Pack → `shirita.pack` envelope: identity + variables/panel (`meta`) + the
/// **full** content tree (no enabled-filter, so disabled `select=one`
/// alternatives travel with the pack) + inlined definitions.
pub fn export_pack(
    pack: &Pack,
    nodes: &[PromptNode],
    defs: &HashMap<String, Definition>,
) -> Value {
    let kept: Vec<&PromptNode> = nodes.iter().collect();
    let (out_nodes, out_defs) = inline_subtree(&kept, defs);
    json!({
        "format": "shirita.pack",
        "version": 1,
        "pack": {
            "name": pack.name,
            "identity": serde_json::to_value(&pack.identity).unwrap_or_else(|_| json!({})),
            "meta": pack.meta,
        },
        "nodes": out_nodes,
        "definitions": out_defs,
    })
}

/// 解析后的可移植节点（local_id 形态，未落库）。
#[derive(Debug, Clone, PartialEq)]
pub struct PortableNode {
    pub local_id: String,
    pub parent_local_id: Option<String>,
    pub kind: NodeKind,
    pub tag: Option<String>,
    pub def_local_id: Option<String>,
    pub enabled: bool,
    pub sort_order: i64,
    pub meta: Value,
}

/// 解析后的可移植定义（带 local_id）。
#[derive(Debug, Clone, PartialEq)]
pub struct PortableDef {
    pub local_id: String,
    pub def_type: String,
    pub name: String,
    pub content: String,
    pub meta: Value,
}

/// 解析结果：单定义或模板 bundle 或 pack。
#[derive(Debug, Clone, PartialEq)]
pub enum PortableDoc {
    Definition(Definition),
    Template { name: String, meta: Value, nodes: Vec<PortableNode>, defs: Vec<PortableDef> },
    Pack {
        name: String,
        identity: PackIdentity,
        meta: Value,
        nodes: Vec<PortableNode>,
        defs: Vec<PortableDef>,
    },
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// Parse the `nodes` + `definitions` arrays shared by template/pack envelopes.
fn parse_subtree(v: &Value) -> Result<(Vec<PortableNode>, Vec<PortableDef>)> {
    let defs = v.get("definitions").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let defs: Vec<PortableDef> = defs
        .iter()
        .map(|d| PortableDef {
            local_id: s(d, "local_id"),
            def_type: s(d, "type"),
            name: s(d, "name"),
            content: s(d, "content"),
            meta: d.get("meta").cloned().unwrap_or_else(|| json!({})),
        })
        .collect();
    let nodes = v.get("nodes").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let nodes: Result<Vec<PortableNode>> = nodes
        .iter()
        .map(|n| {
            Ok(PortableNode {
                local_id: s(n, "local_id"),
                parent_local_id: n.get("parent_local_id").and_then(|x| x.as_str()).map(|x| x.to_string()),
                kind: NodeKind::from_db(&s(n, "kind"))?,
                tag: n.get("tag").and_then(|x| x.as_str()).map(|x| x.to_string()),
                def_local_id: n.get("def_local_id").and_then(|x| x.as_str()).map(|x| x.to_string()),
                enabled: n.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true),
                sort_order: n.get("sort_order").and_then(|x| x.as_i64()).unwrap_or(0),
                meta: n.get("meta").cloned().unwrap_or_else(|| json!({})),
            })
        })
        .collect();
    Ok((nodes?, defs))
}

/// 解析原创信封。`format` 不识别 → Err。
pub fn parse_portable(v: &Value) -> Result<PortableDoc> {
    match v.get("format").and_then(|f| f.as_str()) {
        Some("shirita.definition") => {
            let d = v.get("definition").ok_or_else(|| Error::Config("missing definition".into()))?;
            let mut def = Definition::new(s(d, "type"), s(d, "name"), s(d, "content"));
            def.meta = d.get("meta").cloned().unwrap_or_else(|| json!({}));
            Ok(PortableDoc::Definition(def))
        }
        Some("shirita.template") => {
            let t = v.get("template").ok_or_else(|| Error::Config("missing template".into()))?;
            let name = s(t, "name");
            let meta = t.get("meta").cloned().unwrap_or_else(|| json!({}));
            let (nodes, defs) = parse_subtree(v)?;
            Ok(PortableDoc::Template { name, meta, nodes, defs })
        }
        Some("shirita.pack") => {
            let p = v.get("pack").ok_or_else(|| Error::Config("missing pack".into()))?;
            let name = s(p, "name");
            let identity: PackIdentity =
                serde_json::from_value(p.get("identity").cloned().unwrap_or_else(|| json!({}))).unwrap_or_default();
            let meta = p.get("meta").cloned().unwrap_or_else(|| json!({}));
            let (nodes, defs) = parse_subtree(v)?;
            Ok(PortableDoc::Pack { name, identity, meta, nodes, defs })
        }
        _ => Err(Error::Config("unrecognized shirita format".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::prompt_node::OwnerKind;

    #[test]
    fn definition_round_trip() {
        let mut d = Definition::new("char", "Neo", "The One");
        d.meta = json!({ "wrap_in_tag": true });
        let v = export_definition(&d);
        assert_eq!(v["format"], "shirita.definition");
        match parse_portable(&v).unwrap() {
            PortableDoc::Definition(got) => {
                assert_eq!(got.def_type, "char");
                assert_eq!(got.name, "Neo");
                assert_eq!(got.content, "The One");
                assert_eq!(got.meta["wrap_in_tag"], true);
            }
            _ => panic!("expected definition"),
        }
    }

    #[test]
    fn template_export_filters_disabled_subtree() {
        // root folder(enabled) > ref A(enabled); disabled folder > ref B
        let fa = PromptNode::new_folder(OwnerKind::Template, "t", None, 0, "char");
        let a = Definition::new("char", "A", "aa");
        let ra = PromptNode::new_ref(OwnerKind::Template, "t", Some(fa.id.clone()), 0, &a.id);
        let mut fb = PromptNode::new_folder(OwnerKind::Template, "t", None, 1, "world");
        fb.enabled = false;
        let b = Definition::new("world", "B", "bb");
        let rb = PromptNode::new_ref(OwnerKind::Template, "t", Some(fb.id.clone()), 0, &b.id);

        let mut defs = HashMap::new();
        defs.insert(a.id.clone(), a.clone());
        defs.insert(b.id.clone(), b.clone());
        let tmpl = Template::new("T");
        let v = export_template(&tmpl, &[fa, ra, fb, rb], &defs);

        // 只剩 fa + ra（2 节点），defs 只含 A。
        assert_eq!(v["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(v["definitions"].as_array().unwrap().len(), 1);
        assert_eq!(v["definitions"][0]["name"], "A");
    }

    #[test]
    fn template_export_skips_dangling_ref() {
        // ref 指向 defs 里不存在的 id → 跳过该 ref，bundle 无 dangling。
        let r = PromptNode::new_ref(OwnerKind::Template, "t", None, 0, "missing-def-id");
        let defs: HashMap<String, Definition> = HashMap::new();
        let v = export_template(&Template::new("T"), &[r], &defs);
        assert_eq!(v["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(v["definitions"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn template_parse_reads_nodes_and_local_refs() {
        let v = json!({
            "format": "shirita.template", "version": 1,
            "template": { "name": "T", "meta": {} },
            "nodes": [
                { "local_id": "n0", "parent_local_id": null, "kind": "folder", "tag": "char",
                  "def_local_id": null, "enabled": true, "sort_order": 0 },
                { "local_id": "n1", "parent_local_id": "n0", "kind": "ref", "tag": null,
                  "def_local_id": "d0", "enabled": true, "sort_order": 0 }
            ],
            "definitions": [ { "local_id": "d0", "type": "char", "name": "A", "content": "aa", "meta": {} } ]
        });
        match parse_portable(&v).unwrap() {
            PortableDoc::Template { name, nodes, defs, .. } => {
                assert_eq!(name, "T");
                assert_eq!(nodes.len(), 2);
                assert_eq!(nodes[1].parent_local_id.as_deref(), Some("n0"));
                assert_eq!(nodes[1].def_local_id.as_deref(), Some("d0"));
                assert_eq!(defs.len(), 1);
                assert_eq!(defs[0].local_id, "d0");
            }
            _ => panic!("expected template"),
        }
    }

    #[test]
    fn pack_round_trip_keeps_identity_meta_and_full_tree() {
        let mut pack = Pack::new("Alice");
        pack.identity.avatar = Some("av.png".into());
        pack.identity.display_name = Some("Alice".into());
        pack.meta = json!({
            "variables": [{ "name": "hp", "type": "number", "initial": 100 }],
            "panel": { "html": "<b>{{hp}}</b>", "css": ".x{}", "caps": {} }
        });

        // folder > enabled ref A + DISABLED ref B; both must survive (no filter).
        let f = PromptNode::new_folder(OwnerKind::Pack, &pack.id, None, 0, "char");
        let a = Definition::new("char", "A", "aa");
        let ra = PromptNode::new_ref(OwnerKind::Pack, &pack.id, Some(f.id.clone()), 0, &a.id);
        let b = Definition::new("char", "B", "bb");
        let mut rb = PromptNode::new_ref(OwnerKind::Pack, &pack.id, Some(f.id.clone()), 1, &b.id);
        rb.enabled = false;
        let mut defs = HashMap::new();
        defs.insert(a.id.clone(), a.clone());
        defs.insert(b.id.clone(), b.clone());

        let v = export_pack(&pack, &[f, ra, rb], &defs);
        assert_eq!(v["format"], "shirita.pack");
        assert_eq!(v["pack"]["identity"]["avatar"], "av.png");
        assert_eq!(v["nodes"].as_array().unwrap().len(), 3);        // full tree incl. disabled
        assert_eq!(v["definitions"].as_array().unwrap().len(), 2);  // A + B both inlined

        match parse_portable(&v).unwrap() {
            PortableDoc::Pack { name, identity, meta, nodes, defs } => {
                assert_eq!(name, "Alice");
                assert_eq!(identity.avatar.as_deref(), Some("av.png"));
                assert_eq!(meta["panel"]["html"], "<b>{{hp}}</b>");
                assert_eq!(nodes.len(), 3);
                assert_eq!(defs.len(), 2);
            }
            _ => panic!("expected pack"),
        }
    }

    #[test]
    fn unknown_format_errors() {
        assert!(parse_portable(&json!({ "format": "whatever" })).is_err());
    }
}
