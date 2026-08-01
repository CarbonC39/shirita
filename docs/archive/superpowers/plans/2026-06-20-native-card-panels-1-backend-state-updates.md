# Native Card Panels — Plan 1: Backend state-updates endpoint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `POST /api/sessions/{id}/state-updates` so a panel action can apply typed variable diffs mid-conversation — folding them into a hidden system state-carrier node on the active branch and advancing the active leaf to it.

**Architecture:** Reuse the exact M5 fold pattern from `conversation.rs` (`effective_state` → `apply_updates` → set the new message's `snapshot_state` → `set_session_active_leaf`), but triggered by a panel POST instead of an LLM turn. Schema resolution mirrors `GET …/state` (`resolve_schema_with_packs` over template + mounted packs + locals). The carrier node is `is_hidden` + role `system` + empty content, so it's excluded from the prompt yet stays on the active path (its snapshot is the branch state).

**Tech Stack:** Rust, Axum, shirita-core `state` module, sqlx runtime API (SqliteStorage), `tokio::test` integration tests via `app().oneshot()`.

## Global Constraints

- Writes are **typed, pack-scoped diffs** through `apply_updates`: unknown ops are dropped before applying; undeclared / type-mismatched keys are ignored by `apply_updates`.
- The change is **anchored on the message tree**: a hidden (`is_hidden = true`), content-less, role-`System` node, child of the current `active_leaf_id`, and `active_leaf_id` advances to it. This makes it survive regenerate/fork (copied + id-remapped like any node), excluded from the prompt (`conversation.rs` filters `!m.is_hidden`), and reachable by branch navigation. UI invisibility (not showing the dimmed empty bubble) is a **Plan 3** frontend concern, not here.
- Request `value` is always a JSON **string** (`data-diff-value` is a string attribute); `apply_updates` coerces it per the declared `VarType`.
- This plan is **backend only**. The `pack.meta.panel` TypeScript type (mentioned in the spec's §11 plan 1) is a pure frontend type with no behavior to test, so it moves to Plan 2 where `<PanelView>` first consumes it.
- Comments and commit messages in English.
- Test command: `cargo test -p shirita-web --test variables_test`.

---

## File Structure

- `shirita-core/src/state.rs` — make `Action::parse` public so the web layer can turn an op string into an `Action`. (Task 1)
- `shirita-web/src/routes/variables.rs` — add the `apply_state_updates` handler + request DTOs, alongside the existing `get_state` / `set_local_variables`. (Task 1)
- `shirita-web/src/lib.rs` — register the new route. (Task 1)
- `shirita-web/tests/variables_test.rs` — integration tests. (Task 1)

---

### Task 1: `POST /api/sessions/{id}/state-updates`

**Files:**
- Modify: `shirita-core/src/state.rs`
- Modify: `shirita-web/src/routes/variables.rs`
- Modify: `shirita-web/src/lib.rs`
- Test: `shirita-web/tests/variables_test.rs`

**Interfaces:**
- Consumes: `shirita_core::state::{effective_state, resolve_schema_with_packs, apply_updates, Action, Update}`, `shirita_core::tree::active_path`, `shirita_core::{Message, Role}`, `Storage::{get_session, get_template, get_pack, list_messages, create_message, set_session_active_leaf}`.
- Produces: route `POST /api/sessions/{id}/state-updates`; request body `{ "updates": [ { "action": String, "key": String, "value": Option<String> }, … ] }`; response `{ "values": <effective state object> }`. (Plan 3 calls this with `data-diff` actions.)

- [ ] **Step 1: Write the failing tests**

Append to `shirita-web/tests/variables_test.rs`:

```rust
#[tokio::test]
async fn state_updates_apply_typed_diff_and_persist() {
    let state = test_state().await;
    let tid = create_template(&state, "RPG", r#"{"variables":[{"name":"hp","type":"number","initial":100}]}"#).await;
    let (_, sout) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"Chat","template_id":"{tid}"}}"#))).await;
    let sid = json(&sout)["id"].as_str().unwrap().to_string();

    let (st, out) = send(&state, "POST", &format!("/api/sessions/{sid}/state-updates"),
        Some(r#"{"updates":[{"action":"sub","key":"hp","value":"10"}]}"#)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["values"]["hp"], 90);

    // persisted on the branch
    let (_, state_out) = send(&state, "GET", &format!("/api/sessions/{sid}/state"), None).await;
    assert_eq!(json(&state_out)["values"]["hp"], 90);
}

#[tokio::test]
async fn state_updates_keep_multiword_values_and_ignore_undeclared() {
    let state = test_state().await;
    let tid = create_template(&state, "RPG",
        r#"{"variables":[{"name":"location","type":"string","initial":"Town"}]}"#).await;
    let (_, sout) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"Chat","template_id":"{tid}"}}"#))).await;
    let sid = json(&sout)["id"].as_str().unwrap().to_string();

    let (st, out) = send(&state, "POST", &format!("/api/sessions/{sid}/state-updates"),
        Some(r#"{"updates":[{"action":"set","key":"location","value":"The Dark Forest"},{"action":"set","key":"bogus","value":"x"}]}"#)).await;
    assert_eq!(st, StatusCode::OK);
    let body = json(&out);
    assert_eq!(body["values"]["location"], "The Dark Forest"); // whole multi-word value, not truncated
    assert!(body["values"].get("bogus").is_none());            // undeclared key dropped
}

#[tokio::test]
async fn state_updates_insert_hidden_system_node_and_advance_leaf() {
    let state = test_state().await;
    let tid = create_template(&state, "RPG", r#"{"variables":[{"name":"hp","type":"number","initial":100}]}"#).await;
    let (_, sout) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"Chat","template_id":"{tid}"}}"#))).await;
    let sid = json(&sout)["id"].as_str().unwrap().to_string();

    // before: no active leaf, no messages
    let (_, s0) = send(&state, "GET", &format!("/api/sessions/{sid}"), None).await;
    assert!(json(&s0)["active_leaf_id"].is_null());

    let _ = send(&state, "POST", &format!("/api/sessions/{sid}/state-updates"),
        Some(r#"{"updates":[{"action":"sub","key":"hp","value":"5"}]}"#)).await;

    // after: leaf points at a hidden, role-system carrier node
    let (_, s1) = send(&state, "GET", &format!("/api/sessions/{sid}"), None).await;
    let leaf = json(&s1)["active_leaf_id"].as_str().unwrap().to_string();
    assert!(!leaf.is_empty());

    let (_, msgs_out) = send(&state, "GET", &format!("/api/sessions/{sid}/messages"), None).await;
    let msgs = json(&msgs_out);
    let arr = msgs.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], leaf);
    assert_eq!(arr[0]["role"], "system");
    assert_eq!(arr[0]["is_hidden"], true);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shirita-web --test variables_test state_updates 2>&1 | tail -20`
Expected: FAIL — the route `POST /api/sessions/{id}/state-updates` returns 404/405 (`assert_eq!(st, StatusCode::OK)` fails), since the handler and route don't exist yet.

- [ ] **Step 3: Make `Action::parse` public**

In `shirita-core/src/state.rs`, change the `Action::parse` signature from private to public (it already matches case-insensitively, so lowercase ops like `"sub"` parse to `Action::Sub`):

```rust
impl Action {
    pub fn parse(s: &str) -> Option<Action> {
        match s.to_ascii_uppercase().as_str() {
            "SET" => Some(Action::Set),
            "ADD" => Some(Action::Add),
            "SUB" => Some(Action::Sub),
            "TOGGLE" => Some(Action::Toggle),
            "APPEND" => Some(Action::Append),
            "REMOVE" => Some(Action::Remove),
            _ => None,
        }
    }
}
```

(Only the `fn parse` → `pub fn parse` change; the body is unchanged.)

- [ ] **Step 4: Add the handler + DTOs to `variables.rs`**

In `shirita-web/src/routes/variables.rs`, extend the two `use shirita_core::…` lines to:

```rust
use shirita_core::state::{apply_updates, effective_state, resolve_schema_with_packs, Action, Update};
use shirita_core::tree::active_path;
use shirita_core::{Message, Role};
```

Then append this handler at the end of the file:

```rust
#[derive(Deserialize)]
pub struct StateUpdateItem {
    pub action: String,
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Deserialize)]
pub struct StateUpdatesBody {
    pub updates: Vec<StateUpdateItem>,
}

/// Apply panel-driven variable diffs mid-conversation: fold them into a hidden
/// system state-carrier node on the active branch and advance the leaf to it.
/// Mirrors the M5 fold in conversation.rs, but triggered by a panel action.
pub async fn apply_state_updates(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<StateUpdatesBody>,
) -> Result<Json<Value>, StatusCode> {
    let session = state.storage.get_session(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Same schema GET …/state resolves: template + mounted packs + locals.
    let template_meta = match &session.template_id {
        Some(tid) => state.storage.get_template(tid).await.ok().flatten().map(|t| t.meta),
        None => None,
    };
    let mut pack_metas = Vec::new();
    for pid in &session.mounted_packs {
        if let Ok(Some(p)) = state.storage.get_pack(pid).await {
            pack_metas.push(p.meta);
        }
    }
    let schema = resolve_schema_with_packs(template_meta.as_ref(), &pack_metas, &session.override_config);

    // Current branch state = the active leaf's folded snapshot.
    let all = state.storage.list_messages(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let leaf_snapshot = active_path(&all, session.active_leaf_id.as_deref())
        .last().map(|m| m.snapshot_state.clone()).unwrap_or_else(|| json!({}));
    let branch_state = effective_state(&schema, &session.current_state, &leaf_snapshot);

    // Build typed diffs; drop unknown ops here, undeclared/type-mismatched keys
    // are ignored inside apply_updates.
    let updates: Vec<Update> = body.updates.iter().filter_map(|u| {
        Action::parse(&u.action).map(|action| Update {
            action,
            key: u.key.clone(),
            value: u.value.clone(),
        })
    }).collect();
    let new_snapshot = apply_updates(&branch_state, &schema, &updates);

    // Anchor the change: a hidden, content-less system node, then advance the leaf.
    let mut node = Message::new(&id, session.active_leaf_id.clone(), Role::System, "");
    node.is_hidden = true;
    node.snapshot_state = new_snapshot.clone();
    state.storage.create_message(&node).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    state.storage.set_session_active_leaf(&id, Some(&node.id)).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let values = effective_state(&schema, &session.current_state, &new_snapshot);
    Ok(Json(json!({ "values": values })))
}
```

- [ ] **Step 5: Register the route**

In `shirita-web/src/lib.rs`, immediately after the existing line

```rust
        .route("/sessions/{id}/local-variables", put(routes::variables::set_local_variables))
```

add:

```rust
        .route("/sessions/{id}/state-updates", post(routes::variables::apply_state_updates))
```

(`post` is already imported in this file — it's used by the sibling routes.)

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p shirita-web --test variables_test 2>&1 | tail -20`
Expected: PASS — the three new `state_updates_*` tests plus the four pre-existing variables tests (7 passed).

- [ ] **Step 7: Build the whole workspace (catch the `pub` change + imports)**

Run: `cargo build --workspace 2>&1 | tail -6`
Expected: clean build — no unused-import warnings (every added import is used), no breakage from `Action::parse` becoming public.

- [ ] **Step 8: Commit**

```bash
git add shirita-core/src/state.rs shirita-web/src/routes/variables.rs shirita-web/src/lib.rs shirita-web/tests/variables_test.rs
git commit -m "feat(web): POST /sessions/{id}/state-updates (panel diff -> hidden state node)"
```

---

## Final Verification

- [ ] **Workspace test + build sweep**

Run: `cargo test -p shirita-web --test variables_test 2>&1 | tail -8 && cargo build --workspace 2>&1 | tail -4`
Expected: all variables_test cases pass; workspace builds clean.

---

## Self-Review

**Spec coverage (spec §5):**
- `POST /api/sessions/{id}/state-updates` with body `{ updates: [{action,key,value}] }` — Task 1.
- Applies via `apply_updates` against `resolve_schema_with_packs`, typed validation, undeclared/type-mismatched ignored — Task 1 (handler + `state_updates_keep_multiword_values_and_ignore_undeclared`).
- Inserts a hidden system state-carrier node (`is_hidden`, role `system`, post-diff `snapshot_state`, child of `active_leaf_id`) — Task 1 (`state_updates_insert_hidden_system_node_and_advance_leaf`).
- Advances `active_leaf_id` to that node — Task 1 (same test).
- Returns the new `{ values }`; frontend refetch handled in Plan 3 — Task 1 (response asserted by `state_updates_apply_typed_diff_and_persist`).
- Survives regenerate/fork + excluded from prompt — inherited from the existing M4/M5 node + `is_hidden` filter (`conversation.rs:366/453`); no new code, so no new test here (covered by existing conversation tests).

**Placeholder scan:** none — full handler code, exact route line, complete test code, exact commands.

**Type consistency:** `StateUpdateItem { action: String, key: String, value: Option<String> }` → `Update { action: Action, key: String, value: Option<String> }` matches `shirita_core::state::Update`'s fields. `Action::parse(&str) -> Option<Action>` (now `pub`). `Message::new(session_id, parent_id: Option<String>, role: Role, raw_content)` matches the model. `set_session_active_leaf(&str, Option<&str>)` matches the `Storage` trait. Schema-resolution + branch-state block is copied verbatim from `get_state` / `conversation.rs`, so the types line up with existing code.
