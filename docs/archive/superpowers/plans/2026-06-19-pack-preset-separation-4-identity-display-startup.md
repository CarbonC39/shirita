# Pack/Preset Separation — Plan 4: Identity, Display Schema & Startup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make chat identity (display name + avatar) and the displayed variable schema honor mounted packs, and wire the existing `content`-node backfill into both app startups so legacy templates self-heal.

**Architecture:** Core gets a pure `resolve_identity_with_packs` that layers a pack's bound `PackIdentity` over the existing definition-based resolution (`resolve_identity` becomes the no-pack wrapper). The web `get_session_identity` handler scans mounted packs to pick which pack binds the assistant vs. the user, and folds pack ref nodes into the name-fallback pool. `get_state` switches to `resolve_schema_with_packs` (mirroring `create_session`) so pack-declared variables appear in the display schema. Finally the already-tested `ensure_templates_have_content_node` is re-exported and called at startup in `shirita-web` and `shirita-tauri`.

**Tech Stack:** Rust, axum, sqlx (runtime API), serde_json. Web tests use `tower::ServiceExt::oneshot` with `Bearer secret-token`.

## Global Constraints

- Wrapper pattern: keep the existing public signature `resolve_identity(nodes, defs, template_name, session_avatar)` intact as a thin wrapper; put the real logic in `resolve_identity_with_packs(...)`. Do not churn existing callers/tests.
- `PackIdentity` JSON fields are snake_case: `display_name`, `avatar` (both `Option<String>`, `skip_serializing_if = "Option::is_none"`).
- Empty-string identity fields count as "unset" and must fall through to the next source.
- Zero build warnings after every task (`cargo build` clean). Comments and commit messages in English.
- `resolve_schema_with_packs(template_meta: Option<&Value>, pack_metas: &[Value], override_config: &Value)` already exists in `shirita-core/src/state.rs`.
- `ensure_templates_have_content_node` already exists in `shirita-core/src/seed.rs` with passing unit tests — Plan 4 only re-exports and wires it; do **not** reimplement it.

---

## File Structure

- `shirita-core/src/identity.rs` — add `resolve_identity_with_packs` + pack-override helpers; `resolve_identity` becomes wrapper. (Task 1)
- `shirita-web/src/routes/sessions.rs` — `get_session_identity` becomes pack-aware. (Task 2)
- `shirita-web/tests/sessions_test.rs` — pack identity integration tests. (Task 2)
- `shirita-web/src/routes/variables.rs` — `get_state` uses `resolve_schema_with_packs`. (Task 3)
- `shirita-web/tests/variables_test.rs` — pack-variable display-schema test. (Task 3)
- `shirita-core/src/lib.rs` — re-export `ensure_templates_have_content_node`. (Task 4)
- `shirita-web/src/main.rs`, `shirita-tauri/src/main.rs` — call backfill at startup. (Task 4)

---

### Task 1: Core `resolve_identity_with_packs`

**Files:**
- Modify: `shirita-core/src/identity.rs`

**Interfaces:**
- Consumes: `PackIdentity { display_name: Option<String>, avatar: Option<String> }` from `crate::models::pack`; existing private `pick(nodes, defs, def_type, template_name) -> Option<&Definition>`.
- Produces: `pub fn resolve_identity_with_packs(nodes: &[PromptNode], defs: &HashMap<String, Definition>, template_name: Option<&str>, session_avatar: Option<&str>, assistant_pack: Option<&PackIdentity>, user_pack: Option<&PackIdentity>) -> Identity`. `resolve_identity(...)` keeps its 4-arg signature as a wrapper passing `None, None`.

- [ ] **Step 1: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` block in `shirita-core/src/identity.rs` (after the `disabled_ref_is_ignored` test). `use super::*;` is already present, so `PackIdentity` (imported at module top in Step 3) is in scope.

```rust
    fn pack_id(display: Option<&str>, avatar: Option<&str>) -> PackIdentity {
        PackIdentity {
            display_name: display.map(String::from),
            avatar: avatar.map(String::from),
        }
    }

    #[test]
    fn pack_identity_overrides_char_name_and_avatar() {
        let nodes = vec![refn("c", 0, true)];
        let defs = map(vec![def("c", "char", "Alice", None)]);
        let ap = pack_id(Some("Alice the Bound"), Some("p.png"));
        let id = resolve_identity_with_packs(&nodes, &defs, None, Some("s.png"), Some(&ap), None);
        assert_eq!(id.assistant.name.as_deref(), Some("Alice the Bound"));
        assert_eq!(id.assistant.avatar.as_deref(), Some("p.png"));
    }

    #[test]
    fn empty_pack_identity_falls_back_to_def_and_session() {
        let nodes = vec![refn("c", 0, true)];
        let defs = map(vec![def("c", "char", "Alice", None)]);
        let ap = pack_id(Some(""), Some("")); // empty string == unset
        let id = resolve_identity_with_packs(&nodes, &defs, None, Some("s.png"), Some(&ap), None);
        assert_eq!(id.assistant.name.as_deref(), Some("Alice"));
        assert_eq!(id.assistant.avatar.as_deref(), Some("s.png"));
    }

    #[test]
    fn user_pack_overrides_persona() {
        let nodes = vec![refn("p", 0, true)];
        let defs = map(vec![def("p", "persona", "Me", Some("u.png"))]);
        let up = pack_id(Some("Hero"), Some("hero.png"));
        let id = resolve_identity_with_packs(&nodes, &defs, None, None, None, Some(&up));
        assert_eq!(id.user.name.as_deref(), Some("Hero"));
        assert_eq!(id.user.avatar.as_deref(), Some("hero.png"));
    }

    #[test]
    fn resolve_identity_matches_no_pack_call() {
        let nodes = vec![refn("c", 0, true)];
        let defs = map(vec![def("c", "char", "Alice", None)]);
        let a = resolve_identity(&nodes, &defs, None, Some("s.png"));
        let b = resolve_identity_with_packs(&nodes, &defs, None, Some("s.png"), None, None);
        assert_eq!(a, b);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shirita-core identity:: 2>&1 | tail -20`
Expected: FAIL — compile error `cannot find function resolve_identity_with_packs` / `cannot find ... PackIdentity` (not yet imported).

- [ ] **Step 3: Implement the pack-aware function + wrapper**

In `shirita-core/src/identity.rs`, add the `PackIdentity` import below the existing model imports (after line `use crate::models::prompt_node::{NodeKind, PromptNode};`):

```rust
use crate::models::pack::PackIdentity;
```

Replace the existing `pub fn resolve_identity(...) -> Identity { ... }` (the body that builds `assistant_name`/`persona` and returns `Identity { ... }`) with the wrapper plus the new function and two helpers:

```rust
/// Resolve the assistant/user identity. `session_avatar` is the chat's avatar
/// (the assistant/character avatar source); persona avatar comes from its
/// definition's `meta.avatar`. No-pack wrapper over `resolve_identity_with_packs`.
pub fn resolve_identity(
    nodes: &[PromptNode],
    defs: &HashMap<String, Definition>,
    template_name: Option<&str>,
    session_avatar: Option<&str>,
) -> Identity {
    resolve_identity_with_packs(nodes, defs, template_name, session_avatar, None, None)
}

/// Resolve identity with optional pack-bound overrides. A mounted character
/// pack's `PackIdentity` (display_name/avatar) takes priority over the char
/// definition's name and the session avatar; a persona pack's identity takes
/// priority over the persona definition. Empty-string fields count as "unset"
/// and fall through to the next source.
pub fn resolve_identity_with_packs(
    nodes: &[PromptNode],
    defs: &HashMap<String, Definition>,
    template_name: Option<&str>,
    session_avatar: Option<&str>,
    assistant_pack: Option<&PackIdentity>,
    user_pack: Option<&PackIdentity>,
) -> Identity {
    let char_def = pick(nodes, defs, "char", template_name);
    let persona_def = pick(nodes, defs, "persona", template_name);
    Identity {
        assistant: SideIdentity {
            name: pack_name(assistant_pack).or_else(|| char_def.map(|d| d.name.clone())),
            avatar: pack_avatar(assistant_pack).or_else(|| session_avatar.map(|s| s.to_string())),
        },
        user: SideIdentity {
            name: pack_name(user_pack).or_else(|| persona_def.map(|d| d.name.clone())),
            avatar: pack_avatar(user_pack).or_else(|| {
                persona_def
                    .and_then(|d| d.meta.get("avatar"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            }),
        },
    }
}

/// A pack identity's non-empty display name (treats `Some("")` as unset).
fn pack_name(p: Option<&PackIdentity>) -> Option<String> {
    p.and_then(|p| p.display_name.clone()).filter(|s| !s.is_empty())
}

/// A pack identity's non-empty avatar (treats `Some("")` as unset).
fn pack_avatar(p: Option<&PackIdentity>) -> Option<String> {
    p.and_then(|p| p.avatar.clone()).filter(|s| !s.is_empty())
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p shirita-core identity:: 2>&1 | tail -20`
Expected: PASS — the 4 new tests plus the 4 pre-existing identity tests all pass.

- [ ] **Step 5: Verify zero warnings**

Run: `cargo build -p shirita-core 2>&1 | grep -c warning`
Expected: `0`

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/identity.rs
git commit -m "feat(core): pack-aware resolve_identity_with_packs (identity binding)"
```

---

### Task 2: Pack-aware `get_session_identity`

**Files:**
- Modify: `shirita-web/src/routes/sessions.rs:184-218` (`get_session_identity`)
- Test: `shirita-web/tests/sessions_test.rs`

**Interfaces:**
- Consumes: `shirita_core::identity::resolve_identity_with_packs` (Task 1); `shirita_core::PackIdentity` (re-exported from `shirita-core/src/lib.rs:47`); `state.storage.get_pack(id)`, `state.storage.list_nodes(&OwnerKind::Pack, id)`, `shirita_core::conversation::effective_nodes`.
- Produces: same `GET /api/sessions/{id}/identity` → `Json<Identity>` contract; now reflects mounted-pack bindings.

- [ ] **Step 1: Write the failing integration tests**

Add to `shirita-web/tests/sessions_test.rs` (after `identity_is_null_without_a_template`). The file's `send(state, method, uri, Option<&str>)` helper and `Bearer secret-token` auth are already defined.

```rust
#[tokio::test]
async fn identity_prefers_mounted_pack_binding() {
    let state = test_state().await;
    // A char definition lives inside a pack; the pack binds a display name + avatar.
    let (_, c) = send(&state, "POST", "/api/definitions", Some(r#"{"type":"char","name":"Alice","content":"desc"}"#)).await;
    let cid = serde_json::from_str::<serde_json::Value>(&c).unwrap()["id"].as_str().unwrap().to_string();
    let (_, p) = send(&state, "POST", "/api/packs",
        Some(r#"{"name":"AlicePack","identity":{"display_name":"Alice the Bound","avatar":"pack.png"}}"#)).await;
    let pid = serde_json::from_str::<serde_json::Value>(&p).unwrap()["id"].as_str().unwrap().to_string();
    let body = format!(r#"{{"kind":"ref","definition_id":"{cid}"}}"#);
    send(&state, "POST", &format!("/api/packs/{pid}/nodes?owner_kind=pack"), Some(&body)).await;
    // Session with no template-bound char — only the mounted pack + a session avatar.
    let (_, s) = send(&state, "POST", "/api/sessions",
        Some(&format!(r#"{{"name":"chat","avatar":"face.png","pack_ids":["{pid}"]}}"#))).await;
    let sid = serde_json::from_str::<serde_json::Value>(&s).unwrap()["id"].as_str().unwrap().to_string();

    let (st, out) = send(&state, "GET", &format!("/api/sessions/{sid}/identity"), None).await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["assistant"]["name"], "Alice the Bound"); // pack display_name wins
    assert_eq!(v["assistant"]["avatar"], "pack.png");       // pack avatar over session avatar
}

#[tokio::test]
async fn identity_pack_without_display_name_falls_back_to_char_def() {
    let state = test_state().await;
    let (_, c) = send(&state, "POST", "/api/definitions", Some(r#"{"type":"char","name":"Bob","content":"d"}"#)).await;
    let cid = serde_json::from_str::<serde_json::Value>(&c).unwrap()["id"].as_str().unwrap().to_string();
    let (_, p) = send(&state, "POST", "/api/packs", Some(r#"{"name":"BobPack"}"#)).await; // no identity
    let pid = serde_json::from_str::<serde_json::Value>(&p).unwrap()["id"].as_str().unwrap().to_string();
    let body = format!(r#"{{"kind":"ref","definition_id":"{cid}"}}"#);
    send(&state, "POST", &format!("/api/packs/{pid}/nodes?owner_kind=pack"), Some(&body)).await;
    let (_, s) = send(&state, "POST", "/api/sessions",
        Some(&format!(r#"{{"name":"chat","avatar":"face.png","pack_ids":["{pid}"]}}"#))).await;
    let sid = serde_json::from_str::<serde_json::Value>(&s).unwrap()["id"].as_str().unwrap().to_string();

    let (st, out) = send(&state, "GET", &format!("/api/sessions/{sid}/identity"), None).await;
    assert_eq!(st, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["assistant"]["name"], "Bob");          // falls back to the pack's char def name
    assert_eq!(v["assistant"]["avatar"], "face.png");    // falls back to the session avatar
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shirita-web --test sessions_test identity_ 2>&1 | tail -25`
Expected: FAIL — `identity_prefers_mounted_pack_binding` asserts `"Alice"` (def name) / `"face.png"` (session avatar) under the old handler, not the pack binding. (The fallback test may already pass because the old handler ignores packs and returns null name; treat the binding test's failure as the gate.)

- [ ] **Step 3: Make `get_session_identity` pack-aware**

In `shirita-web/src/routes/sessions.rs`, replace the body of `get_session_identity` (currently lines 184-218) with:

```rust
pub async fn get_session_identity(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<shirita_core::identity::Identity>, StatusCode> {
    let session = state
        .storage
        .get_session(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut nodes = shirita_core::conversation::effective_nodes(state.storage.as_ref(), &session)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Load mounted packs (mount order) with their node trees.
    let mut packs: Vec<(shirita_core::PackIdentity, Vec<PromptNode>)> = Vec::new();
    for pid in &session.mounted_packs {
        let Ok(Some(pack)) = state.storage.get_pack(pid).await else { continue };
        let pnodes = state
            .storage
            .list_nodes(&OwnerKind::Pack, pid)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        packs.push((pack.identity, pnodes));
    }

    // Combined node pool: pack refs lead so a pack character/persona wins the
    // name fallback over any stray template char.
    let mut combined: Vec<PromptNode> = Vec::new();
    for (_, pnodes) in &packs {
        combined.extend(pnodes.iter().cloned());
    }
    combined.append(&mut nodes);

    // One definition fetch per referenced id.
    let mut defs = HashMap::new();
    for n in &combined {
        if let Some(did) = &n.definition_id {
            if !defs.contains_key(did) {
                if let Ok(Some(d)) = state.storage.get_definition(did).await {
                    defs.insert(did.clone(), d);
                }
            }
        }
    }

    // The first pack with an enabled char ref binds the assistant; the first
    // with an enabled persona ref binds the user.
    let mut assistant_pack: Option<&shirita_core::PackIdentity> = None;
    let mut user_pack: Option<&shirita_core::PackIdentity> = None;
    for (identity, pnodes) in &packs {
        let mut has_char = false;
        let mut has_persona = false;
        for n in pnodes.iter().filter(|n| n.kind == NodeKind::Ref && n.enabled) {
            match n.definition_id.as_ref().and_then(|d| defs.get(d)).map(|d| d.def_type.as_str()) {
                Some("char") => has_char = true,
                Some("persona") => has_persona = true,
                _ => {}
            }
        }
        if has_char && assistant_pack.is_none() {
            assistant_pack = Some(identity);
        }
        if has_persona && user_pack.is_none() {
            user_pack = Some(identity);
        }
    }

    let template_name = match &session.template_id {
        Some(tid) => state.storage.get_template(tid).await.ok().flatten().map(|t| t.name),
        None => None,
    };
    let identity = shirita_core::identity::resolve_identity_with_packs(
        &combined,
        &defs,
        template_name.as_deref(),
        session.avatar.as_deref(),
        assistant_pack,
        user_pack,
    );
    Ok(Json(identity))
}
```

(`HashMap`, `NodeKind`, `OwnerKind`, `PromptNode` are already imported at the top of `sessions.rs`; `shirita_core::PackIdentity` is re-exported from core's `lib.rs`.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p shirita-web --test sessions_test 2>&1 | tail -20`
Expected: PASS — the 2 new tests plus the pre-existing `identity_resolves_char_name_and_session_avatar` and `identity_is_null_without_a_template` (the no-pack path is unchanged via combined nodes = effective nodes + `None, None`).

- [ ] **Step 5: Verify zero warnings**

Run: `cargo build -p shirita-web 2>&1 | grep -c warning`
Expected: `0`

- [ ] **Step 6: Commit**

```bash
git add shirita-web/src/routes/sessions.rs shirita-web/tests/sessions_test.rs
git commit -m "feat(web): session identity honors mounted pack bindings"
```

---

### Task 3: Display schema includes pack variables

**Files:**
- Modify: `shirita-web/src/routes/variables.rs:7` (import) and `:13-32` (`get_state`)
- Test: `shirita-web/tests/variables_test.rs`

**Interfaces:**
- Consumes: `shirita_core::state::resolve_schema_with_packs` (already exists); `state.storage.get_pack(id)`.
- Produces: `GET /api/sessions/{id}/state` → `{ schema, values }` where `schema` now includes variables declared in mounted packs' `meta.variables`. Mirrors the seeding precedence already used by `create_session` (system < template < pack < local).

- [ ] **Step 1: Write the failing test**

Add to `shirita-web/tests/variables_test.rs` (after the last test). The `send`, `json`, and `create_template` helpers already exist in that file.

```rust
#[tokio::test]
async fn state_schema_includes_mounted_pack_variables() {
    let state = test_state().await;
    let tid = create_template(&state, "T", "{}").await;
    // A pack declaring its own variable.
    let (_, p) = send(&state, "POST", "/api/packs",
        Some(r#"{"name":"P","meta":{"variables":[{"name":"affection","type":"number","initial":5}]}}"#)).await;
    let pid = json(&p)["id"].as_str().unwrap().to_string();
    let (_, sout) = send(&state, "POST", "/api/sessions",
        Some(&format!(r#"{{"name":"Chat","template_id":"{tid}","pack_ids":["{pid}"]}}"#))).await;
    let sid = json(&sout)["id"].as_str().unwrap().to_string();

    let (st, state_out) = send(&state, "GET", &format!("/api/sessions/{sid}/state"), None).await;
    assert_eq!(st, StatusCode::OK);
    let body = json(&state_out);
    let names: Vec<&str> = body["schema"].as_array().unwrap()
        .iter().map(|d| d["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"affection"), "pack variable present in display schema");
    assert_eq!(body["values"]["affection"], 5);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shirita-web --test variables_test state_schema_includes_mounted_pack_variables 2>&1 | tail -20`
Expected: FAIL — `affection` missing from `schema` because `get_state` calls `resolve_schema` (template + local only), ignoring packs.

- [ ] **Step 3: Switch `get_state` to `resolve_schema_with_packs`**

In `shirita-web/src/routes/variables.rs`, change the import on line 7 from:

```rust
use shirita_core::state::{effective_state, resolve_schema};
```

to:

```rust
use shirita_core::state::{effective_state, resolve_schema_with_packs};
```

Then in `get_state`, replace the single `resolve_schema` line (currently line 24):

```rust
    let schema = resolve_schema(template_meta.as_ref(), &session.override_config);
```

with the pack-meta load + pack-aware resolve (insert before the `list_messages` call):

```rust
    let mut pack_metas = Vec::new();
    for pid in &session.mounted_packs {
        if let Ok(Some(p)) = state.storage.get_pack(pid).await {
            pack_metas.push(p.meta);
        }
    }
    let schema = resolve_schema_with_packs(template_meta.as_ref(), &pack_metas, &session.override_config);
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shirita-web --test variables_test 2>&1 | tail -20`
Expected: PASS — the new test plus the pre-existing variables tests (a session with no packs yields an empty `pack_metas`, so behavior is unchanged there).

- [ ] **Step 5: Verify zero warnings**

Run: `cargo build -p shirita-web 2>&1 | grep -c warning`
Expected: `0`

- [ ] **Step 6: Commit**

```bash
git add shirita-web/src/routes/variables.rs shirita-web/tests/variables_test.rs
git commit -m "feat(web): display schema includes mounted pack variables"
```

---

### Task 4: Wire `content`-node backfill into startup

**Files:**
- Modify: `shirita-core/src/lib.rs:52` (re-export)
- Modify: `shirita-web/src/main.rs:18-20` (after the existing seed calls)
- Modify: `shirita-tauri/src/main.rs:48-50` (after `ensure_default_template`)

**Interfaces:**
- Consumes: `shirita_core::seed::ensure_templates_have_content_node` (already implemented and unit-tested in `seed.rs`).
- Produces: `shirita_core::ensure_templates_have_content_node` re-export; both binaries run the idempotent backfill on every launch so legacy templates lacking a `content` node self-heal.

- [ ] **Step 1: Re-export the backfill from core**

In `shirita-core/src/lib.rs`, change the seed re-export (line 52) from:

```rust
pub use seed::{ensure_builtin_definitions, ensure_default_template};
```

to:

```rust
pub use seed::{
    ensure_builtin_definitions, ensure_default_template, ensure_templates_have_content_node,
};
```

- [ ] **Step 2: Call the backfill in `shirita-web` startup**

In `shirita-web/src/main.rs`, after the two existing seed lines:

```rust
    shirita_core::ensure_default_template(&storage).await?;
    shirita_core::ensure_builtin_definitions(&storage).await?;
```

add:

```rust
    // Backfill: legacy templates gain the undeletable <<content>> mount node.
    shirita_core::ensure_templates_have_content_node(&storage).await?;
```

- [ ] **Step 3: Call the backfill in `shirita-tauri` startup**

In `shirita-tauri/src/main.rs`, after the `ensure_default_template` block (currently lines 48-50, ending with `.map_err(|e| format!("初始化默认模板失败：{e}"))?;`), add:

```rust
    shirita_core::ensure_templates_have_content_node(&storage)
        .await
        .map_err(|e| format!("迁移模板 content 节点失败：{e}"))?;
```

(Place it before `let pool = storage.pool().clone();` so it runs while `storage` is still the concrete `SqliteStorage`.)

- [ ] **Step 4: Build the whole workspace and verify zero warnings**

Run: `cargo build 2>&1 | grep -c warning`
Expected: `0` (the re-export and both calls compile; `ensure_templates_have_content_node` is no longer "unused" anywhere).

- [ ] **Step 5: Re-run the pre-existing backfill unit test as a regression guard**

Run: `cargo test -p shirita-core seed:: 2>&1 | tail -20`
Expected: PASS — including `backfill_adds_one_content_node_idempotently` and `default_template_has_content_before_history` (the function the binaries now call is exercised).

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/lib.rs shirita-web/src/main.rs shirita-tauri/src/main.rs
git commit -m "feat: run content-node backfill at web + tauri startup"
```

---

## Final Verification

- [ ] **Full test sweep + warning check**

Run: `cargo test 2>&1 | tail -30 && echo "--- warnings ---" && cargo build 2>&1 | grep -c warning`
Expected: all tests pass; warning count `0`.

---

## Self-Review

**Spec coverage (Plan 4 scope):**
- Pack-bound identity (display name + avatar) — Task 1 (core) + Task 2 (web). Resolves the brainstorming concern "没有解决头像和显示名绑定的问题".
- Pack variables visible in the settings/state display — Task 3, matching `create_session`'s seeding precedence.
- Legacy template self-heal (`content` node) at startup — Task 4 (re-export + both binaries).

**Placeholder scan:** No TBD/TODO; every code step shows full code; commands include expected output.

**Type consistency:** `resolve_identity_with_packs` signature is identical in Task 1 (definition) and Task 2 (call site). `PackIdentity` fields `display_name`/`avatar` match `pack.rs`. `resolve_schema_with_packs(template_meta, pack_metas, override_config)` matches `create_session`'s existing call. `ensure_templates_have_content_node` name matches `seed.rs`.

**Note:** Task 4 adds no new automated test because the function and its unit tests already exist (`seed.rs`); the wiring is guarded by `cargo build` (zero warnings — it would otherwise be dead code) plus re-running the existing seed tests.
