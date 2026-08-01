# Pack/Preset Separation — Plan 2: Assembly & Scoping

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make mounted packs actually affect generation — their content flows into the template's `<<content>>` node (grouped by type, shared world-info activation), their regex rules join the pipeline, and their variable schemas merge in.

**Architecture:** Add `_with_packs` variants of the two pure resolvers (`assemble_from_nodes`, `resolve_schema`) so existing callers/tests and `shirita-web` stay untouched (the old names become thin wrappers passing empty packs). `conversation.rs::assemble_request` loads the session's mounted-pack node trees + defs and calls the new variants; `effective_regex_rules` and `session_schema` gain a mounted-pack pass.

**Tech Stack:** Rust, `shirita-core` only. Pure functions unit-tested in `assembly.rs`/`state.rs`; storage-backed paths tested in `conversation.rs` (uses the existing `RecordingProvider` to capture the assembled `ChatRequest`).

## Global Constraints

- Code comments and git commit messages in **English**.
- This plan touches **only `shirita-core`**. Do not edit `shirita-web`/`shirita-ui`/`shirita-tauri`. (The `shirita-web` callers of `resolve_schema` keep compiling because the old signature is preserved as a wrapper; they gain pack variables in Plan 3.)
- Preserve existing public signatures `assemble_from_nodes(...)` and `resolve_schema(...)` — add new `_with_packs` functions instead of changing them.
- **Identity pack-aware is NOT in this plan.** Spec §16.2 listed it here, but `resolve_identity` is invoked from the web layer; it is re-sliced into Plan 3 (API) alongside the session-pack endpoints and new-chat wiring. Generation (prompt assembly) does not use identity.
- Every task ends green: `cargo test --workspace`, zero warnings.
- Consumes Plan 1: `OwnerKind::Pack`, `NodeKind::Content`, `Pack`/`Storage::get_pack`/`list_nodes(Pack,..)`, `Session.mounted_packs`.

---

### Task 1: `<<content>>` assembly — pack content grouped by type

**Files:**
- Modify: `shirita-core/src/assembly.rs` (function `assemble_from_nodes`, ~line 416; the `NodeKind::Content` arm, ~line 481)
- Test: `shirita-core/src/assembly.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub fn assemble_from_nodes_with_packs(nodes: &[PromptNode], pack_trees: &[Vec<PromptNode>], definitions: &HashMap<String, Definition>, overrides: &serde_json::Value, state: &serde_json::Value, recent_msgs: &[String], roll: &mut impl FnMut() -> f64) -> AssembledPlan`. `assemble_from_nodes(...)` becomes a wrapper delegating with `&[]`. Mounted packs render at the `content` node, grouped by `definition.type` (one `<type>…</type>` segment each, mount order then first-appearance type order), honoring `select=one` folders; pack refs share the single `activate()` pass.

- [ ] **Step 1: Write the failing test** — append inside `#[cfg(test)] mod tests` in `shirita-core/src/assembly.rs`:

```rust
    #[test]
    fn content_node_injects_packs_grouped_by_type_with_select_one() {
        use std::collections::HashMap;
        // template: content node (sort 0) then history (sort 1)
        let mut content = PromptNode::new_folder(OwnerKind::Template, "t", None, 0, "content");
        content.kind = NodeKind::Content;
        content.tag = None;
        let mut hist = PromptNode::new_folder(OwnerKind::Template, "t", None, 1, "history");
        hist.kind = NodeKind::History;
        hist.tag = None;
        let tmpl = vec![content, hist];

        // pack: ref char "Alice profile" + a select=one folder of two char variants
        let mut alice = PromptNode::new_ref(OwnerKind::Pack, "p", None, 0, "d_alice");
        let mut mood = PromptNode::new_folder(OwnerKind::Pack, "p", None, 1, "mood");
        mood.tag = None;
        mood.meta = serde_json::json!({ "select": "one" });
        let happy = PromptNode::new_ref(OwnerKind::Pack, "p", Some(mood.id.clone()), 0, "d_happy");
        let angry = PromptNode::new_ref(OwnerKind::Pack, "p", Some(mood.id.clone()), 1, "d_angry");
        let pack = vec![alice.clone(), mood, happy, angry];

        let mut defs: HashMap<String, Definition> = HashMap::new();
        for (id, body) in [("d_alice", "Alice profile"), ("d_happy", "Happy Alice"), ("d_angry", "Angry Alice")] {
            let mut d = Definition::new("char", id, body);
            d.id = id.to_string();
            defs.insert(d.id.clone(), d);
        }
        let _ = &mut alice;

        let plan = assemble_from_nodes_with_packs(
            &tmpl, std::slice::from_ref(&pack), &defs,
            &serde_json::json!({}), &serde_json::json!({}), &[], &mut || 0.0,
        );
        let char_seg = plan.segments.iter().find(|s| s.source == "pack:char").expect("a char content segment");
        assert_eq!(char_seg.placement, Placement::BeforeHistory);
        assert!(char_seg.content.starts_with("<char>") && char_seg.content.ends_with("</char>"));
        assert!(char_seg.content.contains("Alice profile"));
        assert!(char_seg.content.contains("Happy Alice"));
        assert!(!char_seg.content.contains("Angry Alice"), "select=one keeps only the first child");
    }

    #[test]
    fn empty_pack_trees_render_no_content_segments() {
        use std::collections::HashMap;
        let mut content = PromptNode::new_folder(OwnerKind::Template, "t", None, 0, "content");
        content.kind = NodeKind::Content;
        content.tag = None;
        let plan = assemble_from_nodes_with_packs(
            &[content], &[], &HashMap::new(),
            &serde_json::json!({}), &serde_json::json!({}), &[], &mut || 0.0,
        );
        assert!(plan.segments.iter().all(|s| !s.source.starts_with("pack:")));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-core assembly::tests::content_node_injects_packs`
Expected: FAIL — `cannot find function assemble_from_nodes_with_packs`.

- [ ] **Step 3: Rename the function and add the wrapper.** In `shirita-core/src/assembly.rs`, change the signature at line 416 from `pub fn assemble_from_nodes(` to:

```rust
pub fn assemble_from_nodes_with_packs(
    nodes: &[PromptNode],
    pack_trees: &[Vec<PromptNode>],
    definitions: &HashMap<String, Definition>,
    overrides: &serde_json::Value,
    state: &serde_json::Value,
    recent_msgs: &[String],
    roll: &mut impl FnMut() -> f64,
) -> AssembledPlan {
```

Immediately **above** it, add the back-compat wrapper:

```rust
/// Back-compat: assemble a single owner tree with no mounted packs.
pub fn assemble_from_nodes(
    nodes: &[PromptNode],
    definitions: &HashMap<String, Definition>,
    overrides: &serde_json::Value,
    state: &serde_json::Value,
    recent_msgs: &[String],
    roll: &mut impl FnMut() -> f64,
) -> AssembledPlan {
    assemble_from_nodes_with_packs(nodes, &[], definitions, overrides, state, recent_msgs, roll)
}
```

- [ ] **Step 4: Feed pack refs into the activation pass.** Inside `assemble_from_nodes_with_packs`, right after the loop that builds `entries` from `nodes` (ends ~line 444, before `let active = activate(...)`), add:

```rust
    // Pack refs share the single activation pass (spec §8): same scan buffer and
    // recursion budget as the template/session tree.
    for pack in pack_trees {
        for n in pack {
            if n.kind != NodeKind::Ref {
                continue;
            }
            let Some(def) = n.definition_id.as_ref().and_then(|id| definitions.get(id)) else {
                continue;
            };
            if is_non_rendering(&def.def_type) {
                continue;
            }
            let (scan_depth, recursive) = effective_scan(def, overrides);
            entries.push(Entry {
                id: def.id.clone(),
                trigger: effective_trigger(def, overrides),
                content: render_vars(&strip_comments(&effective_def_content(def, overrides)), state),
                scan_depth,
                recursive,
            });
        }
    }
```

- [ ] **Step 5: Add a pack body renderer + folder walker.** After the existing `let resolve = |n: &PromptNode| -> Option<String> { … };` closure (ends ~line 463), add:

```rust
    // Renders a pack ref's body WITHOUT a per-node tag wrap (the type grouping at
    // the content node owns the wrapping). Returns (def_type, body) when enabled,
    // world-info-active, rendering, and non-empty.
    let render_pack_body = |n: &PromptNode| -> Option<(String, String)> {
        if !n.enabled || n.kind != NodeKind::Ref {
            return None;
        }
        let def = n.definition_id.as_ref().and_then(|id| definitions.get(id))?;
        if is_non_rendering(&def.def_type) || !active.contains(&def.id) {
            return None;
        }
        let body = render_vars(&strip_comments(&effective_def_content(def, overrides)), state);
        (!body.trim().is_empty()).then(|| (def.def_type.clone(), body))
    };

    // Walks one pack tree (root refs + one level of folders), honoring select=one,
    // yielding (def_type, body) pairs in walk order. Pack folder tags are ignored
    // here — the content node groups by type. (Nested folder-tag wrapping in packs
    // is out of scope for this plan.)
    let pack_pairs = |pack: &[PromptNode]| -> Vec<(String, String)> {
        let mut roots: Vec<&PromptNode> = pack.iter().filter(|n| n.parent_id.is_none()).collect();
        roots.sort_by_key(|n| n.sort_order);
        let mut pairs: Vec<(String, String)> = Vec::new();
        for root in roots {
            match root.kind {
                NodeKind::Ref => {
                    if let Some(p) = render_pack_body(root) {
                        pairs.push(p);
                    }
                }
                NodeKind::Folder => {
                    if !root.enabled {
                        continue;
                    }
                    let select_one =
                        root.meta.get("select").and_then(|v| v.as_str()) == Some("one");
                    let mut kids: Vec<&PromptNode> = pack
                        .iter()
                        .filter(|n| n.parent_id.as_deref() == Some(root.id.as_str()))
                        .collect();
                    kids.sort_by_key(|n| n.sort_order);
                    for k in kids {
                        if let Some(p) = render_pack_body(k) {
                            pairs.push(p);
                            if select_one {
                                break;
                            }
                        }
                    }
                }
                _ => {} // packs hold no history/content nodes
            }
        }
        pairs
    };
```

- [ ] **Step 6: Replace the `Content` stub arm.** In the root walk, replace the existing arm (lines ~481–488):

```rust
            NodeKind::Content => {
                // TODO(plan-2): inject mounted-pack content here, sorting
                // before history. For now a no-op that prevents breaking
                // changed when old templates gain a content node.
                if root.enabled {
                    // content node is recognised; packs are assembled in plan 2.
                }
            }
```

with:

```rust
            NodeKind::Content => {
                if !root.enabled {
                    continue;
                }
                // Gather (type, body) from all packs (mount order), group by type
                // preserving first-appearance order, emit one <type>…</type> segment.
                let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
                for pack in pack_trees {
                    for (ty, body) in pack_pairs(pack) {
                        match grouped.iter_mut().find(|(t, _)| *t == ty) {
                            Some((_, bodies)) => bodies.push(body),
                            None => grouped.push((ty, vec![body])),
                        }
                    }
                }
                for (ty, bodies) in grouped {
                    let mut tag = sanitize_tag(&ty);
                    if tag.is_empty() {
                        tag = "content".to_string();
                    }
                    segments.push(PromptSegment {
                        placement,
                        content: format!("<{tag}>\n{}\n</{tag}>", bodies.join("\n")),
                        source: format!("pack:{ty}"),
                    });
                }
            }
```

- [ ] **Step 7: Run to verify it passes**

Run: `cargo test -p shirita-core assembly::tests::content_node_injects_packs assembly::tests::empty_pack_trees`
Expected: PASS.

- [ ] **Step 8: Full suite + commit**

Run: `cargo test --workspace`
Expected: PASS, zero warnings (the wrapper keeps all existing `assemble_from_nodes` callers/tests compiling unchanged).

```bash
git add shirita-core/src/assembly.rs
git commit -m "feat(core): assemble mounted pack content at the content node"
```

---

### Task 2: Wire `assemble_request` to mounted packs

**Files:**
- Modify: `shirita-core/src/conversation.rs` (`assemble_request`, ~line 115; add a helper near `effective_nodes`, ~line 82)
- Test: `shirita-core/src/conversation.rs` (`#[cfg(test)] mod tests`, which already has `RecordingProvider`)

**Interfaces:**
- Consumes: `assemble_from_nodes_with_packs` (Task 1), `Session.mounted_packs`, `Storage::list_nodes(Pack, id)`.
- Produces: `pub async fn mounted_pack_trees(storage: &dyn Storage, session: &Session) -> crate::Result<Vec<Vec<PromptNode>>>` (mount order, skips empty). `assemble_request` now injects mounted-pack content + defs.

- [ ] **Step 1: Write the failing test** — append inside `#[cfg(test)] mod tests` in `shirita-core/src/conversation.rs` (mirrors the existing `RecordingProvider` tests; `send_message` setup copied from the per-entry-recursive test):

```rust
    #[tokio::test]
    async fn mounted_pack_content_reaches_the_prompt() {
        let storage: Arc<dyn Storage> = Arc::new(temp_storage().await);
        // template: content + history
        let t = crate::models::template::Template::new("T");
        storage.create_template(&t).await.unwrap();
        let mut content = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "content");
        content.kind = NodeKind::Content; content.tag = None;
        storage.create_node(&content).await.unwrap();
        let mut hist = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 1, "history");
        hist.kind = NodeKind::History; hist.tag = None;
        storage.create_node(&hist).await.unwrap();
        // pack with a char def
        let p = crate::models::pack::Pack::new("Alice");
        storage.create_pack(&p).await.unwrap();
        let mut def = Definition::new("char", "Alice", "Alice is brave.");
        def.id = "d_alice".into();
        storage.create_definition(&def).await.unwrap();
        storage.create_node(&PromptNode::new_ref(OwnerKind::Pack, &p.id, None, 0, &def.id)).await.unwrap();
        // session mounting template + pack
        let mut session = Session::new("Chat");
        session.template_id = Some(t.id.clone());
        session.mounted_packs = vec![p.id.clone()];
        storage.create_session(&session).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let counter: Arc<dyn TokenCounter> = Arc::new(crate::tokenizer::TiktokenCounter::new());
        let mut stream = send_message(storage.clone(), provider, counter, &session.id, "hi", "m", &[], "/tmp");
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().expect("a request was sent");
        let system_blob: String = req.messages.iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("\n");
        assert!(system_blob.contains("<char>") && system_blob.contains("Alice is brave."),
            "mounted pack char content appears in the assembled prompt");
    }
```

> If `send_message`'s argument list differs from the copied call, match it to the nearest existing `RecordingProvider` test in this module (same file) — do not invent arguments.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-core conversation::tests::mounted_pack_content_reaches_the_prompt`
Expected: FAIL — assertion fails (`<char>`/content absent: packs not yet wired in).

- [ ] **Step 3: Add the helper** in `shirita-core/src/conversation.rs`, right after `effective_nodes` (~line 82):

```rust
/// The node trees of the session's mounted packs, in mount order (empty trees skipped).
pub async fn mounted_pack_trees(
    storage: &dyn Storage,
    session: &Session,
) -> crate::Result<Vec<Vec<PromptNode>>> {
    let mut trees = Vec::new();
    for pid in &session.mounted_packs {
        let nodes = storage.list_nodes(&OwnerKind::Pack, pid).await?;
        if !nodes.is_empty() {
            trees.push(nodes);
        }
    }
    Ok(trees)
}
```

- [ ] **Step 4: Wire it into `assemble_request`.** After the `defs` map is built from `nodes` (~line 133, before `let local = …`), add pack loading:

```rust
    let pack_trees = mounted_pack_trees(storage, session).await?;
    for tree in &pack_trees {
        for n in tree {
            if let Some(did) = &n.definition_id {
                if !defs.contains_key(did) {
                    if let Ok(Some(d)) = storage.get_definition(did).await {
                        defs.insert(did.clone(), d);
                    }
                }
            }
        }
    }
```

Then change the assembler call (~line 149) from `crate::assembly::assemble_from_nodes(` to `crate::assembly::assemble_from_nodes_with_packs(` and insert `&pack_trees,` as the second argument:

```rust
    let mut plan = crate::assembly::assemble_from_nodes_with_packs(
        &nodes,
        &pack_trees,
        &defs,
        &local,
        state,
        &recent,
        &mut || rand::Rng::gen::<f64>(&mut rng),
    );
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p shirita-core conversation::tests::mounted_pack_content_reaches_the_prompt`
Expected: PASS.

- [ ] **Step 6: Full suite + commit**

Run: `cargo test --workspace`
Expected: PASS, zero warnings.

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(core): assemble_request injects mounted pack content"
```

---

### Task 3: Regex pipeline includes mounted packs

**Files:**
- Modify: `shirita-core/src/conversation.rs` (`effective_regex_rules`, ~line 86)
- Test: `shirita-core/src/conversation.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `Session.mounted_packs`, `Storage::list_nodes(Pack, id)`.
- Produces: `effective_regex_rules` now appends mounted packs' `regex_rule` refs after the template/session tree's, in mount order then pack node order (deterministic pipeline, spec §7.1).

- [ ] **Step 1: Write the failing test** — append inside `#[cfg(test)] mod tests`:

```rust
    #[tokio::test]
    async fn effective_regex_includes_mounted_pack_rules_in_order() {
        let storage: Arc<dyn Storage> = Arc::new(temp_storage().await);
        // a global orphan rule (referenced by nothing)
        let mut global = Definition::new("regex_rule", "global", "");
        global.id = "r_global".into();
        global.meta = serde_json::json!({ "pattern": "a", "replacement": "b" });
        storage.create_definition(&global).await.unwrap();
        // a pack with a scoped regex rule
        let p = crate::models::pack::Pack::new("FX");
        storage.create_pack(&p).await.unwrap();
        let mut scoped = Definition::new("regex_rule", "scoped", "");
        scoped.id = "r_scoped".into();
        scoped.meta = serde_json::json!({ "pattern": "x", "replacement": "y" });
        storage.create_definition(&scoped).await.unwrap();
        storage.create_node(&PromptNode::new_ref(OwnerKind::Pack, &p.id, None, 0, &scoped.id)).await.unwrap();

        let mut session = Session::new("Chat");
        session.mounted_packs = vec![p.id.clone()];
        storage.create_session(&session).await.unwrap();

        let rules = super::effective_regex_rules(storage.as_ref(), &session).await.unwrap();
        let ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["r_global", "r_scoped"], "global first, then mounted-pack scoped");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-core conversation::tests::effective_regex_includes_mounted_pack_rules_in_order`
Expected: FAIL — `r_scoped` missing (only `["r_global"]`).

- [ ] **Step 3: Append mounted-pack rules** in `effective_regex_rules`, after the `for n in effective_nodes(...)` loop and before `Ok(rules)`:

```rust
    // Mounted packs' scoped regex rules, in mount order then pack node order —
    // extends the deterministic pipeline after the template/session tree's rules.
    for pid in &session.mounted_packs {
        for n in storage.list_nodes(&crate::models::prompt_node::OwnerKind::Pack, pid).await? {
            if n.kind == crate::models::prompt_node::NodeKind::Ref && n.enabled {
                if let Some(d) = n.definition_id.as_deref().and_then(|id| by_id.get(id)) {
                    if d.def_type == "regex_rule" {
                        rules.push((*d).clone());
                    }
                }
            }
        }
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p shirita-core conversation::tests::effective_regex_includes_mounted_pack_rules_in_order`
Expected: PASS.

- [ ] **Step 5: Full suite + commit**

Run: `cargo test --workspace`
Expected: PASS, zero warnings.

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(core): mounted pack regex rules join the effective pipeline"
```

---

### Task 4: Variable schema merges mounted-pack metas

**Files:**
- Modify: `shirita-core/src/state.rs` (`resolve_schema`, ~line 185)
- Modify: `shirita-core/src/conversation.rs` (`session_schema`, ~line 22)
- Test: `shirita-core/src/state.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub fn resolve_schema_with_packs(template_meta: Option<&Value>, pack_metas: &[Value], override_config: &Value) -> Vec<VarDecl>`. Merge order: system < template < packs < local. `resolve_schema(...)` becomes a wrapper passing `&[]` (so `shirita-web` callers stay valid). `session_schema` loads mounted packs' `meta` and calls the new variant.

- [ ] **Step 1: Write the failing test** — append inside `#[cfg(test)] mod tests` in `shirita-core/src/state.rs`:

```rust
    #[test]
    fn pack_variables_merge_between_template_and_local() {
        let tmeta = serde_json::json!({ "variables": [ { "name": "tone", "type": "string", "initial": "calm" } ] });
        let pmeta = serde_json::json!({ "variables": [ { "name": "affection", "type": "number", "initial": "0" } ] });
        let cfg = serde_json::json!({});
        let schema = resolve_schema_with_packs(Some(&tmeta), std::slice::from_ref(&pmeta), &cfg);
        assert!(schema.iter().any(|d| d.name == "tone"));
        assert!(schema.iter().any(|d| d.name == "affection"), "pack variable is in the schema");
    }

    #[test]
    fn local_overrides_pack_variable() {
        let pmeta = serde_json::json!({ "variables": [ { "name": "affection", "type": "number", "initial": "0" } ] });
        let cfg = serde_json::json!({ "local_variables": [ { "name": "affection", "type": "string", "initial": "x" } ] });
        let schema = resolve_schema_with_packs(None, std::slice::from_ref(&pmeta), &cfg);
        let d = schema.iter().find(|d| d.name == "affection").unwrap();
        assert_eq!(d.var_type, VarType::String, "local declaration wins over pack");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-core state::tests::pack_variables_merge state::tests::local_overrides_pack`
Expected: FAIL — `cannot find function resolve_schema_with_packs`.

- [ ] **Step 3: Add the `_with_packs` variant + wrapper** in `shirita-core/src/state.rs`. Replace the existing `resolve_schema` (lines ~185–190) with:

```rust
/// Resolve a session's effective schema: system ∪ template `meta.variables` ∪
/// each mounted pack's `meta.variables` ∪ session `override_config.local_variables`.
/// Later sources win on name collision (local is authoritative).
pub fn resolve_schema_with_packs(
    template_meta: Option<&Value>,
    pack_metas: &[Value],
    override_config: &Value,
) -> Vec<VarDecl> {
    let mut out = system_variables();
    merge_decls(&mut out, parse_decls(template_meta.and_then(|m| m.get("variables")), "template"));
    for pm in pack_metas {
        merge_decls(&mut out, parse_decls(pm.get("variables"), "pack"));
    }
    merge_decls(&mut out, parse_decls(override_config.get("local_variables"), "local"));
    out
}

/// Back-compat: resolve a schema with no mounted packs.
pub fn resolve_schema(template_meta: Option<&Value>, override_config: &Value) -> Vec<VarDecl> {
    resolve_schema_with_packs(template_meta, &[], override_config)
}
```

- [ ] **Step 4: Run the state tests to verify they pass**

Run: `cargo test -p shirita-core state::tests::pack_variables_merge state::tests::local_overrides_pack`
Expected: PASS.

- [ ] **Step 5: Load pack metas in `session_schema`** — in `shirita-core/src/conversation.rs`, replace the body of `session_schema` (lines ~22–28) with:

```rust
async fn session_schema(storage: &dyn Storage, session: &Session) -> Vec<VarDecl> {
    let template_meta = match &session.template_id {
        Some(tid) => storage.get_template(tid).await.ok().flatten().map(|t| t.meta),
        None => None,
    };
    let mut pack_metas = Vec::new();
    for pid in &session.mounted_packs {
        if let Ok(Some(p)) = storage.get_pack(pid).await {
            pack_metas.push(p.meta);
        }
    }
    crate::state::resolve_schema_with_packs(template_meta.as_ref(), &pack_metas, &session.override_config)
}
```

(The `use` of `resolve_schema` in conversation.rs stays — it is still re-exported and used elsewhere; the new call is fully qualified.)

- [ ] **Step 6: Full suite + commit**

Run: `cargo test --workspace`
Expected: PASS, zero warnings. (The two `shirita-web` `resolve_schema` callers still compile — the old signature is preserved.)

```bash
git add shirita-core/src/state.rs shirita-core/src/conversation.rs
git commit -m "feat(core): merge mounted pack variable schemas into session schema"
```

---

## Self-Review

**Spec coverage (§16.2 slice):** content node injection (cross-pack type grouping + shared activation) ✓ T1; `assemble_request` wiring ✓ T2; regex deterministic pipeline incl. packs ✓ T3; variable schema merge incl. packs ✓ T4. **Identity pack-aware** is intentionally **moved to Plan 3** (constraint above) — flagged, not dropped.

**Placeholder scan:** none — every step has exact code/commands. The one soft spot (Task 2 `send_message` arg list) is guarded with an explicit instruction to match the neighboring `RecordingProvider` test rather than invent args.

**Type consistency:** `assemble_from_nodes_with_packs` (T1) is called with `&pack_trees: &Vec<Vec<PromptNode>>` (coerces to `&[Vec<PromptNode>]`) in T2 ✓. `mounted_pack_trees` returns `Vec<Vec<PromptNode>>` (T2) matching the assembler's `pack_trees` param (T1) ✓. `resolve_schema_with_packs(Option<&Value>, &[Value], &Value)` (T4) matches `session_schema`'s call ✓. `OwnerKind::Pack` / `NodeKind::Content` / `Storage::{get_pack,list_nodes}` / `Session.mounted_packs` all from Plan 1 ✓.

**Deferred to later plans (intentional):** pack/session-pack REST endpoints + `PUT …/packs` + new-chat seeding (Plan 3); pack-aware `resolve_identity` + web `resolve_schema` callers gaining pack vars (Plan 3); nested folder-tag wrapping inside packs and `def_types.sort` type ordering at the content node (currently first-appearance order — acceptable, refine later); frontend (Plan 4); ST import → Pack (Plan 5).
