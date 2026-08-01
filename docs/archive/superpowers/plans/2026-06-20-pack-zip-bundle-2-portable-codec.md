# Pack Zip Bundle — Plan 2: Pack portable codec Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the pure `shirita.pack` codec to `portable.rs`: `export_pack` (manifest with identity + full content tree + inlined defs + meta), `parse_pack` (a new `PortableDoc::Pack`), plus the **deterministic** asset-ref `collect_pack_assets` and `rewrite_pack_assets` (designated fields only; dead-link blanking). All pure data transforms — no DB, no filesystem, no zip.

**Architecture:** Reuse the existing `export_template` / template-parse machinery by extracting two shared helpers — `inline_subtree` (export side) and `parse_subtree` (parse side) — so the template and pack codecs share the `local_id` indirection. `export_pack` differs from `export_template` in two ways: it carries `pack: { name, identity, meta }`, and it exports the **full** tree (no `filter_enabled`) so a shared pack keeps its disabled `select=one` alternatives. The asset collector/rewriter touch **only** designated fields (identity.avatar, inlined defs' `meta.avatar`, panel `/assets/…`) — never arbitrary strings — and blank unmapped refs.

**Tech Stack:** Rust, `serde_json`, `regex` (already a core dep), inline `#[cfg(test)]` unit tests.

## Global Constraints

- **Pure:** these functions take/return `serde_json::Value` + model structs; they never touch storage or the filesystem. Wiring (zip, dedup, DB) is Plans 3–4.
- **Deterministic asset refs only:** identity.avatar, each inlined definition's `meta.avatar`, and panel `meta.panel.html`/`css` `/assets/<path>` occurrences. A look-alike text variable (value `123.png`) is **never** collected or rewritten — asserted by test.
- **Dead-link blanking on rewrite:** a designated ref present but absent from the remap → avatar fields set to `null`, panel `/assets/…` occurrence stripped.
- **Full tree on pack export** (unlike `export_template`'s enabled-only) — disabled refs/alternatives survive.
- Comments/commits in English. Tests: `cargo test -p shirita-core portable` (the module's tests) + the existing template tests must stay green.

---

## File Structure

- `shirita-core/src/portable.rs` — shared subtree helpers, `export_pack`, `PortableDoc::Pack`, pack parse branch (Task 1); `collect_pack_assets`, `rewrite_pack_assets` (Task 2).
- `shirita-core/src/lib.rs` — re-export the new public fns. (Tasks 1 & 2)

---

### Task 1: `export_pack` + `parse_pack` (`PortableDoc::Pack`)

**Files:**
- Modify: `shirita-core/src/portable.rs`
- Modify: `shirita-core/src/lib.rs`

**Interfaces:**
- Consumes: `Pack`, `PackIdentity` (models), the existing `PromptNode` / `Definition` / `NodeKind`.
- Produces: `export_pack(&Pack, &[PromptNode], &HashMap<String, Definition>) -> Value` (`format: "shirita.pack"`); `PortableDoc::Pack { name: String, identity: PackIdentity, meta: Value, nodes: Vec<PortableNode>, defs: Vec<PortableDef> }` from `parse_portable`. (Plan 3 calls `export_pack`; Plan 4 matches `PortableDoc::Pack`.)

- [ ] **Step 1: Extract the shared export helper `inline_subtree`**

In `shirita-core/src/portable.rs`, add the import and a helper holding the body that `export_template` currently inlines:

```rust
use crate::models::pack::{Pack, PackIdentity};
```

```rust
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
```

- [ ] **Step 2: Slim `export_template` to use the helper (behavior-preserving)**

Replace the body of `export_template` (everything after its signature) with:

```rust
    let kept = filter_enabled(nodes);
    let (out_nodes, out_defs) = inline_subtree(&kept, defs);
    json!({
        "format": "shirita.template",
        "version": 1,
        "template": { "name": template.name, "meta": template.meta },
        "nodes": out_nodes,
        "definitions": out_defs,
    })
```

- [ ] **Step 3: Add `export_pack`**

After `export_template`, add:

```rust
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
```

- [ ] **Step 4: Add the `PortableDoc::Pack` variant**

In the `PortableDoc` enum, add the variant:

```rust
    Pack {
        name: String,
        identity: PackIdentity,
        meta: Value,
        nodes: Vec<PortableNode>,
        defs: Vec<PortableDef>,
    },
```

- [ ] **Step 5: Extract the shared parse helper `parse_subtree`**

Add this helper (it holds exactly what the `shirita.template` branch currently does for `defs` + `nodes`):

```rust
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
```

- [ ] **Step 6: Use `parse_subtree` in the template branch + add the pack branch**

In `parse_portable`, replace the `shirita.template` branch body so it uses the helper, and add the `shirita.pack` branch (both before the `_ => Err(...)` arm):

```rust
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
```

- [ ] **Step 7: Re-export `export_pack`**

In `shirita-core/src/lib.rs`, add `export_pack` to the `portable` re-export (the line already exporting `export_definition` / `export_template` / `parse_portable` / `PortableDoc`).

- [ ] **Step 8: Write the round-trip test**

In `portable.rs`'s `#[cfg(test)] mod tests`, add:

```rust
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
```

- [ ] **Step 9: Run the tests**

Run: `cargo test -p shirita-core portable 2>&1 | tail -20`
Expected: PASS — `pack_round_trip_keeps_identity_meta_and_full_tree` plus all the **existing** template/definition tests (the `inline_subtree`/`parse_subtree` extraction is behavior-preserving).

- [ ] **Step 10: Commit**

```bash
git add shirita-core/src/portable.rs shirita-core/src/lib.rs
git commit -m "feat(core): shirita.pack portable codec (export_pack + parse + PortableDoc::Pack)"
```

---

### Task 2: Deterministic asset collector + rewriter

**Files:**
- Modify: `shirita-core/src/portable.rs`
- Modify: `shirita-core/src/lib.rs`

**Interfaces:**
- Consumes: a `shirita.pack` manifest `Value`.
- Produces: `collect_pack_assets(&Value) -> Vec<String>` (distinct relative paths); `rewrite_pack_assets(&Value, &HashMap<String, String>) -> Value`. (Plan 3 uses `collect_pack_assets`; Plan 4 uses both.)

- [ ] **Step 1: Write the failing tests**

In `portable.rs`'s test module, add:

```rust
    #[test]
    fn collect_pack_assets_designated_only_ignores_lookalike_var() {
        let m = json!({
            "pack": { "identity": { "avatar": "a.png" },
                      "meta": { "panel": { "css": ".x{background:url(/assets/c.png)}", "html": "" },
                                "variables": [{ "name": "note", "type": "string", "initial": "d.png" }] } },
            "definitions": [ { "meta": { "avatar": "b.png" } }, { "meta": {} } ]
        });
        let got = collect_pack_assets(&m);
        assert!(got.contains(&"a.png".to_string()));
        assert!(got.contains(&"b.png".to_string()));
        assert!(got.contains(&"c.png".to_string()));
        assert!(!got.contains(&"d.png".to_string()), "look-alike text variable must NOT be collected");
        assert_eq!(got.len(), 3);
    }

    #[test]
    fn rewrite_pack_assets_remaps_and_blanks_dead_links() {
        let m = json!({
            "pack": { "identity": { "avatar": "a.png" },
                      "meta": { "panel": { "css": "url(/assets/c.png) url(/assets/gone.png)", "html": "" } } },
            "definitions": [ { "meta": { "avatar": "b.png" } } ]
        });
        let mut map = HashMap::new();
        map.insert("a.png".to_string(), "x.png".to_string());
        map.insert("c.png".to_string(), "z.png".to_string());
        // b.png and gone.png are intentionally absent from the map.
        let out = rewrite_pack_assets(&m, &map);
        assert_eq!(out["pack"]["identity"]["avatar"], "x.png");
        assert!(out["definitions"][0]["meta"]["avatar"].is_null(), "unmapped avatar blanked");
        let css = out["pack"]["meta"]["panel"]["css"].as_str().unwrap();
        assert!(css.contains("/assets/z.png"));
        assert!(!css.contains("gone.png"), "dead /assets link stripped");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p shirita-core portable 2>&1 | tail -15`
Expected: FAIL — `collect_pack_assets` / `rewrite_pack_assets` don't exist.

- [ ] **Step 3: Implement the collector + rewriter**

In `portable.rs`, add:

```rust
/// Walk a `mut Value` down a key path, returning `None` if any segment is absent.
fn field_mut<'a>(v: &'a mut Value, path: &[&str]) -> Option<&'a mut Value> {
    let mut cur = v;
    for k in path {
        cur = cur.get_mut(*k)?;
    }
    Some(cur)
}

fn push_unique(out: &mut Vec<String>, p: &str) {
    if !p.is_empty() && !out.iter().any(|x| x == p) {
        out.push(p.to_string());
    }
}

/// Distinct relative asset paths a `shirita.pack` manifest references, from
/// **designated fields only** — identity.avatar, each inlined definition's
/// `meta.avatar`, and panel `meta.panel.{html,css}` `/assets/<path>` occurrences.
/// Arbitrary strings (e.g. a text variable valued `123.png`) are never scanned.
pub fn collect_pack_assets(manifest: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(a) = manifest["pack"]["identity"]["avatar"].as_str() {
        push_unique(&mut out, a);
    }
    if let Some(defs) = manifest["definitions"].as_array() {
        for d in defs {
            if let Some(a) = d["meta"]["avatar"].as_str() {
                push_unique(&mut out, a);
            }
        }
    }
    let re = regex::Regex::new(r#"/assets/([^"'\s)]+)"#).unwrap();
    for key in ["html", "css"] {
        if let Some(text) = manifest["pack"]["meta"]["panel"][key].as_str() {
            for cap in re.captures_iter(text) {
                push_unique(&mut out, &cap[1]);
            }
        }
    }
    out
}

fn remap_field(field: &mut Value, map: &HashMap<String, String>) {
    if let Some(old) = field.as_str() {
        if old.is_empty() {
            return;
        }
        *field = match map.get(old) {
            Some(n) => Value::String(n.clone()),
            None => Value::Null, // unmapped → blank (dead-link guard)
        };
    }
}

/// Rewrite a manifest's **designated** asset refs through `map` (old rel → new
/// rel). A designated ref present but absent from the map is blanked — avatar
/// fields to `null`, panel `/assets/…` occurrences stripped — so import never
/// yields a dead link.
pub fn rewrite_pack_assets(manifest: &Value, map: &HashMap<String, String>) -> Value {
    let mut m = manifest.clone();
    if let Some(f) = field_mut(&mut m, &["pack", "identity", "avatar"]) {
        remap_field(f, map);
    }
    if let Some(defs) = m.get_mut("definitions").and_then(|d| d.as_array_mut()) {
        for d in defs {
            if let Some(f) = field_mut(d, &["meta", "avatar"]) {
                remap_field(f, map);
            }
        }
    }
    let re = regex::Regex::new(r#"/assets/([^"'\s)]+)"#).unwrap();
    for key in ["html", "css"] {
        let cur = field_mut(&mut m, &["pack", "meta", "panel", key])
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(text) = cur {
            let rewritten = re
                .replace_all(&text, |c: &regex::Captures| match map.get(&c[1]) {
                    Some(n) => format!("/assets/{n}"),
                    None => String::new(),
                })
                .into_owned();
            if let Some(f) = field_mut(&mut m, &["pack", "meta", "panel", key]) {
                *f = Value::String(rewritten);
            }
        }
    }
    m
}
```

- [ ] **Step 4: Re-export both fns**

In `shirita-core/src/lib.rs`, add `collect_pack_assets` and `rewrite_pack_assets` to the `portable` re-export line.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p shirita-core portable 2>&1 | tail -15`
Expected: PASS — the collector ignores the look-alike variable; the rewriter remaps and blanks dead links; the Task-1 + existing tests still pass.

- [ ] **Step 6: Build + commit**

```bash
cargo build -p shirita-core 2>&1 | tail -4
git add shirita-core/src/portable.rs shirita-core/src/lib.rs
git commit -m "feat(core): deterministic pack asset collect + rewrite (dead-link blanking)"
```

---

## Final Verification

- [ ] **Core test + build sweep**

Run: `cargo test -p shirita-core portable 2>&1 | tail -6 && cargo build --workspace 2>&1 | tail -4`
Expected: all `portable` tests pass (new + existing template/definition); workspace builds clean.

---

## Self-Review

**Spec coverage (spec §3, §4, §7-rewrite, §11.2):**
- `shirita.pack` manifest (identity + inlined full tree + meta) — Task 1 (`export_pack`).
- Round-trip parse → `PortableDoc::Pack` — Task 1 (`parse_subtree` + pack branch).
- Deterministic asset discovery (designated fields only; no string scan) — Task 2 (`collect_pack_assets` + the look-alike-variable test).
- Import-side ref rewrite + dead-link blanking — Task 2 (`rewrite_pack_assets`); Plan 4 builds the `map` (hash dedup) and calls it.
- Full tree (vs template's enabled-only) so `select=one` alternatives travel — Task 1 (`export_pack` skips `filter_enabled`; asserted by the disabled-ref test).

**Placeholder scan:** none — full helper/codec/collector/rewriter code, complete tests, exact commands. `inline_subtree`/`parse_subtree` are verbatim extractions of the current `export_template`/template-parse bodies (behavior-preserving; guarded by the existing tests).

**Type consistency:** `export_pack(&Pack, &[PromptNode], &HashMap<String, Definition>) -> Value` mirrors `export_template`'s shape. `PortableDoc::Pack { name, identity: PackIdentity, meta: Value, nodes: Vec<PortableNode>, defs: Vec<PortableDef> }` — `PackIdentity` parsed via `serde_json::from_value`. `collect_pack_assets(&Value) -> Vec<String>` and `rewrite_pack_assets(&Value, &HashMap<String, String>) -> Value` are the exact signatures Plans 3-4 consume; both touch only designated paths via `field_mut`.
