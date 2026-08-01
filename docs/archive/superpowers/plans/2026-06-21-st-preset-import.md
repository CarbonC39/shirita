# SillyTavern Preset Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Import a SillyTavern chat-completion preset (`prompts` + `prompt_order`) as an editable Shirita Template, so `examples/示例预设.json` stops returning `import failed: 400`.

**Architecture:** A pure core adapter `stpreset_to_loreset(preset, name) -> LoreSet` maps the enabled, ordered prompts of group `100000` onto the Template node tree (authored text → `prompt` def + `Ref`; `chatHistory` → `History`; first other marker → one `Content` mount; the rest dropped). A new web `persist_preset` creates every definition **fresh** (no name dedup — preset prompt names are generic), mirroring `import_template_bundle`. The `import` handler gains a structural sniff (`prompts` array AND `prompt_order` array) and threads the uploaded filename stem in as the template name.

**Tech Stack:** Rust, `serde_json`, Axum multipart, existing `shirita-core` models (`Template`/`Definition`/`PromptNode`/`LoreSet`), `uuid`.

## Global Constraints

- Code comments and git commit messages in **English**.
- End every commit message with `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- **No new dependencies** — `uuid`, `serde_json`, `tracing` are already in both crates.
- The adapter is pure (no DB, no I/O); all persistence lives in the web layer.
- Faithful-but-lossy by design: samplers, depth injection, and per-prompt roles are dropped (see spec `docs/superpowers/specs/2026-06-21-st-preset-import-design.md`).

---

## File Structure

- `shirita-core/src/adapters/stpreset.rs` — **new**. The pure adapter + its unit tests. Sibling to `charcard.rs`/`worldinfo.rs`.
- `shirita-core/src/adapters/mod.rs` — **modify**. Register `pub mod stpreset;`.
- `shirita-core/src/lib.rs` — **modify**. Re-export `stpreset_to_loreset`.
- `shirita-web/src/routes/import_export.rs` — **modify**. Add `persist_preset`, the preset sniff arm, and filename threading (`first_field`).
- `shirita-web/tests/import_preset_test.rs` — **new**. Integration tests (real file, empty-order 400, collision independence).

---

## Task 1: Core `stpreset_to_loreset` adapter

**Files:**
- Create: `shirita-core/src/adapters/stpreset.rs`
- Modify: `shirita-core/src/adapters/mod.rs`
- Modify: `shirita-core/src/lib.rs:37-39` (re-export block)

**Interfaces:**
- Consumes: `crate::adapters::charcard::LoreSet`, `crate::models::{definition::Definition, prompt_node::{NodeKind, OwnerKind, PromptNode}, template::Template}`.
- Produces: `pub fn stpreset_to_loreset(preset: &serde_json::Value, name: &str) -> LoreSet`, re-exported as `shirita_core::stpreset_to_loreset`. When `name` is empty/whitespace the adapter fills a unique fallback `"Imported preset (xxxx)"` (4 hex chars), so the web layer can pass a raw filename stem (or `""`) without its own name policy.

- [ ] **Step 1: Register the module and write the failing unit tests**

Add to `shirita-core/src/adapters/mod.rs` (after `pub mod preset;`):

```rust
pub mod stpreset;
```

Create `shirita-core/src/adapters/stpreset.rs` with **only** the imports and the test module (the function does not exist yet — that is the failing state):

```rust
//! SillyTavern chat-completion preset -> Shirita loreset (Template + Definitions + Nodes).
//! Lossy: only the enabled, ordered prompts of the default group (character_id
//! 100000) are mapped. Authored text -> `prompt` def + `Ref`; `chatHistory` ->
//! `History`; the first other marker -> one `Content` mount (later markers
//! dropped). Samplers, depth injection, and per-prompt roles are out of scope.

use crate::adapters::charcard::LoreSet;
use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
use crate::models::template::Template;

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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shirita-core stpreset`
Expected: FAIL — `cannot find function \`stpreset_to_loreset\` in this scope`.

- [ ] **Step 3: Implement the adapter**

Insert the function in `shirita-core/src/adapters/stpreset.rs` between the `use` lines and the `#[cfg(test)] mod tests`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p shirita-core stpreset`
Expected: PASS — all 7 tests green.

- [ ] **Step 5: Re-export the function**

In `shirita-core/src/lib.rs`, add to the adapter re-export block (currently lines 37-39, after the `charcard` line):

```rust
pub use adapters::stpreset::stpreset_to_loreset;
```

Run: `cargo build -p shirita-core`
Expected: clean build (no unused-import or missing-symbol errors).

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/adapters/stpreset.rs shirita-core/src/adapters/mod.rs shirita-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(core): stpreset_to_loreset — ST chat-completion preset -> Template loreset

Maps the enabled, ordered prompts of group 100000: authored text -> prompt
def + Ref, chatHistory -> History, first other marker -> one Content mount,
the rest dropped. Empty/whitespace name gets a unique "Imported preset (xxxx)"
fallback. Pure and unit-tested.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Web `persist_preset` + sniff + filename threading + integration tests

**Files:**
- Modify: `shirita-web/src/routes/import_export.rs` (add `persist_preset`; replace `first_field_bytes` with `first_field`; add preset arm to `import`'s JSON sniff)
- Create: `shirita-web/tests/import_preset_test.rs`

**Interfaces:**
- Consumes: `shirita_core::stpreset_to_loreset` (Task 1), existing `LoreSet`, `NodeKind`, `OnConflict`, `ImportSummary`, `item(...)`, `state.storage.{list_templates,create_template,create_definition,create_node}`.
- Produces: `async fn persist_preset(state, ls: LoreSet, oc: OnConflict, summary) -> Result<(), StatusCode>` — creates the template (conflict unit) and every definition **fresh** (no name dedup), then inserts nodes (containers before refs). `async fn first_field(mp) -> Result<(Vec<u8>, Option<String>), StatusCode>` — first field's bytes + filename stem.

- [ ] **Step 1: Write the failing integration tests**

Create `shirita-web/tests/import_preset_test.rs`:

```rust
//! POST /api/import — SillyTavern chat-completion presets (prompts + prompt_order).

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state(dir: &std::path::Path) -> AppState {
    let storage = SqliteStorage::connect(dir.join("p.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", dir.join("assets").to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState {
        storage,
        config,
        provider,
        token_counter,
        model: "m".into(),
        generations: Arc::new(shirita_web::Generations::new()),
        http_client: shirita_web::new_http_client(),
    }
}

async fn import_named(state: &AppState, query: &str, filename: &str, data: &[u8]) -> (StatusCode, Value) {
    let boundary = "BND";
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/import{query}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn import_real_st_preset_creates_template_and_prompt_defs() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/示例预设.json")).unwrap();
    let (st, summary) = import_named(&state, "", "示例预设.json", &data).await;
    assert_eq!(st, StatusCode::OK);
    // template created, named after the filename stem
    let created = summary["created"].as_array().unwrap();
    assert!(created.iter().any(|c| c["kind"] == "template" && c["name"] == "示例预设"));
    // the three enabled authored prompts (main + nsfw + jailbreak) became prompt defs
    let defs = state.storage.list_definitions().await.unwrap();
    let prompts: Vec<_> = defs.iter().filter(|d| d.def_type == "prompt").collect();
    assert_eq!(prompts.len(), 3, "main + nsfw + jailbreak");
    assert!(prompts.iter().any(|d| d.name == "➡️扩写/转述输入"));
}

#[tokio::test]
async fn import_preset_with_empty_order_is_400() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let preset = serde_json::json!({
        "prompts": [],
        "prompt_order": [ { "character_id": 100000, "order": [] } ]
    });
    let (st, _) = import_named(&state, "", "empty.json", &serde_json::to_vec(&preset).unwrap()).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn two_presets_with_colliding_prompt_name_stay_independent() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let mk = |content: &str| {
        serde_json::json!({
            "prompts": [ { "identifier": "main", "name": "main", "content": content } ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "main", "enabled": true }
            ]}]
        })
    };
    // Distinct filenames -> distinct template names -> neither short-circuits under skip.
    let (s1, _) = import_named(&state, "", "preset-a.json", &serde_json::to_vec(&mk("AAA")).unwrap()).await;
    let (s2, _) = import_named(&state, "", "preset-b.json", &serde_json::to_vec(&mk("BBB")).unwrap()).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    let defs = state.storage.list_definitions().await.unwrap();
    let mains: Vec<_> = defs.iter().filter(|d| d.def_type == "prompt" && d.name == "main").collect();
    assert_eq!(mains.len(), 2, "fresh def per import — no dedup, no overwrite");
    assert!(mains.iter().any(|d| d.content == "AAA"));
    assert!(mains.iter().any(|d| d.content == "BBB"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shirita-web --test import_preset_test`
Expected: FAIL — the real-file import returns `400` (no preset arm yet), so `import_real_st_preset_...` fails its `assert_eq!(st, StatusCode::OK)`; the collision test likewise fails.

- [ ] **Step 3: Add `persist_preset` to `import_export.rs`**

Insert after `persist_loreset` (after line 374, before `pub async fn import`):

```rust
/// Persist an ST-preset loreset. The template name is the conflict unit (like
/// `import_template_bundle`); definitions are always created **fresh** (no
/// name dedup) because preset prompt names are generic (`main`, `nsfw`, …) and
/// deduping across imports would reuse or clobber an earlier preset's text.
/// Node `definition_id`s already point at the fresh def UUIDs from
/// `stpreset_to_loreset`, so no id remap is needed.
async fn persist_preset(
    state: &AppState,
    ls: LoreSet,
    oc: OnConflict,
    summary: &mut ImportSummary,
) -> Result<(), StatusCode> {
    if matches!(oc, OnConflict::Skip) {
        let templates = state.storage.list_templates().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(ex) = templates.iter().find(|t| t.name == ls.template.name) {
            summary.skipped.push(item("template", &ex.id, &ex.name));
            return Ok(());
        }
    }
    state.storage.create_template(&ls.template).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for d in &ls.definitions {
        state.storage.create_definition(d).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    // Container nodes (history/content) before refs, mirroring persist_loreset's
    // self-referential-FK ordering (preset refs are all roots, but keep it safe).
    let (containers, refs): (Vec<PromptNode>, Vec<PromptNode>) =
        ls.nodes.into_iter().partition(|n| n.kind != NodeKind::Ref);
    for n in containers.into_iter().chain(refs) {
        state.storage.create_node(&n).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    summary.created.push(item("template", &ls.template.id, &ls.template.name));
    Ok(())
}
```

- [ ] **Step 4: Replace `first_field_bytes` with `first_field` (captures the filename stem)**

Replace the existing helper (lines 305-310):

```rust
/// 读取首个 multipart 字段的字节。
async fn first_field_bytes(mut mp: Multipart) -> Result<Vec<u8>, StatusCode> {
    let field = mp.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)?.ok_or(StatusCode::BAD_REQUEST)?;
    let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(bytes.to_vec())
}
```

with:

```rust
/// Read the first multipart field's bytes plus its filename stem (no extension),
/// if any. The stem seeds the imported preset's template name.
async fn first_field(mut mp: Multipart) -> Result<(Vec<u8>, Option<String>), StatusCode> {
    let field = mp.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)?.ok_or(StatusCode::BAD_REQUEST)?;
    // Capture the (owned) stem before `bytes()` consumes the field.
    let stem = field.file_name().map(|f| {
        std::path::Path::new(f).file_stem().and_then(|s| s.to_str()).unwrap_or(f).to_string()
    });
    let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok((bytes.to_vec(), stem))
}
```

- [ ] **Step 5: Thread the filename and add the preset sniff arm in `import`**

In `pub async fn import`, change the field read (line 383) from:

```rust
    let bytes = first_field_bytes(mp).await?;
```

to:

```rust
    let (bytes, filename) = first_field(mp).await?;
```

Then replace the `_ =>` fallback arm of the `match v.get("format")...` block (current lines 415-426) with:

```rust
        _ => {
            // Structural sniff for an ST chat-completion preset (no `format`
            // field): both `prompts` and `prompt_order` are arrays. Checked
            // before the char-card/worldinfo heuristics.
            let is_preset = v.get("prompts").map(|p| p.is_array()).unwrap_or(false)
                && v.get("prompt_order").map(|o| o.is_array()).unwrap_or(false);
            let is_card = v.get("spec").and_then(|s| s.as_str()).map(|s| s.contains("chara_card")).unwrap_or(false)
                || v.get("data").and_then(|d| d.get("name")).is_some()
                || (v.get("name").is_some() && v.get("description").is_some());
            if is_preset {
                // Filename stem -> template name; empty -> adapter's unique fallback.
                let name = filename.as_deref().unwrap_or("");
                let ls = shirita_core::stpreset_to_loreset(&v, name);
                // Nothing usable (empty/missing enabled order) -> 400, not an empty template.
                if ls.definitions.is_empty() && !ls.nodes.iter().any(|n| n.kind == NodeKind::Content) {
                    return Err(StatusCode::BAD_REQUEST);
                }
                persist_preset(&state, ls, oc, &mut summary).await?;
            } else if is_card {
                persist_loreset(&state, charcard_to_loreset(&v), oc, &mut summary).await?;
            } else if v.get("entries").is_some() {
                persist_defs(&state, shirita_core::worldinfo_to_defs(&v), oc, &mut summary).await?;
            } else {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
```

- [ ] **Step 6: Run the full import test suite to verify it passes**

Run: `cargo test -p shirita-web --test import_preset_test`
Expected: PASS — all three tests green.

Then guard against regressions in the shared handler:

Run: `cargo test -p shirita-web --test import_test --test import_charcard_test --test import_empty_test --test pack_import_test`
Expected: PASS — the filename-threading change (`first_field`) does not break PNG / charcard / worldinfo / pack paths.

- [ ] **Step 7: Commit**

```bash
git add shirita-web/src/routes/import_export.rs shirita-web/tests/import_preset_test.rs
git commit -m "$(cat <<'EOF'
feat(web): import ST chat-completion presets as Templates

Add a structural sniff (prompts + prompt_order arrays) to /api/import that
routes presets through stpreset_to_loreset + a new persist_preset. Defs are
created fresh (no name dedup) so a later preset never reuses or clobbers an
earlier one's generic-named prompt. Thread the multipart filename stem in as
the template name. Empty enabled order -> 400.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review notes

- **Spec coverage:** §3 mapping (authored→prompt+Ref, chatHistory→History, first marker→Content, skips) → Task 1 Step 3 + unit tests. §3 name/edge cases (filename stem, unique fallback, append-history, empty→400) → Task 1 (fallback/append) + Task 2 Step 5 (400) + tests. §4 architecture (core fn + persist_preset, structural sniff, filename threading) → Task 2. §5 tests (all core bullets + web integration + collision independence) → Task 1 Step 1 + Task 2 Step 1. §6 decomposition → these two tasks.
- **Fallback location:** the spec left it between §4 (web) and §5 (core test); this plan places it in the **adapter** (so §5's core test holds and the web layer needs no name policy — it passes the raw stem or `""`). A minor, consistent refinement.
- **No placeholders, type consistency:** `stpreset_to_loreset(&Value, &str) -> LoreSet`, `persist_preset(&AppState, LoreSet, OnConflict, &mut ImportSummary)`, `first_field(Multipart) -> Result<(Vec<u8>, Option<String>), StatusCode>` are used identically wherever referenced.

## Out of scope

Sampler/depth/role fidelity; the full prompt library (only the enabled order); exporting Shirita templates as ST presets; the ST regex/JS frontend-compat work; the Pack-section import/export button gap (its own small plan).
