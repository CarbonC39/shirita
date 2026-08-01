# Variables-as-Bricks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make variable declarations first-class `variables` definition bricks referenced by the node tree, replacing the untyped `template.meta.variables` / `pack.meta.variables` god-object facets.

**Architecture:** A `variables` brick is a reserved, non-rendering `Definition` whose declarations live in `meta.decls: VarDecl[]`. Session schema resolution walks the effective template/session tree and each mounted pack tree, extracts decls from enabled `variables`-brick refs, and merges them (system ∪ template ∪ packs[mount order] ∪ session-local; later wins). All producers (charcard, stpreset, PackEditor, BookView) emit/author bricks instead of writing `*.meta.variables`. Session-local variables (`override_config.local_variables`) are unchanged.

**Tech Stack:** Rust (`shirita-core`, `shirita-web`, sqlx/SQLite, Axum), Vue 3 + TypeScript + Vitest.

## Global Constraints

- **No data migration** — testing phase, no users; clean break. Leave the `pack.meta` / `template.meta` columns in place (inert, default `{}`).
- **`variables` brick shape:** `Definition { type: "variables", name, content: "" (free-text note allowed), meta: { decls: VarDecl[] } }`. Declarations live in `meta.decls`, NOT `content`.
- **`VarDecl`** (existing, `shirita-core/src/state.rs`): `{ name: String, type: VarType, initial: Value, scope: Option<String> }` (serde: field `type`, snake_case). Frontend `VarDecl` already in `shirita-ui/src/api/types.ts`.
- **Reserved + non-rendering:** `variables` is reserved (never a user-created container) and non-rendering (never emitted into the LLM prompt) — the same treatment `html`/`css` already received in Plan 1.
- **Precedence on name collision:** later source wins — system < template-tree < packs (mount order) < session-local. Mirrors today's `merge_decls`.
- **Scope tagging preserved:** merged decls keep `scope` = `"system" | "template" | "pack" | "local"` exactly as today (the frontend `VariablesPanel` groups System vs Custom by it).
- **i18n:** `en` is the source schema; any key removed must be removed from all four locales (`en`, `zh-Hans`, `zh-Hant`, `ja`) — the parity test enforces it. Comments and commit messages in English.
- **Out of scope (explicit non-goals):** the panel-folder *combined* `PanelView` preview in `DefinitionEditor` (panel polish, not variables — defer); any priority/depth/weight system; flattening packs to bare definition-sets; portable envelope changes (variables bricks travel automatically as `definitions` + `nodes`, no asset scanning needed).

---

### Task 1: Reserve `variables` + mark non-rendering

**Files:**
- Modify: `shirita-core/src/models/def_type.rs:6-7` (extend `RESERVED`), test ~`def_type.rs:69`
- Modify: `shirita-core/src/assembly.rs:386-387` (`is_non_rendering`), test ~`assembly.rs:1395`

**Interfaces:**
- Produces: `def_type::is_reserved("variables") == true`; `assembly::is_non_rendering("variables") == true` (the single guard that keeps `variables` bricks out of the LLM prompt). Consumed by Tasks 2-3 (schema resolution treats them as the only schema source) and the frontend.

- [ ] **Step 1: Write the failing tests**

In `shirita-core/src/models/def_type.rs` tests, extend the existing reserved test (next to `html_css_are_reserved`):

```rust
    #[test]
    fn variables_is_reserved() {
        assert!(is_reserved("variables"));
    }
```

In `shirita-core/src/assembly.rs` tests, find the test that asserts `is_non_rendering("html")` (~line 1395) and add:

```rust
        assert!(is_non_rendering("variables"));
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shirita-core --lib def_type:: assembly::`
Expected: FAIL — `variables` not yet reserved / still renders.

- [ ] **Step 3: Implement**

In `shirita-core/src/models/def_type.rs`, change the `RESERVED` array (length 7 → 8):

```rust
pub const RESERVED: [&str; 8] =
    ["prompt", "regex_rule", "tool", "first_message", "protocol", "html", "css", "variables"];
```

In `shirita-core/src/assembly.rs:386-387`, extend the guard and its doc comment to list all five non-rendering types:

```rust
fn is_non_rendering(def_type: &str) -> bool {
    matches!(def_type, "regex_rule" | "first_message" | "html" | "css" | "variables")
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core --lib def_type:: assembly::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/models/def_type.rs shirita-core/src/assembly.rs
git commit -m "feat(def-type): reserve the variables brick type and mark it non-rendering"
```

---

### Task 2: Pure schema-from-bricks helpers in `state.rs`

**Files:**
- Modify: `shirita-core/src/state.rs` (add `variables_from_nodes`, `resolve_schema_from_bricks`, `tag_scope`, `decls_of`)
- Modify: `shirita-core/src/lib.rs:62-66` (re-export the two new public fns)
- Test: `shirita-core/src/state.rs` (tests module)

**Interfaces:**
- Consumes: `VarDecl`, `system_variables()`, `merge_decls()`, `parse_decls()` (all existing in `state.rs`); `PromptNode`, `NodeKind` (`crate::models::prompt_node`); `Definition` (`crate::models::definition`).
- Produces:
  - `pub fn variables_from_nodes(nodes: &[PromptNode], defs: &std::collections::HashMap<String, Definition>) -> Vec<VarDecl>` — extracts decls (scope `None`) from enabled `variables`-brick refs in one tree, in `sort_order`.
  - `pub fn resolve_schema_from_bricks(template_decls: Vec<VarDecl>, pack_decls: Vec<Vec<VarDecl>>, override_config: &serde_json::Value) -> Vec<VarDecl>` — system ∪ template ∪ packs (mount order) ∪ local; later wins; scope-tagged.
  - These replace `resolve_schema_with_packs` / `resolve_schema`, which **remain in place until Task 3** removes them and their callers together (so the crate stays green between tasks).

- [ ] **Step 1: Write the failing tests**

Add to the `state.rs` tests module:

```rust
    #[test]
    fn variables_from_nodes_reads_enabled_variables_bricks_in_order() {
        use crate::models::definition::Definition;
        use crate::models::prompt_node::{OwnerKind, PromptNode};
        use std::collections::HashMap;

        let mut a = Definition::new("variables", "A", "");
        a.id = "a".into();
        a.meta = json!({ "decls": [{ "name": "hp", "type": "number", "initial": 100 }] });
        let mut b = Definition::new("variables", "B", "");
        b.id = "b".into();
        b.meta = json!({ "decls": [{ "name": "mood", "type": "string", "initial": "calm" }] });
        let mut other = Definition::new("char", "C", "x"); // not a variables brick
        other.id = "c".into();
        let mut disabled = Definition::new("variables", "D", "");
        disabled.id = "d".into();
        disabled.meta = json!({ "decls": [{ "name": "secret", "type": "string", "initial": "x" }] });

        let r_b = PromptNode::new_ref(OwnerKind::Pack, "p", None, 1, "b");
        let r_a = PromptNode::new_ref(OwnerKind::Pack, "p", None, 0, "a");
        let r_c = PromptNode::new_ref(OwnerKind::Pack, "p", None, 2, "c");
        let mut r_d = PromptNode::new_ref(OwnerKind::Pack, "p", None, 3, "d");
        r_d.enabled = false;
        let nodes = vec![r_b, r_a, r_c, r_d];

        let mut defs = HashMap::new();
        for d in [a, b, other, disabled] {
            defs.insert(d.id.clone(), d);
        }

        let decls = variables_from_nodes(&nodes, &defs);
        let names: Vec<&str> = decls.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["hp", "mood"], "sort_order, only enabled variables bricks");
    }

    #[test]
    fn resolve_schema_from_bricks_merges_with_mount_order_precedence() {
        let template = vec![VarDecl { name: "hp".into(), var_type: VarType::Number, initial: json!(100), scope: None }];
        let pack_a = vec![VarDecl { name: "hp".into(), var_type: VarType::Number, initial: json!(50), scope: None }];
        let pack_b = vec![VarDecl { name: "gold".into(), var_type: VarType::Number, initial: json!(7), scope: None }];
        let cfg = json!({ "local_variables": [{ "name": "hp", "type": "number", "initial": 250 }] });

        let schema = resolve_schema_from_bricks(template, vec![pack_a, pack_b], &cfg);
        let hp = schema.iter().find(|d| d.name == "hp").unwrap();
        assert_eq!(hp.initial, json!(250), "local wins over template/packs");
        assert_eq!(hp.scope.as_deref(), Some("local"));
        let gold = schema.iter().find(|d| d.name == "gold").unwrap();
        assert_eq!(gold.scope.as_deref(), Some("pack"));
        // system variables ($avatar/$background) are always present
        assert!(schema.iter().any(|d| d.name == "$avatar"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shirita-core --lib state::tests::variables_from_nodes state::tests::resolve_schema_from_bricks`
Expected: FAIL — functions not defined.

- [ ] **Step 3: Implement**

At the top of `state.rs`, ensure these imports exist (add what's missing):

```rust
use std::collections::HashMap;

use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, PromptNode};
```

Add near `parse_decls` / `merge_decls` (~`state.rs:177`):

```rust
/// Parse a brick's `meta.decls` array into VarDecls (scope left `None`; the
/// caller tags scope when merging).
fn decls_of(meta: &Value) -> Vec<VarDecl> {
    meta.get("decls")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| serde_json::from_value::<VarDecl>(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn tag_scope(mut decls: Vec<VarDecl>, scope: &str) -> Vec<VarDecl> {
    for d in &mut decls {
        d.scope = Some(scope.to_string());
    }
    decls
}

/// Extract VarDecls from `variables` bricks referenced by enabled `ref` nodes in
/// one tree, in `sort_order`. Decls come from each brick's `meta.decls`; scope is
/// left `None` (the schema resolver tags it per source).
pub fn variables_from_nodes(nodes: &[PromptNode], defs: &HashMap<String, Definition>) -> Vec<VarDecl> {
    let mut refs: Vec<&PromptNode> =
        nodes.iter().filter(|n| n.kind == NodeKind::Ref && n.enabled).collect();
    refs.sort_by_key(|n| n.sort_order);
    let mut out = Vec::new();
    for n in refs {
        let Some(def) = n.definition_id.as_deref().and_then(|id| defs.get(id)) else {
            continue;
        };
        if def.def_type == "variables" {
            out.extend(decls_of(&def.meta));
        }
    }
    out
}

/// Resolve a session's effective schema from `variables` bricks: system ∪
/// template-tree decls ∪ each mounted pack's decls (mount order) ∪ session
/// `override_config.local_variables`. Later sources win on name collision.
pub fn resolve_schema_from_bricks(
    template_decls: Vec<VarDecl>,
    pack_decls: Vec<Vec<VarDecl>>,
    override_config: &Value,
) -> Vec<VarDecl> {
    let mut out = system_variables();
    merge_decls(&mut out, tag_scope(template_decls, "template"));
    for pd in pack_decls {
        merge_decls(&mut out, tag_scope(pd, "pack"));
    }
    merge_decls(&mut out, parse_decls(override_config.get("local_variables"), "local"));
    out
}
```

In `shirita-core/src/lib.rs`, add the two new fns to the `pub use state::{ ... }` block (keep `resolve_schema` for now — Task 3 removes it):

```rust
pub use state::{
    apply_updates, effective_state, parse_state_updates, resolve_schema, resolve_schema_from_bricks,
    schema_initials, variables_from_nodes,
    // ...keep the rest of the existing list unchanged...
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core --lib state::`
Expected: PASS (new tests green; existing `resolve_schema*` tests still green — old fns untouched).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/state.rs shirita-core/src/lib.rs
git commit -m "feat(state): pure variables_from_nodes + resolve_schema_from_bricks"
```

---

### Task 3: Cut session schema resolution over to bricks

**Files:**
- Modify: `shirita-core/src/conversation.rs` (replace private `session_schema` with `pub async fn resolve_session_schema`; move `load_defs` here as `pub(crate)`)
- Modify: `shirita-core/src/panels.rs` (use `crate::conversation::load_defs`, drop its private copy)
- Modify: `shirita-core/src/state.rs` (delete `resolve_schema_with_packs` / `resolve_schema` and their unit tests)
- Modify: `shirita-core/src/lib.rs` (drop `resolve_schema` re-export; re-export `resolve_session_schema`)
- Modify: `shirita-web/src/routes/variables.rs:7,31,82` and `shirita-web/src/routes/sessions.rs:13,77`
- Test: `shirita-core/src/conversation.rs` (tests module) — `resolve_session_schema`

**Interfaces:**
- Consumes: `variables_from_nodes`, `resolve_schema_from_bricks` (Task 2); `effective_nodes` (existing).
- Produces: `pub async fn resolve_session_schema(storage: &dyn Storage, session: &Session) -> Vec<VarDecl>` — the single source of truth for a session's schema, replacing `session_schema` (private) and `resolve_schema_with_packs` (deleted). `pub(crate) async fn load_defs(storage: &dyn Storage, nodes: &[PromptNode]) -> crate::Result<HashMap<String, Definition>>` shared by `panels.rs` and `conversation.rs`.

- [ ] **Step 1: Write the failing test**

Add to the `conversation.rs` tests module (it already has `MemStorage`/`InMemory`-style helpers used by `effective_nodes_prefers_session_else_template`; reuse the same storage test fixture). Model the test on that existing one — seed a template tree with a `variables` brick and assert the resolved schema:

```rust
    #[tokio::test]
    async fn resolve_session_schema_reads_template_variables_bricks() {
        // Reuse the same in-memory storage + template/session setup pattern as
        // effective_nodes_prefers_session_else_template (see that test for the
        // exact fixture constructor). Seed: a template with one `variables` brick
        // ref at root declaring `hp`, and a session pointing at that template.
        let (storage, session) = seed_template_with_variables_brick(&[
            ("hp", "number", json!(100)),
        ]).await;

        let schema = resolve_session_schema(storage.as_ref(), &session).await;
        assert!(schema.iter().any(|d| d.name == "hp" && d.scope.as_deref() == Some("template")));
        assert!(schema.iter().any(|d| d.name == "$avatar")); // system always present
    }
```

> Implementer note: if no reusable fixture constructor exists, write a small local helper that creates the in-memory `Storage`, a `Template`, a `variables` `Definition` (`meta.decls`), a root `ref` node owned by the template, and a `Session` with `template_id` set — mirroring the construction already used by neighboring `conversation.rs` async tests. Do not add mocks; use the real `Storage` test double the module already uses.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core --lib conversation::tests::resolve_session_schema`
Expected: FAIL — `resolve_session_schema` not defined.

- [ ] **Step 3: Implement**

In `conversation.rs`, move `load_defs` here (copy the body from `panels.rs:91-106`) as `pub(crate)`, and add `resolve_session_schema`; delete the private `session_schema` (lines ~23-35):

```rust
/// Load the definitions referenced by a node tree, de-duplicated by id.
pub(crate) async fn load_defs(
    storage: &dyn Storage,
    nodes: &[PromptNode],
) -> crate::Result<std::collections::HashMap<String, Definition>> {
    let mut defs = std::collections::HashMap::new();
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

/// Resolve a session's effective variable schema from `variables` bricks across
/// the effective template/session tree and each mounted pack (mount order).
pub async fn resolve_session_schema(storage: &dyn Storage, session: &Session) -> Vec<VarDecl> {
    let nodes = effective_nodes(storage, session).await.unwrap_or_default();
    let defs = load_defs(storage, &nodes).await.unwrap_or_default();
    let template_decls = crate::state::variables_from_nodes(&nodes, &defs);

    let mut pack_decls = Vec::new();
    for pid in &session.mounted_packs {
        let pnodes = storage.list_nodes(&OwnerKind::Pack, pid).await.unwrap_or_default();
        let pdefs = load_defs(storage, &pnodes).await.unwrap_or_default();
        pack_decls.push(crate::state::variables_from_nodes(&pnodes, &pdefs));
    }
    crate::state::resolve_schema_from_bricks(template_decls, pack_decls, &session.override_config)
}
```

Add the imports `conversation.rs` needs: `VarDecl` (`crate::state::VarDecl`) and `Definition` (`crate::models::definition::Definition`) if not already imported.

Replace the three internal call sites — `let schema = session_schema(...)` at lines ~216, ~348, ~463 — with `resolve_session_schema`:

```rust
    let schema = resolve_session_schema(storage, session).await;          // ~216 (storage: &dyn Storage)
```
```rust
    let schema = resolve_session_schema(storage.as_ref(), &session).await; // ~348 and ~463 (storage: Arc<dyn Storage>)
```

In `panels.rs`, delete its private `load_defs` (lines 91-106) and call the shared one — replace the two `load_defs(storage, ...)` calls in `resolve_session_panels` with `crate::conversation::load_defs(storage, ...)`.

In `state.rs`, delete `resolve_schema_with_packs` (lines ~189-204), `resolve_schema` (lines ~206-209), and their unit tests (the two `#[test]`s using `resolve_schema_with_packs` ~lines 289/298 and the two `resolve_schema_*` tests ~408/424). The now-unused private `parse_decls` is still used by `resolve_schema_from_bricks` (local_variables) — keep it.

In `lib.rs`: remove `resolve_schema` from the `pub use state::{...}` list, and add `resolve_session_schema` to the `pub use conversation::{...}` list (line ~33).

In `shirita-web/src/routes/variables.rs`: change the import (line 7) to drop `resolve_schema_with_packs`, and replace both schema computations (lines ~31 and ~82) — they currently load `template_meta`/`pack_metas` then call `resolve_schema_with_packs`. Replace each with:

```rust
    let schema = shirita_core::conversation::resolve_session_schema(state.storage.as_ref(), &session).await;
```

Delete the now-dead `template_meta` / `pack_metas` gathering blocks that fed the old call in each handler (keep any of that fetch that is still used elsewhere in the handler — verify before deleting).

In `shirita-web/src/routes/sessions.rs`: change the import (line 13) to drop `resolve_schema_with_packs` (keep `schema_initials`), and replace line ~77:

```rust
    let schema = shirita_core::conversation::resolve_session_schema(state.storage.as_ref(), &session).await;
    session.current_state = Value::Object(schema_initials(&schema));
```

Delete the now-dead `template_meta`/`pack_metas` gathering that only fed the old call (verify they aren't used by the avatar-resolution block just above — `pack_identities` is separate; check `pack_metas` specifically).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core --lib conversation:: state:: panels::` then `cargo build -p shirita-web`
Expected: PASS; web crate compiles. Then `cargo test -p shirita-web` for the variables/sessions route tests.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/conversation.rs shirita-core/src/panels.rs shirita-core/src/state.rs shirita-core/src/lib.rs shirita-web/src/routes/variables.rs shirita-web/src/routes/sessions.rs
git commit -m "refactor(state): resolve session schema from variables bricks, drop meta path"
```

---

### Task 4: charcard importer emits `variables` bricks

**Files:**
- Modify: `shirita-core/src/adapters/charcard.rs:413-453` (variables emission + panel folder)
- Test: `shirita-core/src/adapters/charcard.rs` tests (~line 928 region, and the panel-folder test)

**Interfaces:**
- Consumes: `tavern_helper_vardecls(data)` (existing), `PanelConversion.var_decls` (existing), `Definition`, `PromptNode`, the local `next(&mut sort)` / `next(&mut csort)` counters.
- Produces: a root `variables` brick for card-level (tavern_helper) vars; a `variables` brick **inside** the panel folder for status-bar capture fields. No `template.meta.variables` is written.

- [ ] **Step 1: Update the failing tests**

In `charcard.rs` tests, find the test that asserts `ls.template.meta["variables"]` (~line 933) and rewrite it to assert a root `variables` brick instead:

```rust
        // Card-level (tavern_helper) variables become a root `variables` brick,
        // not template meta.
        let vbrick = ls
            .definitions
            .iter()
            .find(|d| d.def_type == "variables" && d.name.ends_with("·vars"))
            .expect("a root variables brick");
        let decls = vbrick.meta["decls"].as_array().unwrap();
        let names: Vec<&str> = decls.iter().map(|d| d["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"field1") && names.contains(&"mood"));
        assert!(ls.template.meta.get("variables").is_none(), "no meta.variables god-object");
```

In the panel-folder test (`charcard_to_loreset_emits_panel_folder_for_unambiguous_status_bar`), add an assertion that the panel folder also holds a `variables` child brick carrying the status-bar fields:

```rust
        let panel_vars = ls
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Ref && n.parent_id.as_deref() == Some(folder.id.as_str()))
            .filter_map(|n| n.definition_id.as_deref())
            .filter_map(|id| ls.definitions.iter().find(|d| d.id == id))
            .find(|d| d.def_type == "variables")
            .expect("panel folder has a variables child brick");
        assert!(panel_vars.meta["decls"].as_array().unwrap().iter().any(|d| d["name"] == "field1"));
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shirita-core --lib adapters::charcard`
Expected: FAIL — still writing `meta.variables`, no variables bricks.

- [ ] **Step 3: Implement**

Replace the merged-variables block (`charcard.rs:413-429`, the `let mut vardecls = ...` through `tmpl.meta = ...`) with a root variables brick for card-level vars only:

```rust
    // --- card-level (tavern_helper) variables → a root `variables` brick ---
    let card_vars = tavern_helper_vardecls(data);
    if !card_vars.is_empty() {
        let mut vdef = Definition::new("variables", format!("{name}·vars"), "");
        vdef.meta = serde_json::json!({ "decls": card_vars });
        nodes.push(PromptNode::new_ref(
            OwnerKind::Template, &tmpl.id, None, next(&mut sort), &vdef.id));
        defs.push(vdef);
    }
```

In the panel-folder block (`charcard.rs:432-453`), after the `css` brick and before the `panel_regex_def_id` ref, add the status-fields variables brick:

```rust
        if !conv.var_decls.is_empty() {
            let mut vdef = Definition::new("variables", format!("{name}·panel·vars"), "");
            vdef.meta = serde_json::json!({ "decls": conv.var_decls.clone() });
            nodes.push(PromptNode::new_ref(
                OwnerKind::Template, &tmpl.id, Some(folder.id.clone()), next(&mut csort), &vdef.id));
            defs.push(vdef);
        }
```

(The `tmpl.meta` assignment is gone; `tmpl.meta` stays its default `{}`.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core --lib adapters::charcard`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/adapters/charcard.rs
git commit -m "feat(import): charcard emits variables bricks (root + in-panel) not meta.variables"
```

---

### Task 5: stpreset importer emits a root `variables` brick

**Files:**
- Modify: `shirita-core/src/adapters/stpreset.rs:227-229` (drop `tmpl.meta`) + end of `stpreset_to_loreset` (~line 449, before `LoreSet {`)
- Test: `shirita-core/src/adapters/stpreset.rs` tests (~line 691)

**Interfaces:**
- Consumes: the local `vars: Vec<VarDecl>` (computed at ~line 212-226), `nodes`/`defs` accumulators, `Definition`, `PromptNode`.
- Produces: a single root `variables` brick (`meta.decls = vars`) when the preset declared any `{{setvar}}` variables; no `template.meta.variables`.

- [ ] **Step 1: Update the failing test**

In `stpreset.rs` tests, find `setvar_registers_variables_and_emits_no_node_when_emptied` (~line 677) and rewrite the meta assertion (~line 691) to read the brick:

```rust
        // Variables register as a root `variables` brick, not template meta.
        let vbrick = ls
            .definitions
            .iter()
            .find(|d| d.def_type == "variables")
            .expect("a variables brick");
        let names: Vec<&str> = vbrick.meta["decls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"vars"));
        assert!(ls.template.meta.get("variables").is_none());
```

> Implementer note: confirm the variable name the existing fixture expects (the test body around line 689 documents it as "vars"); use whatever name that fixture actually declares.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core --lib adapters::stpreset`
Expected: FAIL — still on `meta.variables`.

- [ ] **Step 3: Implement**

Delete the `tmpl.meta` assignment at `stpreset.rs:227-229`:

```rust
    if !vars.is_empty() {
        tmpl.meta = serde_json::json!({ "variables": vars });
    }
```

At the end of `stpreset_to_loreset`, immediately before `LoreSet { template: tmpl, definitions: defs, nodes }` (~line 449), append:

```rust
    // Variables declared via {{setvar}} macros → one root `variables` brick.
    if !vars.is_empty() {
        let mut vdef = Definition::new("variables", "Variables", "");
        vdef.meta = serde_json::json!({ "decls": vars });
        let sort = nodes.iter().map(|n| n.sort_order).max().unwrap_or(-1) + 1;
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, sort, &vdef.id));
        defs.push(vdef);
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p shirita-core --lib adapters::stpreset`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/adapters/stpreset.rs
git commit -m "feat(import): stpreset emits a root variables brick not meta.variables"
```

---

### Task 6: Frontend types for `variables` bricks

**Files:**
- Modify: `shirita-ui/src/api/types.ts`
- Test: none (type-only; covered by `vue-tsc` in later tasks)

**Interfaces:**
- Produces: `VariablesMeta { decls: VarDecl[] }` exported for editors to cast a `variables` definition's `meta`. `VarDecl` already exists in this file.

- [ ] **Step 1: Implement**

In `shirita-ui/src/api/types.ts`, near the `VarDecl` definition, add:

```ts
/** A `variables` brick's meta payload: its declared variables. */
export interface VariablesMeta {
  decls: VarDecl[]
}
```

- [ ] **Step 2: Verify typecheck**

Run: `cd shirita-ui && npx vue-tsc --noEmit`
Expected: exit 0.

- [ ] **Step 3: Commit**

```bash
git add shirita-ui/src/api/types.ts
git commit -m "feat(ui-api): VariablesMeta type for variables bricks"
```

---

### Task 7: DefinitionEditor edits `variables` bricks

**Files:**
- Modify: `shirita-ui/src/components/DefinitionEditor.vue` (imports, `isContainerType`/`showWrapInTag`, content block, new variables block)
- Modify: `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` (one key)
- Test: `shirita-ui/src/components/DefinitionEditor.test.ts`

**Interfaces:**
- Consumes: `VariablesEditor.vue` (props `modelValue: VarDecl[]`, emits `update:modelValue`), `VarDecl`/`VariablesMeta` (Task 6), the existing `update:meta` emit.
- Produces: for a `variables` definition, the editor shows a `VariablesEditor` bound to `meta.decls` (and hides the raw content textarea + wrap-in-tag). `data-test="variables-editor"`.

- [ ] **Step 1: Write the failing test**

Add to `DefinitionEditor.test.ts` (mirror the existing html-preview test's mount/setup):

```ts
  it('shows a variables editor bound to meta.decls for variables bricks', () => {
    const wrapper = mountEditor({
      id: 'v1', type: 'variables', name: 'Vars', content: '',
      meta: { decls: [{ name: 'hp', type: 'number', initial: 100 }] },
    })
    expect(wrapper.find('[data-test="variables-editor"]').exists()).toBe(true)
    // the raw content textarea is not shown for variables bricks
    expect(wrapper.find('textarea').exists()).toBe(false)
  })
```

> Implementer note: use whatever mount helper the file already uses (`mountEditor`/inline `mount` with the i18n + stubs global config). If `VariablesEditor` is heavy to render, stub it the way other child components are stubbed in this test file and assert on the stubbed element.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/DefinitionEditor.test.ts`
Expected: FAIL — no variables editor; textarea still present.

- [ ] **Step 3: Implement**

In `DefinitionEditor.vue` script, import `VariablesEditor` and the types:

```ts
import VariablesEditor from './VariablesEditor.vue'
import type { VarDecl } from '../api/types'
```

Exclude `variables` from container + wrap UI (lines 49 and 53):

```ts
const isContainerType = computed(() => !['prompt', 'regex_rule', 'tool', 'first_message', 'html', 'css', 'variables'].includes(props.definition.type))
const showWrapInTag = computed(() => !['regex_rule', 'first_message', 'html', 'css', 'variables'].includes(props.definition.type))
```

Add a computed for the decls:

```ts
const decls = computed<VarDecl[]>(() => ((props.definition.meta as Record<string, unknown>).decls as VarDecl[]) ?? [])
function saveDecls(next: VarDecl[]) {
  emit('update:meta', { ...props.definition.meta, decls: next })
}
```

In the template, gate the content block (the `<!-- content -->` `<div class="relative">` at lines 287-297) so it does not render for `variables`:

```html
    <!-- content (free-text payload); variables bricks declare in meta.decls instead -->
    <div v-if="definition.type !== 'variables'" class="relative">
```

Add, right after that content block, a variables editor block:

```html
    <!-- variables brick: declarations live in meta.decls -->
    <div v-if="definition.type === 'variables'" data-test="variables-editor" class="mt-1">
      <span class="text-[12px] text-muted block mb-1">{{ $t('definition.variablesDecls') }}</span>
      <VariablesEditor :model-value="decls" @update:model-value="saveDecls" />
    </div>
```

Add the i18n key `definition.variablesDecls` to all four locales (place next to `definition.htmlPreview`):
- `en.ts`: `variablesDecls: 'Variables',`
- `zh-Hans.ts`: `variablesDecls: '变量',`
- `zh-Hant.ts`: `variablesDecls: '變數',`
- `ja.ts`: `variablesDecls: '変数',`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd shirita-ui && npx vitest run src/components/DefinitionEditor.test.ts && npx vitest run src/locales/parity.test.ts && npx vue-tsc --noEmit`
Expected: PASS + parity green + typecheck clean.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/DefinitionEditor.vue shirita-ui/src/locales/
git commit -m "feat(ui-def): edit variables bricks via VariablesEditor on meta.decls"
```

---

### Task 8: `variables` creatable in the tree

**Files:**
- Modify: `shirita-ui/src/components/PromptTree.vue` (brick types: add `variables`; offer at root + in panel folders)
- Test: `shirita-ui/src/components/PromptTree.test.ts`

**Interfaces:**
- Consumes: the existing `createNewInContainer` / `createNewPrompt` / `create-new` plumbing (NodePicker emits `createNew(type)`; `createNewInContainer(parentId, typeId)` already creates `createDefinition({ type: typeId, ... })` in `PackEditor`/`BookView`).
- Produces: a user can create a `variables` brick at the tree root and inside a `panel` folder. The synthetic-DefType pattern matches the existing `panelPickerTypes` (Plan 1).

- [ ] **Step 1: Write the failing test**

Add to `PromptTree.test.ts` (mirror the existing panel/picker tests' mount + props):

```ts
  it('offers variables as a creatable brick type at the root', async () => {
    const wrapper = mountTree({ nodes: [], allowPanel: true })
    await openRootPicker(wrapper) // however the existing tests open the root omnibox
    expect(wrapper.text()).toContain('Variables')
    // and emits create-new with the variables type when chosen
    await wrapper.find('[data-test="create-variables"]').trigger('click')
    expect(wrapper.emitted('createNewPrompt') || wrapper.emitted('createNewInContainer') || wrapper.emitted('create-new')).toBeTruthy()
  })
```

> Implementer note: match the assertion to whatever event/flow the root omnibox actually uses to create a typed brick (study how `prompt` creation is wired in this file before writing the test). The hard requirement: choosing "Variables" at the root results in a `variables` definition being created (a `createNew`/`createNewInContainer` emission carrying `type === 'variables'`). Add the `data-test="create-variables"` hook to whatever control you add.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/PromptTree.test.ts`
Expected: FAIL — variables not creatable.

- [ ] **Step 3: Implement**

In `PromptTree.vue`, add `variables` to the panel-folder brick set (line ~52) so it can be created inside a panel folder, and surface a brick-type list at the root. Reuse the synthetic-DefType pattern already used by `panelPickerTypes` (lines 59-60):

```ts
const panelBrickTypes = ['html', 'css', 'variables', 'regex_rule']
```

Add a root-level brick-type list and merge it into the root omnibox's creatable types (next to `availableTypes`):

```ts
const brickTypes = ['variables']
const rootBrickPickerTypes = computed<DefType[]>(() =>
  brickTypes.map((id) => ({ id, label: id === 'variables' ? 'Variables' : id.toUpperCase(), sort: 0, builtin: true, created_at: '' })),
)
```

Render these synthetic types in the root omnibox alongside `availableTypes`, wiring a click to the same path container types use (so `createNewInContainer`/`createNewPrompt` fires with `type === 'variables'`). Add `data-test="create-variables"` to the variables entry.

> Implementer note: keep the change minimal and consistent with the existing root-omnibox markup; do not restructure the picker. If the cleanest insertion point is to append `rootBrickPickerTypes` to the same `v-for` that renders container types, do that. The panel-folder picker already routes through `panelPickerTypes` (line 196) — extend `panelBrickTypes` only; `panelPickerTypes` maps from it automatically.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd shirita-ui && npx vitest run src/components/PromptTree.test.ts && npx vue-tsc --noEmit`
Expected: PASS + typecheck clean.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/PromptTree.vue shirita-ui/src/components/PromptTree.test.ts
git commit -m "feat(ui-tree): make variables a creatable brick (root + panel folder)"
```

---

### Task 9: BookView drops the template-meta Variables section

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue` (remove `templateVars`/`saveTemplateVars` + the global Variables section ~line 1066; KEEP the this-chat local vars section ~line 909)
- Modify: `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` (prune `book.variables` if orphaned; KEEP `book.variablesThisChat`)
- Test: `shirita-ui/src/views/BookView.test.ts` (if it asserts the template-vars section)

**Interfaces:**
- Produces: BookView no longer reads/writes `template.meta.variables`. Template variables are authored as a `variables` brick in the template `PromptTree` (Task 8). Session-local variables (`override_config.local_variables`) are unchanged.

- [ ] **Step 1: Write/adjust the failing test**

In `BookView.test.ts`, add (or adapt an existing template-section test) a negative assertion. First check the current markup for the section's `data-test` or heading; assert the global template-variables editor is gone while the this-chat one remains:

```ts
  it('no longer renders the template-meta variables editor', async () => {
    const wrapper = await mountBook(/* existing template-selected fixture */)
    // global "Variables" (template meta) section is gone
    expect(wrapper.findAll('[data-test="template-variables"]').length).toBe(0)
    // the per-chat local variables section still renders
    expect(wrapper.find('[data-test="local-variables"]').exists()).toBe(true)
  })
```

> Implementer note: the section may not have these exact `data-test`s today. Before writing the test, read the two `<VariablesEditor>` usages (~lines 909-910 local, ~1066-1067 template). Add a `data-test="template-variables"` to the template section *only to delete it*, or assert via the section heading text. Add `data-test="local-variables"` to the kept local section if absent so the test is stable.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/views/BookView.test.ts`
Expected: FAIL — template variables section still present.

- [ ] **Step 3: Implement**

In `BookView.vue`:
- Delete `templateVars` (computed ~254-257) and `saveTemplateVars` (~258-262).
- Delete the global template Variables section in the template editing block (the `<h3>…$t('book.variables')…</h3>` + its `<VariablesEditor :model-value="templateVars" @update:model-value="saveTemplateVars" />`, ~line 1066-1067).
- Keep the this-chat section (`book.variablesThisChat` + `localVars`/`saveLocalVars`, ~909-910).
- If `templateVars`/`saveTemplateVars` were the only users of any import, remove the now-unused import.

Prune the `book.variables` i18n key from all four locales **iff** no other component references `book.variables` (grep first: `grep -rn "book\.variables'" src` — note `book.variablesThisChat` is a different key and stays).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd shirita-ui && npx vitest run src/views/BookView.test.ts && npx vitest run src/locales/parity.test.ts && npx vue-tsc --noEmit`
Expected: PASS + parity green + typecheck clean.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/BookView.vue shirita-ui/src/locales/
git commit -m "refactor(ui-book): drop template meta.variables editor; vars are tree bricks"
```

---

### Task 10: PackEditor drops the Variables section

**Files:**
- Modify: `shirita-ui/src/components/PackEditor.vue` (remove `packVars`/`saveVars` + the Variables section + `VariablesEditor` import)
- Modify: `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` (prune `pack.variables` if orphaned)
- Test: `shirita-ui/src/components/PackEditor.test.ts`

**Interfaces:**
- Produces: PackEditor collapses to identity + content tree only. Pack variables are authored as `variables` bricks in the pack `PromptTree` (Task 8). No reads/writes of `pack.meta.variables`.

- [ ] **Step 1: Write the failing test**

Add to `PackEditor.test.ts` (mirror the Task-10/Plan-1 negative-assertion test style):

```ts
  it('no longer renders the pack variables section', () => {
    const wrapper = mount(PackEditor, { props: { pack: samplePack }, global: { /* existing */ } })
    expect(wrapper.find('[data-test="pack-variables"]').exists()).toBe(false)
  })
```

> Implementer note: the Variables section (`PackEditor.vue:177-179`) currently has no `data-test`. Add `data-test="pack-variables"` to the section wrapper only so this test is meaningful, then delete the section in Step 3 (the test then asserts absence). Mirror how the Plan-1 `pack-panel` negative test was written.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/PackEditor.test.ts`
Expected: FAIL — variables section present.

- [ ] **Step 3: Implement**

In `PackEditor.vue`:
- Delete `packVars` (computed ~43-46) and `saveVars` (~47-49).
- Delete the `<!-- variables -->` section (~177-179: the `<h3>…$t('pack.variables')…</h3>` and `<VariablesEditor … />`).
- Remove the now-unused `import VariablesEditor from './VariablesEditor.vue'` (line 11) and drop `VarDecl` from the `../api/types` import if it becomes unused.
- Keep identity, the `PromptTree` (with `:allow-panel="true"` and all its wiring), and `save()`/`updateDisplayName`/`updateAvatar`.

Prune the `pack.variables` i18n key from all four locales iff unused elsewhere (grep: `grep -rn "pack\.variables'" src`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd shirita-ui && npx vitest run src/components/PackEditor.test.ts && npx vitest run src/locales/parity.test.ts && npx vue-tsc --noEmit`
Expected: PASS + parity green + typecheck clean.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/PackEditor.vue shirita-ui/src/locales/
git commit -m "refactor(ui-pack): drop meta.variables editor; vars are tree bricks now"
```

---

### Task 11: Full-suite verification

**Files:** none (verification + cleanup of any leftovers).

- [ ] **Step 1: Backend**

Run: `cargo test --workspace`
Expected: PASS. Then grep for any remaining `meta.variables` producers/readers and fix:

```bash
git grep -n 'meta\.variables\|meta\["variables"\]\|"variables":\|"variables"\.to_string' -- 'shirita-core/*' 'shirita-web/*'
```

Expected after fixes: only `local_variables` (session-local, intentional) and the `variables` *brick* `meta.decls` usages remain — no `*.meta.variables` god-object reads/writes.

Run: `cargo clippy --workspace` — confirm no **new** warnings versus the pre-existing set (`charcard.rs:204` boolean-simplify, `stpreset.rs:314/410` loop-index, `conversation.rs` too-many-args, `embed.rs` `is_reserved_prefix`). Fix any warning that points at code this plan added.

- [ ] **Step 2: Frontend**

Run: `cd shirita-ui && npx vitest run && npx vue-tsc --noEmit`
Expected: PASS + no type errors. Then grep for residual meta-variable readers and fix:

```bash
cd shirita-ui && grep -rn "meta\.variables\|\.meta as.*variables\|\['variables'\]" src
```

Expected: no component reads `pack.meta.variables` / `template.meta.variables` (only `override_config.local_variables` for the this-chat section, and `meta.decls` for variables bricks).

- [ ] **Step 3: Commit any fixups**

```bash
git add -A
git commit -m "test: green workspace + ui suites for variables-as-bricks"
```

---

## Out of scope (this plan)

- The panel-folder **combined** `PanelView` preview in `DefinitionEditor`/`NodeRow` (panel polish; deferred — Plan 1 left it out too).
- Any priority/depth/weight system; flattening packs to bare definition-sets.
- Portable envelope changes — `variables` bricks travel as `definitions` + `nodes` automatically; they carry no `/assets/` so `collect_pack_assets`/`rewrite_pack_assets` need no change.
- Dropping the now-inert `pack.meta` / `template.meta` columns (no migration; leave them).

## Self-Review

- **Spec coverage (against `2026-06-23-pack-bricks-redesign-design.md`):** §1 `variables` reserved+non-rendering → Task 1. §4.2 `variables_from_nodes` + `resolve_schema_from_bricks` → Task 2; `resolve_session_schema` replacing `session_schema` + `get_state` unification → Task 3; producers switch (charcard → Task 4, stpreset → Task 5, PackEditor → Task 10, BookView → Task 9). §4.5 charcard variables bricks → Task 4. §4.6 `pack.meta` left inert → Global Constraints + Task 3 (no writes). §5 frontend (DefinitionEditor variables block → Task 7; NodePicker/PromptTree creatable variables → Task 8; PackEditor collapse → Task 10; types → Task 6). §6 tests → each task is TDD. Note: §4.2 also names `routes/sessions.rs` indirectly (session-create seed) — covered in Task 3 (the third call site found in code, beyond the spec's explicit list).
- **Placeholder scan:** code shown for every code step. Steps that depend on file-local idioms (test mount helpers, root-omnibox markup, BookView section markers) point at the exact neighbor to copy and state the hard requirement, because those test scaffolds differ per file and must mirror existing local patterns rather than invent new ones.
- **Type consistency:** `variables_from_nodes(nodes, defs)` / `resolve_schema_from_bricks(template_decls, pack_decls, override_config)` / `resolve_session_schema(storage, session)` names and signatures are stable across Tasks 2-3 and their web callers. `meta.decls: VarDecl[]` is the brick shape used identically by charcard (T4), stpreset (T5), DefinitionEditor (T7), and `variables_from_nodes` (T2). `VariablesMeta` (T6) matches. The Task-3 cutover removes `resolve_schema`/`resolve_schema_with_packs` in the same commit as all their callers, so the crate never references a deleted symbol.
