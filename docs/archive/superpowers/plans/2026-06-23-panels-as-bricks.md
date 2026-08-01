# Panels-as-Bricks Implementation Plan (Plan 1 of 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make panels first-class Lego bricks — an `html`/`css` definition pair grouped under a `panel` folder, resolved server-side and rendered in chat — replacing the `pack.meta.panel` god-object blob.

**Architecture:** New reserved, non-rendering definition types `html` and `css`. A panel is a folder node tagged `panel` whose `meta` carries `{name, caps}` and whose children are `html`/`css`/`regex_rule` refs. A pure `collect_panels` + async `resolve_session_panels` walk the effective template/session tree and mounted-pack trees, joining each folder's html/css children with `"\n"`. `GET /sessions/:id/panels` returns them; `ChatView` renders the stack. The ST charcard importer emits a panel folder instead of `meta.panel`. **Variables stay in `meta` this plan** — they are Plan 2.

**Tech Stack:** Rust (shirita-core lib, shirita-web Axum + sqlx/SQLite), Vue 3 + TypeScript + Vite + Vitest + Tailwind (shirita-ui).

## Global Constraints

- Comments and commit messages in English.
- TDD: write the failing test first, minimal implementation, frequent commits.
- **No data migration** (testing phase, no users).
- Axum path params use brace syntax: `/sessions/{id}/panels`.
- i18n: `en` is the source schema; every new key MUST be added to all four locale files (`en.ts`, `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts`) — a parity test fails otherwise.
- Backend tests: `cargo test -p shirita-core` and `cargo test -p shirita-web`.
- Frontend tests: `cd shirita-ui && npx vitest run <path>`.
- Reserved def types are never user-created containers and never emit into the LLM prompt; the only guard for the latter is `assembly::is_non_rendering`.

---

### Task 1: Reserve `html`/`css` and mark them non-rendering

**Files:**
- Modify: `shirita-core/src/models/def_type.rs:6` (RESERVED array)
- Modify: `shirita-core/src/assembly.rs:385-387` (`is_non_rendering`)

**Interfaces:**
- Produces: `def_type::RESERVED` now contains `"html"`, `"css"`; `def_type::is_reserved("html") == true`. `assembly::is_non_rendering("html") == true` (private fn; consumed only inside `assembly.rs`).

- [ ] **Step 1: Write the failing tests**

In `shirita-core/src/models/def_type.rs`, add to `mod tests`:

```rust
#[test]
fn html_css_are_reserved() {
    assert!(is_reserved("html"));
    assert!(is_reserved("css"));
    assert!(!is_reserved("char"));
}
```

In `shirita-core/src/assembly.rs`, inside its `#[cfg(test)] mod tests`, add:

```rust
#[test]
fn html_and_css_are_non_rendering() {
    assert!(is_non_rendering("html"));
    assert!(is_non_rendering("css"));
    assert!(is_non_rendering("regex_rule"));
    assert!(!is_non_rendering("prompt"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shirita-core html_css_are_reserved html_and_css_are_non_rendering`
Expected: FAIL (`html`/`css` not yet reserved / non-rendering).

- [ ] **Step 3: Implement**

In `def_type.rs:6`:

```rust
pub const RESERVED: [&str; 7] =
    ["prompt", "regex_rule", "tool", "first_message", "protocol", "html", "css"];
```

In `assembly.rs:385-387`:

```rust
fn is_non_rendering(def_type: &str) -> bool {
    matches!(def_type, "regex_rule" | "first_message" | "html" | "css")
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core`
Expected: PASS (whole crate, to catch any RESERVED-length assumptions).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/models/def_type.rs shirita-core/src/assembly.rs
git commit -m "feat(def-type): reserve html/css brick types and mark them non-rendering"
```

---

### Task 2: `RenderedPanel` + panel resolution module

**Files:**
- Create: `shirita-core/src/panels.rs`
- Modify: `shirita-core/src/lib.rs` (register module + re-export)

**Interfaces:**
- Consumes: `conversation::effective_nodes` (pub), `Storage::list_nodes`, `Storage::get_definition`.
- Produces:
  - `pub struct RenderedPanel { pub id: String, pub name: String, pub html: String, pub css: String, pub caps: serde_json::Value }` (Serialize/Deserialize)
  - `pub fn collect_panels(nodes: &[PromptNode], defs: &HashMap<String, Definition>) -> Vec<RenderedPanel>`
  - `pub async fn resolve_session_panels(storage: &dyn Storage, session: &Session) -> crate::Result<Vec<RenderedPanel>>`

- [ ] **Step 1: Write the failing test**

Create `shirita-core/src/panels.rs` with the test module only (implementation comes in Step 3):

```rust
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
```

Add `pub mod panels;` to `shirita-core/src/lib.rs` (after `pub mod pack;`? — it lives under `models`; add `pub mod panels;` alphabetically near line 17, after `pub mod pngcard;` is fine) so the test compiles.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core collect_joins_html_and_css_with_newline`
Expected: FAIL to compile (`collect_panels`, `RenderedPanel` undefined).

- [ ] **Step 3: Write the implementation**

Prepend to `shirita-core/src/panels.rs` (above the test module):

```rust
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
```

Add the re-export to `shirita-core/src/lib.rs` (near the other `pub use`, e.g. after the `portable` re-export block):

```rust
pub use panels::{collect_panels, resolve_session_panels, RenderedPanel};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core panels`
Expected: PASS (all three `panels::tests`).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/panels.rs shirita-core/src/lib.rs
git commit -m "feat(panels): resolve panel folders into RenderedPanel (html/css joined with newline)"
```

---

### Task 3: `GET /sessions/{id}/panels` endpoint

**Files:**
- Modify: `shirita-web/src/routes/sessions.rs` (add `get_panels` handler)
- Modify: `shirita-web/src/lib.rs:67` area (register route)
- Test: create `shirita-web/tests/panels_test.rs`

**Interfaces:**
- Consumes: `shirita_core::panels::resolve_session_panels`, `RenderedPanel`.
- Produces: route `GET /sessions/{id}/panels` → `200 [RenderedPanel...]`, `404` if the session is absent.

- [ ] **Step 1: Write the failing test**

Create `shirita-web/tests/panels_test.rs`. Reuse the harness shape from `packs_test.rs` (`test_state`, `send`, `body_json` — copy those three helpers verbatim from `shirita-web/tests/packs_test.rs:1-58`), then add:

```rust
use shirita_core::models::definition::Definition;
use shirita_core::models::pack::Pack;
use shirita_core::models::prompt_node::{OwnerKind, PromptNode};
use shirita_core::models::session::Session;

#[tokio::test]
async fn get_panels_returns_mounted_pack_panel() {
    let state = test_state().await;

    // A pack with a panel folder: html + css children.
    let pack = Pack::new("HUD pack");
    state.storage.create_pack(&pack).await.unwrap();

    let html = Definition::new("html", "markup", "<div id=\"a\"></div>");
    let css = Definition::new("css", "theme", ".a{color:red}");
    state.storage.create_definition(&html).await.unwrap();
    state.storage.create_definition(&css).await.unwrap();

    let mut folder = PromptNode::new_folder(OwnerKind::Pack, &pack.id, None, 0, "panel");
    folder.meta = serde_json::json!({ "name": "Status", "caps": { "write": true } });
    let h = PromptNode::new_ref(OwnerKind::Pack, &pack.id, Some(folder.id.clone()), 0, &html.id);
    let c = PromptNode::new_ref(OwnerKind::Pack, &pack.id, Some(folder.id.clone()), 1, &css.id);
    state.storage.create_node(&folder).await.unwrap();
    state.storage.create_node(&h).await.unwrap();
    state.storage.create_node(&c).await.unwrap();

    // A session that mounts the pack.
    let session = Session::new("chat");
    state.storage.create_session(&session).await.unwrap();
    state.storage.set_mounted_packs(&session.id, &[pack.id.clone()]).await.unwrap();

    let (status, body) = send(&state, "GET", &format!("/sessions/{}/panels", session.id), None).await;
    assert_eq!(status, StatusCode::OK);
    let panels = body_json(&body);
    assert_eq!(panels.as_array().unwrap().len(), 1);
    assert_eq!(panels[0]["name"], "Status");
    assert_eq!(panels[0]["html"], "<div id=\"a\"></div>");
    assert_eq!(panels[0]["css"], ".a{color:red}");
    assert_eq!(panels[0]["caps"]["write"], true);
}

#[tokio::test]
async fn get_panels_missing_session_is_404() {
    let state = test_state().await;
    let (status, _) = send(&state, "GET", "/sessions/nope/panels", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
```

> If `Session::new("chat")` is not the exact constructor, mirror how `sessions_test.rs` / `packs_test.rs` create a session (some suites POST `/sessions`); use whichever the existing suites use. The assertions stay the same.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-web get_panels_returns_mounted_pack_panel`
Expected: FAIL (route not found → `404`/`405`, or compile error on the missing handler).

- [ ] **Step 3: Implement the handler + route**

In `shirita-web/src/routes/sessions.rs`, add (the `State`, `Path`, `Json`, `StatusCode` imports already exist in this file):

```rust
/// Renderable panels for a session: panel folders across the effective
/// template/session tree + mounted packs, html/css joined server-side.
pub async fn get_panels(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<shirita_core::panels::RenderedPanel>>, StatusCode> {
    let session = state
        .storage
        .get_session(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let panels = shirita_core::panels::resolve_session_panels(&*state.storage, &session)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(panels))
}
```

In `shirita-web/src/lib.rs`, alongside the existing `/sessions/{id}/packs` route (line ~67):

```rust
.route("/sessions/{id}/panels", get(routes::sessions::get_panels))
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-web panels`
Expected: PASS (both panel tests).

- [ ] **Step 5: Commit**

```bash
git add shirita-web/src/routes/sessions.rs shirita-web/src/lib.rs shirita-web/tests/panels_test.rs
git commit -m "feat(web): GET /sessions/:id/panels resolves panel folders for the session"
```

---

### Task 4: charcard importer emits a panel folder

**Files:**
- Modify: `shirita-core/src/adapters/charcard.rs` (regex loop ~358-373; panel/meta emit ~408-427)
- Modify: tests in same file (`charcard_to_loreset_populates_panel_for_unambiguous_status_bar` ~800-821, and the `loreset_to_pack`/panel assertions ~778-790, 834, 857)

**Interfaces:**
- Consumes: `PromptNode::new_folder`, `PromptNode::new_ref`, `Definition::new`, the existing `PanelConversion { source_index, html, css, var_decls, capture_vars }`.
- Produces: when a status-bar panel is detected, the loreset gains an `OwnerKind::Template` folder node `tag = "panel"` with `meta = { name, caps: {} }`, an `html` def + ref child, a `css` def + ref child, and the panel-sync `regex_rule` ref **re-parented under that folder**. `template.meta` no longer gets a `panel` key. (Variables still go to `template.meta.variables` this plan.)

- [ ] **Step 1: Rewrite the test for the new shape**

Replace `charcard_to_loreset_populates_panel_for_unambiguous_status_bar` (~800-821) with:

```rust
#[test]
fn charcard_to_loreset_emits_panel_folder_for_unambiguous_status_bar() {
    let card = serde_json::json!({
        "data": {
            "name": "Neo", "description": "desc",
            "extensions": { "regex_scripts": [
                { "scriptName": "status", "findRegex": "<hp>(\\d+)</hp>",
                  "replaceString": "HP: $1", "disabled": false, "markdownOnly": true }
            ] }
        }
    });
    let ls = charcard_to_loreset(&card);

    // No panel blob on template meta anymore.
    assert!(ls.template.meta.get("panel").is_none());

    // A panel folder exists.
    let folder = ls.nodes.iter()
        .find(|n| n.kind == NodeKind::Folder && n.tag.as_deref() == Some("panel"))
        .expect("a panel folder must be emitted");

    // It has an html child whose def content is the converted markup.
    let html_ref = ls.nodes.iter()
        .find(|n| n.kind == NodeKind::Ref && n.parent_id.as_deref() == Some(folder.id.as_str())
            && ls.definitions.iter().any(|d| Some(d.id.as_str()) == n.definition_id.as_deref() && d.def_type == "html"))
        .expect("panel folder has an html child");
    let html_def = ls.definitions.iter()
        .find(|d| Some(d.id.as_str()) == html_ref.definition_id.as_deref()).unwrap();
    assert_eq!(html_def.content, "HP: {{field1}}");

    // It has a css child (may be empty content when the card had no <style>).
    assert!(ls.nodes.iter().any(|n| n.kind == NodeKind::Ref
        && n.parent_id.as_deref() == Some(folder.id.as_str())
        && ls.definitions.iter().any(|d| Some(d.id.as_str()) == n.definition_id.as_deref() && d.def_type == "css")));

    // The panel-sync regex rule lives INSIDE the folder and keeps its capture_vars.
    let rule_ref = ls.nodes.iter()
        .find(|n| n.kind == NodeKind::Ref && n.parent_id.as_deref() == Some(folder.id.as_str())
            && ls.definitions.iter().any(|d| Some(d.id.as_str()) == n.definition_id.as_deref() && d.def_type == "regex_rule"))
        .expect("panel-sync regex is parented under the folder");
    let rule = ls.definitions.iter()
        .find(|d| Some(d.id.as_str()) == rule_ref.definition_id.as_deref()).unwrap();
    assert_eq!(rule.meta["capture_vars"], serde_json::json!(["field1"]));

    // Variables still register on template meta this plan (Plan 2 moves them).
    let vars = ls.template.meta["variables"].as_array().unwrap();
    assert!(vars.iter().any(|v| v["name"] == "field1"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core charcard_to_loreset_emits_panel_folder_for_unambiguous_status_bar`
Expected: FAIL (no panel folder; regex still at root; `template.meta.panel` still present).

- [ ] **Step 3: Implement**

3a. In the regex loop (`charcard.rs` ~359-373), **collect** the panel-sync regex def id instead of pushing its ref at root. Replace the loop body so that the script at `conv.source_index` does not get a root ref; capture its def id:

```rust
let panel_conversion = try_convert_status_panel(scripts);
let mut panel_regex_def_id: Option<String> = None;
for (index, s) in scripts.iter().enumerate() {
    let mut d = regex_rule_def(s);
    let is_panel_rule = panel_conversion.as_ref().map_or(false, |c| c.source_index == index);
    if is_panel_rule {
        if let Some(conv) = &panel_conversion {
            if let Some(obj) = d.meta.as_object_mut() {
                obj.insert("capture_vars".to_string(), serde_json::to_value(&conv.capture_vars).unwrap());
            }
        }
        panel_regex_def_id = Some(d.id.clone());
        defs.push(d); // def only; its ref is added under the panel folder below
    } else {
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
        defs.push(d);
    }
}
```

3b. Replace the `meta.panel` emit block (~422-424) — delete the `meta.insert("panel", …)` line — and after the `meta`/`variables` block, emit the panel folder + bricks:

```rust
// --- panel folder (html/css bricks + the panel-sync regex ref) ---
if let Some(conv) = &panel_conversion {
    let mut folder =
        PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, next(&mut sort), "panel");
    folder.meta = serde_json::json!({ "name": format!("{name}·panel"), "caps": {} });
    let mut csort: i64 = 0;

    let html = Definition::new("html", format!("{name}·panel·html"), conv.html.clone());
    nodes.push(PromptNode::new_ref(
        OwnerKind::Template, &tmpl.id, Some(folder.id.clone()), next(&mut csort), &html.id));
    defs.push(html);

    let css = Definition::new("css", format!("{name}·panel·css"), conv.css.clone());
    nodes.push(PromptNode::new_ref(
        OwnerKind::Template, &tmpl.id, Some(folder.id.clone()), next(&mut csort), &css.id));
    defs.push(css);

    if let Some(rid) = &panel_regex_def_id {
        nodes.push(PromptNode::new_ref(
            OwnerKind::Template, &tmpl.id, Some(folder.id.clone()), next(&mut csort), rid));
    }
    nodes.push(folder);
}
```

> `next(&mut x)` is the existing local sort-counter helper used throughout this function; `name` is the card name already in scope. Keep the `variables` registration block exactly as-is this plan.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core charcard`
Expected: PASS. Fix any sibling tests that asserted `template.meta["panel"]` — those assertions are removed/replaced by the folder shape above. (`try_convert_status_panel_*` unit tests are unaffected.)

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/adapters/charcard.rs
git commit -m "feat(import): emit a panel folder (html/css bricks + sync regex) instead of meta.panel"
```

---

### Task 5: Portable assets scan/remap over html/css brick content

**Files:**
- Modify: `shirita-core/src/portable.rs` — `collect_pack_assets` (~235-256), `rewrite_pack_assets` (~277-307), and their tests (~426-488)

**Interfaces:**
- Consumes: `ASSET_REF_RE`, existing `field_mut`, `push_unique`, `remap_field`.
- Produces: `collect_pack_assets` scans every `definitions[].content` whose `type ∈ {html, css}` for `/assets/<path>`; `rewrite_pack_assets` rewrites those same fields. The `pack.meta.panel.{html,css}` paths are gone.

- [ ] **Step 1: Rewrite the asset tests for brick content**

In `portable.rs` tests, replace the panel-meta-based asset cases (~462-488) with definition-content cases:

```rust
#[test]
fn collect_pack_assets_scans_html_css_brick_content() {
    let manifest = json!({
        "format": "shirita.pack", "version": 1,
        "pack": { "name": "P", "identity": { "avatar": "a.png" }, "meta": {} },
        "nodes": [],
        "definitions": [
            { "local_id": "h", "type": "html", "name": "m", "content": "<img src=\"/assets/c.png\">", "meta": {} },
            { "local_id": "s", "type": "css",  "name": "t", "content": ".x{background:url(/assets/bg.png)}", "meta": {} }
        ]
    });
    let assets = collect_pack_assets(&manifest);
    assert!(assets.contains(&"a.png".to_string()));
    assert!(assets.contains(&"c.png".to_string()));
    assert!(assets.contains(&"bg.png".to_string()));
}

#[test]
fn rewrite_pack_assets_remaps_html_css_brick_content() {
    let manifest = json!({
        "format": "shirita.pack", "version": 1,
        "pack": { "name": "P", "identity": {}, "meta": {} },
        "nodes": [],
        "definitions": [
            { "local_id": "s", "type": "css", "name": "t",
              "content": "url(/assets/c.png) url(/assets/gone.png)", "meta": {} }
        ]
    });
    let mut map = std::collections::HashMap::new();
    map.insert("c.png".to_string(), "new/c.png".to_string());
    let out = rewrite_pack_assets(&manifest, &map);
    let css = out["definitions"][0]["content"].as_str().unwrap();
    assert!(css.contains("/assets/new/c.png"));
    assert!(!css.contains("gone.png")); // unmapped → blanked
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shirita-core collect_pack_assets_scans_html_css_brick_content rewrite_pack_assets_remaps_html_css_brick_content`
Expected: FAIL (current code scans `pack.meta.panel`, not def content).

- [ ] **Step 3: Implement**

In `collect_pack_assets`, delete the `for key in ["html", "css"] { … pack.meta.panel … }` block (~248-254) and extend the existing `definitions` loop to also scan html/css content:

```rust
if let Some(defs) = manifest["definitions"].as_array() {
    for d in defs {
        if let Some(a) = d["meta"]["avatar"].as_str() {
            push_unique(&mut out, a);
        }
        let ty = d["type"].as_str().unwrap_or("");
        if ty == "html" || ty == "css" {
            if let Some(text) = d["content"].as_str() {
                for cap in re.captures_iter(text) {
                    push_unique(&mut out, &cap[1]);
                }
            }
        }
    }
}
```

(Move the `let re = &*ASSET_REF_RE;` binding above this loop if it currently sits after the deleted block.)

In `rewrite_pack_assets`, delete the `for key in ["html", "css"] { … pack.meta.panel … }` block (~289-305) and extend the `definitions` rewrite loop:

```rust
let re = &*ASSET_REF_RE;
if let Some(defs) = m.get_mut("definitions").and_then(|d| d.as_array_mut()) {
    for d in defs {
        if let Some(f) = field_mut(d, &["meta", "avatar"]) {
            remap_field(f, map);
        }
        let ty = d.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if ty == "html" || ty == "css" {
            if let Some(text) = d.get("content").and_then(|v| v.as_str()).map(|s| s.to_string()) {
                let rewritten = re
                    .replace_all(&text, |c: &regex::Captures| match map.get(&c[1]) {
                        Some(n) => format!("/assets/{n}"),
                        None => String::new(),
                    })
                    .into_owned();
                if let Some(f) = d.get_mut("content") {
                    *f = Value::String(rewritten);
                }
            }
        }
    }
}
```

(If a separate `definitions` avatar-remap loop already exists, merge the html/css branch into it rather than adding a second loop.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core portable`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/portable.rs
git commit -m "feat(portable): scan/remap assets in html/css brick content instead of meta.panel"
```

---

### Task 6: Frontend API types + client

**Files:**
- Modify: `shirita-ui/src/api/types.ts` (add `SessionPanel`)
- Modify: `shirita-ui/src/api/client.ts` (add `getSessionPanels`)
- Test: `shirita-ui/src/api/client.test.ts`

**Interfaces:**
- Produces: `SessionPanel { id: string; name: string; html: string; css: string; caps: PanelCaps }`; `getSessionPanels(sessionId: string): Promise<SessionPanel[]>`. `PanelCaps` already exists and is reused.

- [ ] **Step 1: Write the failing test**

In `shirita-ui/src/api/client.test.ts`, follow the existing fetch-mock pattern in that file and add:

```ts
it('getSessionPanels GETs /sessions/:id/panels', async () => {
  const panels = [{ id: 'F', name: 'Status', html: '<b/>', css: '.x{}', caps: { write: true } }]
  fetchMock.mockResolvedValueOnce({ ok: true, json: async () => panels } as Response)
  const out = await getSessionPanels('s1')
  expect(fetchMock).toHaveBeenCalledWith(expect.stringContaining('/sessions/s1/panels'), expect.anything())
  expect(out).toEqual(panels)
})
```

> Match the exact mock/assertion idiom already used by neighboring tests in `client.test.ts` (e.g. how `getPack` is tested) — import `getSessionPanels` at the top with the other imports.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts`
Expected: FAIL (`getSessionPanels` is not exported).

- [ ] **Step 3: Implement**

In `types.ts`, after the `Panel`/`PanelCaps` interfaces:

```ts
/** A server-resolved panel for a session: one `panel` folder's combined
 *  html/css plus its caps. Returned by GET /sessions/:id/panels. */
export interface SessionPanel {
  id: string
  name: string
  html: string
  css: string
  caps: PanelCaps
}
```

In `client.ts`, mirroring the existing GET helpers (e.g. `getPack`):

```ts
export async function getSessionPanels(sessionId: string): Promise<SessionPanel[]> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/panels`, { headers: authHeaders() })
  if (!res.ok) throw new Error(`getSessionPanels failed: ${res.status}`)
  return res.json()
}
```

> Use the same base-URL constant, auth-header helper, and error idiom as the surrounding functions in `client.ts` (copy from `getPack`). Add `SessionPanel` to the type import from `./types`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/api/client.ts shirita-ui/src/api/client.test.ts
git commit -m "feat(ui-api): SessionPanel type + getSessionPanels client"
```

---

### Task 7: ChatView renders panels from the endpoint

**Files:**
- Modify: `shirita-ui/src/views/ChatView.vue` (`loadPanels`, `panelOf`, `onPanelAction`, panel-stack template ~175-182)
- Test: `shirita-ui/src/views/ChatView.test.ts`

**Interfaces:**
- Consumes: `getSessionPanels`, `SessionPanel`.
- Produces: ChatView renders one `PanelView` per `SessionPanel`; actions gate on `panel.caps`.

- [ ] **Step 1: Write the failing test**

In `ChatView.test.ts`, mock `getSessionPanels` (add to the existing `vi.mock('../api/client', …)` block) to resolve `[{ id:'F', name:'Status', html:'<b>hi</b>', css:'', caps:{} }]`, mount the view, await ticks, and assert the panel stack renders:

```ts
expect(wrapper.find('[data-test="panel-stack"]').exists()).toBe(true)
expect(wrapper.html()).toContain('Status')
```

> Match how `ChatView.test.ts` already mocks `../api/client` and mounts the view; replace any old `getPack`/`meta.panel` panel setup with the `getSessionPanels` mock.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/views/ChatView.test.ts`
Expected: FAIL (still reads `pack.meta.panel`).

- [ ] **Step 3: Implement**

In `ChatView.vue` `<script setup>`:

```ts
import { getSessionPanels } from '../api/client'
import type { SessionPanel } from '../api/types'

const panels = ref<SessionPanel[]>([])
async function loadPanels() {
  try {
    panels.value = await getSessionPanels(sessionId)
  } catch {
    panels.value = []
  }
}

async function onPanelAction(panel: SessionPanel, action: PanelAction) {
  const caps = panel.caps || {}
  if (action.kind === 'diff') {
    if (!caps.write) return
    try {
      const res = await applyStateUpdates(sessionId, [{ action: action.op, key: action.key, value: action.value }])
      sessionState.value = { ...sessionState.value, values: res.values }
    } catch { /* stay on last good state */ }
  } else if (action.kind === 'insert') {
    if (caps.insert) composerRef.value?.setText(action.text)
  } else if (action.kind === 'send') {
    if (caps.send) await handleSend(action.text, [])
  }
}
```

Remove the old `panelPacks`, `panelOf`, and the `getPack`/`getSession`/`Pack`/`Panel` imports that only served panels (keep any still used elsewhere in the file). Update the template (~175-182):

```html
<div v-if="panels.length" data-test="panel-stack" class="flex flex-col gap-2 py-2">
  <details v-for="p in panels" :key="p.id" open class="rounded-xl border border-line bg-card/50 overflow-hidden">
    <summary class="px-3 py-1.5 text-[12px] text-muted cursor-pointer select-none">{{ p.name }}</summary>
    <PanelView :html="p.html" :css="p.css" :values="sessionState.values" @action="onPanelAction(p, $event)" />
  </details>
</div>
```

> Preserve the existing summary/markup styling; only the data source (`p.*` vs `panelOf(p).*`) and the summary label (`p.name`) change. `loadPanels()` is already called in `onMounted` and the route-change watcher — leave those call sites.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/views/ChatView.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/ChatView.vue shirita-ui/src/views/ChatView.test.ts
git commit -m "feat(ui-chat): render panels from GET /sessions/:id/panels"
```

---

### Task 8: DefinitionEditor — html preview + css editing

**Files:**
- Modify: `shirita-ui/src/components/DefinitionEditor.vue` (add an `html` preview block; ensure `html`/`css` use the generic content editor)
- Modify: locale files `en.ts`, `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts` (one new key)
- Test: `shirita-ui/src/components/DefinitionEditor.test.ts`

**Interfaces:**
- Consumes: existing generic content `<textarea>`, `PanelView`.
- Produces: editing an `html` definition shows a live `PanelView` preview of its content; `css` edits via the generic textarea. `html`/`css` are non-container types, so `isContainerType` is already false and the generic content editor renders for them.

- [ ] **Step 1: Write the failing test**

In `DefinitionEditor.test.ts`, mount the editor with an `html` definition and assert a preview renders:

```ts
it('shows a PanelView preview for html definitions', () => {
  const def = { id: 'h', type: 'html', name: 'm', content: '<b>hi</b>', meta: {} }
  const wrapper = mount(DefinitionEditor, { props: { definition: def, /* ...existing required props */ } })
  expect(wrapper.find('[data-test="html-preview"]').exists()).toBe(true)
})
```

> Fill the other required props the way existing `DefinitionEditor.test.ts` cases do.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/DefinitionEditor.test.ts`
Expected: FAIL (no `html-preview`).

- [ ] **Step 3: Implement**

Add the import (if absent) `import PanelView from './PanelView.vue'`, and after the generic content `<textarea>` block add:

```html
<div v-if="definition.type === 'html'" data-test="html-preview" class="mt-3">
  <span class="text-[12px] text-muted block mb-1">{{ $t('definition.htmlPreview') }}</span>
  <PanelView :html="definition.content" :css="''" :values="{}" />
</div>
```

Add the locale key `definition.htmlPreview` to all four files:
- `en.ts`: `htmlPreview: 'Preview'`
- `zh-Hans.ts`: `htmlPreview: '预览'`
- `zh-Hant.ts`: `htmlPreview: '預覽'`
- `ja.ts`: `htmlPreview: 'プレビュー'`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd shirita-ui && npx vitest run src/components/DefinitionEditor.test.ts src/locales`
Expected: PASS (component test + locale-parity test).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/DefinitionEditor.vue shirita-ui/src/components/DefinitionEditor.test.ts shirita-ui/src/locales
git commit -m "feat(ui-def): live preview for html bricks; css edits via content editor"
```

---

### Task 9: PromptTree "Add panel" scaffold + folder name/caps editing

**Files:**
- Modify: `shirita-ui/src/components/PromptTree.vue` (optional `allowPanel` prop + "Add panel" affordance → `add-panel` emit; surface `html`/`css` as creatable types)
- Modify: `shirita-ui/src/components/NodeRow.vue` (panel-folder name + caps editing → `update-node-meta`)
- Modify: `shirita-ui/src/components/PackEditor.vue` (handle `add-panel` by scaffolding folder + html + css)
- Modify: locale files (four) — new keys
- Test: `shirita-ui/src/components/PromptTree.test.ts` (or `PackEditor.test.ts` for the scaffold)

**Interfaces:**
- Consumes: `createDefinition`, `createNode`, `updateNode` (already imported in `PackEditor.vue`).
- Produces: `PromptTree` emits `add-panel` (only when `allowPanel` is true). `PackEditor` handles it by creating an `html` def + `css` def, a `panel` folder node (`meta = { name, caps: {} }`), and two child refs, then reloading. `NodeRow` lets a `panel` folder edit `meta.name` and `meta.caps.{write,insert,send}` via `update-node-meta`.

- [ ] **Step 1: Write the failing test**

In `PackEditor.test.ts`, mock the client functions and assert that triggering `add-panel` creates the bricks + folder. Match the file's existing mocking style:

```ts
it('Add panel scaffolds a panel folder with html and css bricks', async () => {
  // createDefinition mock returns sequential ids; createNode mock records calls
  const wrapper = mount(PackEditor, { props: { pack: samplePack }, global: { /* i18n etc. */ } })
  await wrapper.findComponent(PromptTree).vm.$emit('add-panel')
  await flushPromises()
  expect(createDefinitionMock).toHaveBeenCalledWith(expect.objectContaining({ type: 'html' }))
  expect(createDefinitionMock).toHaveBeenCalledWith(expect.objectContaining({ type: 'css' }))
  expect(createNodeMock).toHaveBeenCalledWith('pack', samplePack.id, expect.objectContaining({ kind: 'folder', tag: 'panel' }))
})
```

> Reuse the existing `PackEditor.test.ts` setup (sample pack, i18n stub, client mocks).

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/PackEditor.test.ts`
Expected: FAIL (no `add-panel` handler/emit).

- [ ] **Step 3: Implement**

In `PromptTree.vue`: add `allowPanel?: boolean` to props and `(e: 'add-panel'): void` to emits; render an "Add panel" button (near the existing add affordances) guarded by `v-if="allowPanel"` that calls `emit('add-panel')`. Also surface `html`/`css` in the creatable-type list (pass them through to `NodePicker`'s create path, mirroring how `prompt` is offered).

In `PackEditor.vue`: pass `:allow-panel="true"` to `<PromptTree>` and add the handler + wire `@add-panel`:

```ts
async function addPanel() {
  try {
    const html = await createDefinition({ type: 'html', name: 'Panel HTML', content: '', meta: {} })
    const css = await createDefinition({ type: 'css', name: 'Panel CSS', content: '', meta: {} })
    await library.loadDefinitions()
    const folder = await createNode('pack', props.pack.id, {
      parent_id: null, kind: 'folder', tag: 'panel', meta: { name: 'Panel', caps: {} },
    })
    await createNode('pack', props.pack.id, { parent_id: folder.id, kind: 'ref', definition_id: html.id })
    await createNode('pack', props.pack.id, { parent_id: folder.id, kind: 'ref', definition_id: css.id })
    await reload()
  } catch (e) { error.value = (e as Error).message }
}
```

> Confirm `createNode` returns the created node (so `folder.id` is available); if it returns void, fetch the folder id via `reload()` + lookup, or extend the API. The existing `createNode` calls in this file return a node-like value used elsewhere — follow that.

In `NodeRow.vue`: when a folder's `tag === 'panel'`, render a name `<input>` bound to `node.meta.name` and three caps checkboxes bound to `node.meta.caps.{write,insert,send}`, each emitting `update-node-meta(node.id, nextMeta)`. Reuse the cap labels `pack.capWrite/capInsert/capSend` already in the locales.

Add locale keys to all four files: `pack.addPanel` and `pack.panelName`:
- `en.ts`: `addPanel: 'Add panel'`, `panelName: 'Panel name'`
- `zh-Hans.ts`: `addPanel: '添加面板'`, `panelName: '面板名称'`
- `zh-Hant.ts`: `addPanel: '新增面板'`, `panelName: '面板名稱'`
- `ja.ts`: `addPanel: 'パネルを追加'`, `panelName: 'パネル名'`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd shirita-ui && npx vitest run src/components/PackEditor.test.ts src/components/PromptTree.test.ts src/components/NodeRow.test.ts src/locales`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/PromptTree.vue shirita-ui/src/components/NodeRow.vue shirita-ui/src/components/PackEditor.vue shirita-ui/src/locales shirita-ui/src/components/PackEditor.test.ts
git commit -m "feat(ui-tree): Add-panel scaffold + panel-folder name/caps editing"
```

---

### Task 10: Remove the dedicated Panel section from PackEditor

**Files:**
- Modify: `shirita-ui/src/components/PackEditor.vue` (delete panel html/css/caps/preview UI + `savePanel`/`panelHtml`/`panelCss`/`panelCaps`/`toggleCap`/`previewValues` panel state)
- Test: `shirita-ui/src/components/PackEditor.test.ts`

**Interfaces:**
- Produces: PackEditor no longer reads or writes `pack.meta.panel`. Panels are authored via the tree (Task 9). **The Variables section stays** (Plan 2 removes it).

- [ ] **Step 1: Write the failing test**

In `PackEditor.test.ts`:

```ts
it('no longer renders the legacy pack-panel section', () => {
  const wrapper = mount(PackEditor, { props: { pack: samplePack }, global: { /* ... */ } })
  expect(wrapper.find('[data-test="pack-panel"]').exists()).toBe(false)
  expect(wrapper.find('[data-test="panel-html"]').exists()).toBe(false)
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/PackEditor.test.ts`
Expected: FAIL (panel section still present).

- [ ] **Step 3: Implement**

In `PackEditor.vue`, delete: the `<!-- panel -->` template block (`<h3>…pack.panel…</h3>` through the closing `</div>` of `data-test="pack-panel"`, ~197-232), the `panelHtml`/`panelCss`/`panelCaps` refs and their seeding `watch`, `savePanel`, `toggleCap`, `previewValues`, and the now-unused `Panel`/`PanelCaps`/`PanelView` imports. Keep `import` of `VariablesEditor` and the Variables section, and keep identity + `PromptTree`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd shirita-ui && npx vitest run src/components/PackEditor.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/PackEditor.vue shirita-ui/src/components/PackEditor.test.ts
git commit -m "refactor(ui-pack): drop the legacy meta.panel editor; panels are tree bricks now"
```

---

### Task 11: Full-suite verification

**Files:** none (verification only).

- [ ] **Step 1: Backend**

Run: `cargo test --workspace`
Expected: PASS. Investigate and fix any remaining references to `meta.panel` (grep: `git grep -n "meta\\.panel\|meta\\[\"panel\"\]\|\"panel\":" -- 'shirita-core/*' 'shirita-web/*'`).

- [ ] **Step 2: Frontend**

Run: `cd shirita-ui && npx vitest run && npx vue-tsc --noEmit`
Expected: PASS + no type errors. Fix any leftover `pack.meta.panel` typings.

- [ ] **Step 3: Commit any fixups**

```bash
git add -A
git commit -m "test: green workspace + ui suites for panels-as-bricks"
```

---

## Out of scope (this plan)

- **Variables-as-bricks** — Plan 2: the `variables` def type, `resolve_schema_from_bricks`/`variables_from_nodes`/`resolve_session_schema`, and switching `charcard.rs` / `stpreset.rs` / `PackEditor.vue` / `BookView.vue` off `meta.variables`. Variables remain in `meta` here and keep working unchanged.
- **BookView "Add panel"** for templates. `resolve_session_panels` already reads template/session trees, so a template panel folder created by other means renders; the authoring button is added only to `PackEditor` this plan (gated by `PromptTree`'s `allowPanel`).
- Dropping the now-inert `pack.meta` column (no migration; leave it).

## Self-Review

- **Spec coverage:** §1 html/css reserved+non-rendering → Task 1. §2 panel folder + newline join → Task 2 (+ Task 9 authoring, Task 4 importer). §4.1 resolve_session_panels + endpoint → Tasks 2-3. §4.4 portable assets → Task 5. §4.5 charcard importer → Task 4. §5 frontend (ChatView/PackEditor/DefinitionEditor/tree, types/client) → Tasks 6-10. §4.2 variables + §4.6 + stpreset/BookView → explicitly deferred to Plan 2 (Out of scope). §6 tests → each task is TDD.
- **Placeholder scan:** none; every code step shows code. Frontend steps that depend on file-local idioms (test mock style, base-url helper) point at the exact neighbor to copy.
- **Type consistency:** `RenderedPanel`/`SessionPanel` fields (`id,name,html,css,caps`) match across core, web, and ui. `collect_panels`/`resolve_session_panels`/`getSessionPanels` names are stable across tasks. `add-panel` emit + `allowPanel` prop are consistent between Tasks 9 and the PackEditor handler.
