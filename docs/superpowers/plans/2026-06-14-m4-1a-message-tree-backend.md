# M4 Plan 1a — Message Tree (Backend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give a chat session a navigable message tree — an active branch, sibling-creating regenerate, in-place edit, per-message hide, and fork-to-new-session — with generation cancellation, all behind the existing REST/SSE API.

**Architecture:** Messages already form a `parent_id` tree. We add one column `chat_sessions.active_leaf_id` (the current branch's leaf). A pure core helper `tree::active_path` turns `(all messages, active_leaf)` into the linear context the assembler consumes; `send_message`/`regenerate` walk it and update the leaf. New endpoints edit/hide a message, switch the active leaf, regenerate a sibling, and fork. In-flight SSE generations register a `futures::stream::AbortHandle` per session so a newer generation aborts the prior one (no partial persists, since the assistant is only written after the stream completes).

**Tech Stack:** Rust, Axum 0.8, sqlx (runtime query API) + SQLite, `async-stream`, `futures` (already deps). Tests: `cargo test` — pure unit tests in `shirita-core`, Axum `oneshot` integration tests in `shirita-web/tests`.

**Scope note:** This is the *backend* half of M4 subsystem A (the message tree). The frontend (swipe/edit/hide/regenerate/fork UI + chat store) is a follow-on plan (1b); copy-on-write definitions are Plan 2. Delete-branch is deferred per the spec.

**Upstream spec:** `docs/superpowers/specs/2026-06-14-m4-message-tree-design.md` (§4, §4.6).

---

## File Structure

- `shirita-core/migrations/0011_session_active_leaf.sql` — **create**: add `active_leaf_id` column.
- `shirita-core/src/models/session.rs` — **modify**: add `active_leaf_id` field + initializer.
- `shirita-core/src/storage/mod.rs` — **modify**: 3 new `Storage` methods.
- `shirita-core/src/storage/sqlite.rs` — **modify**: SELECTs + `row_to_session`; implement new methods.
- `shirita-core/src/tree.rs` — **create**: pure `active_path` / `deepest_leaf`.
- `shirita-core/src/lib.rs` — **modify**: `pub mod tree;` + re-exports.
- `shirita-core/src/conversation.rs` — **modify**: extract `assemble_request`; rewrite `send_message` to walk the active path + set the leaf; add `regenerate`.
- `shirita-web/src/routes/messages.rs` — **create**: `edit_message`, `set_active_leaf`, `fork_session` handlers.
- `shirita-web/src/routes/chat.rs` — **modify**: add `regenerate` SSE handler; wire cancellation into `send`.
- `shirita-web/src/generations.rs` — **create**: per-session `AbortHandle` registry.
- `shirita-web/src/state.rs` — **modify**: add `generations` to `AppState`.
- `shirita-web/src/lib.rs` — **modify**: register routes; construct `generations` in `app`.
- `shirita-web/src/routes/mod.rs` — **modify**: `pub mod messages;`.
- `shirita-web/tests/message_tree_test.rs` — **create**: endpoint integration tests.

---

## Task 1: Add `active_leaf_id` column + Session field (read path)

**Files:**
- Create: `shirita-core/migrations/0011_session_active_leaf.sql`
- Modify: `shirita-core/src/models/session.rs`
- Modify: `shirita-core/src/storage/sqlite.rs` (SELECTs in `get_session` + `list_sessions`, and `row_to_session`)

- [ ] **Step 1: Write the migration**

Create `shirita-core/migrations/0011_session_active_leaf.sql`:

```sql
-- The leaf message of the session's currently active branch (NULL = no messages).
ALTER TABLE chat_sessions ADD COLUMN active_leaf_id TEXT;
```

- [ ] **Step 2: Add the field to the model**

In `shirita-core/src/models/session.rs`, add the field after `sort_order` (before `preview`):

```rust
    /// Leaf message of the session's active branch (the path shown / extended).
    /// `None` until the first message; falls back to the newest message.
    #[serde(default)]
    pub active_leaf_id: Option<String>,
```

And in `Session::new`, add after `sort_order: now.timestamp_millis(),`:

```rust
            active_leaf_id: None,
```

- [ ] **Step 3: Read the column in storage**

In `shirita-core/src/storage/sqlite.rs`, add `active_leaf_id` to the `SELECT` column list of **both** `get_session` and `list_sessions` (insert it right after `sort_order`):

```
... created_at, updated_at, sort_order, active_leaf_id ...
```

(for `list_sessions`, keep the trailing `preview` correlated subquery after `active_leaf_id`).

In `row_to_session`, add the field before `preview: None,`:

```rust
        active_leaf_id: row.try_get("active_leaf_id")?,
```

- [ ] **Step 4: Verify it compiles and existing tests pass**

Run: `cargo test -p shirita-core --lib`
Expected: PASS (the new column defaults to NULL; `Session::new` sets `None`; round-trip tests unaffected).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/migrations/0011_session_active_leaf.sql shirita-core/src/models/session.rs shirita-core/src/storage/sqlite.rs
git commit -m "feat(core): add chat_sessions.active_leaf_id + Session field"
```

---

## Task 2: Storage writes — `set_session_active_leaf`, `get_message`, `update_message`

**Files:**
- Modify: `shirita-core/src/storage/mod.rs`
- Modify: `shirita-core/src/storage/sqlite.rs`

- [ ] **Step 1: Write the failing test**

Append to the `#[cfg(test)] mod tests` block in `shirita-core/src/storage/sqlite.rs` (the file already has a `new_storage()`/temp-db helper used by `session_and_message_roundtrip`; reuse it):

```rust
    #[tokio::test]
    async fn active_leaf_and_message_updates_roundtrip() {
        let store = new_test_storage().await;
        let s = Session::new("Tree");
        store.create_session(&s).await.unwrap();
        let m = Message::new(&s.id, None, Role::User, "hello");
        store.create_message(&m).await.unwrap();

        // set + read active leaf
        store.set_session_active_leaf(&s.id, Some(&m.id)).await.unwrap();
        let got = store.get_session(&s.id).await.unwrap().unwrap();
        assert_eq!(got.active_leaf_id.as_deref(), Some(m.id.as_str()));

        // get_message
        let fetched = store.get_message(&m.id).await.unwrap().unwrap();
        assert_eq!(fetched.raw_content, "hello");

        // update_message (edit + hide)
        let mut edited = fetched.clone();
        edited.raw_content = "edited".into();
        edited.display_content = Some("EDITED".into());
        edited.is_hidden = true;
        store.update_message(&edited).await.unwrap();
        let after = store.get_message(&m.id).await.unwrap().unwrap();
        assert_eq!(after.raw_content, "edited");
        assert_eq!(after.display_content.as_deref(), Some("EDITED"));
        assert!(after.is_hidden);
    }
```

> If the existing test helper has a different name than `new_test_storage`, use whatever the sibling tests call (e.g. inline `SqliteStorage::connect`/`run_migrations` like `session_and_message_roundtrip` does) — match the file's existing pattern.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core --lib active_leaf_and_message_updates_roundtrip`
Expected: FAIL — `set_session_active_leaf` / `get_message` / `update_message` not found.

- [ ] **Step 3: Declare the methods on the trait**

In `shirita-core/src/storage/mod.rs`, under the `// --- messages ---` section, add:

```rust
    async fn get_message(&self, id: &str) -> Result<Option<Message>>;
    /// Update an existing message's editable fields (raw/display content, hidden).
    async fn update_message(&self, message: &Message) -> Result<()>;
```

And under `// --- sessions ---`, add:

```rust
    /// Set (or clear with `None`) the session's active branch leaf.
    async fn set_session_active_leaf(&self, session_id: &str, leaf_id: Option<&str>) -> Result<()>;
```

- [ ] **Step 4: Implement them in sqlite**

In `shirita-core/src/storage/sqlite.rs`, inside `impl Storage for SqliteStorage`, add:

```rust
    async fn get_message(&self, id: &str) -> Result<Option<Message>> {
        let row = sqlx::query(
            "SELECT id, session_id, parent_id, role, raw_content, display_content, is_hidden, snapshot_state, created_at \
             FROM messages WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some(row_to_message(&r)?)),
            None => Ok(None),
        }
    }

    async fn update_message(&self, message: &Message) -> Result<()> {
        let snapshot = serde_json::to_string(&message.snapshot_state)?;
        sqlx::query(
            "UPDATE messages SET raw_content = ?, display_content = ?, is_hidden = ?, snapshot_state = ? WHERE id = ?",
        )
        .bind(&message.raw_content)
        .bind(&message.display_content)
        .bind(message.is_hidden as i64)
        .bind(snapshot)
        .bind(&message.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_session_active_leaf(&self, session_id: &str, leaf_id: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE chat_sessions SET active_leaf_id = ? WHERE id = ?")
            .bind(leaf_id)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p shirita-core --lib active_leaf_and_message_updates_roundtrip`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/storage/mod.rs shirita-core/src/storage/sqlite.rs
git commit -m "feat(core): storage get_message/update_message/set_session_active_leaf"
```

---

## Task 3: Pure tree helpers — `active_path` + `deepest_leaf`

**Files:**
- Create: `shirita-core/src/tree.rs`
- Modify: `shirita-core/src/lib.rs`

- [ ] **Step 1: Write the failing test (inside the new file)**

Create `shirita-core/src/tree.rs`:

```rust
//! Pure helpers over the message tree: the active branch path and branch descent.

use crate::models::message::Message;
use std::collections::HashMap;

/// The linear path root→`active_leaf_id`, following `parent_id` upward, root first.
/// If `active_leaf_id` is `None` or unknown, falls back to the newest message as
/// the leaf (keeps pre-M4 / freshly-forked sessions working).
pub fn active_path<'a>(messages: &'a [Message], active_leaf_id: Option<&str>) -> Vec<&'a Message> {
    let by_id: HashMap<&str, &Message> = messages.iter().map(|m| (m.id.as_str(), m)).collect();
    let leaf = active_leaf_id
        .and_then(|id| by_id.get(id).copied())
        .or_else(|| messages.iter().max_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id))));
    let mut path = Vec::new();
    let mut cur = leaf;
    while let Some(m) = cur {
        path.push(m);
        cur = m.parent_id.as_deref().and_then(|p| by_id.get(p).copied());
    }
    path.reverse();
    path
}

/// From `from_id`, descend by picking the newest child at each level until a
/// leaf; returns that leaf id (= `from_id` if it has no children).
pub fn deepest_leaf(messages: &[Message], from_id: &str) -> String {
    let mut cur = from_id.to_string();
    loop {
        let next = messages
            .iter()
            .filter(|m| m.parent_id.as_deref() == Some(cur.as_str()))
            .max_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        match next {
            Some(child) => cur = child.id.clone(),
            None => return cur,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::message::Role;

    fn msg(id: &str, parent: Option<&str>, created: &str) -> Message {
        let mut m = Message::new("s", parent.map(|p| p.to_string()), Role::User, "x");
        m.id = id.to_string();
        m.created_at = created.to_string();
        m
    }

    #[test]
    fn active_path_walks_root_to_leaf() {
        // a -> b -> c   and a -> b -> c2 (sibling of c)
        let ms = vec![
            msg("a", None, "1"),
            msg("b", Some("a"), "2"),
            msg("c", Some("b"), "3"),
            msg("c2", Some("b"), "4"),
        ];
        let path: Vec<&str> = active_path(&ms, Some("c2")).iter().map(|m| m.id.as_str()).collect();
        assert_eq!(path, vec!["a", "b", "c2"]);
    }

    #[test]
    fn active_path_falls_back_to_newest_when_leaf_missing() {
        let ms = vec![msg("a", None, "1"), msg("b", Some("a"), "2")];
        let path: Vec<&str> = active_path(&ms, None).iter().map(|m| m.id.as_str()).collect();
        assert_eq!(path, vec!["a", "b"]);
    }

    #[test]
    fn deepest_leaf_follows_newest_child() {
        let ms = vec![
            msg("a", None, "1"),
            msg("b", Some("a"), "2"),
            msg("c_old", Some("b"), "3"),
            msg("c_new", Some("b"), "4"),
        ];
        assert_eq!(deepest_leaf(&ms, "b"), "c_new");
        assert_eq!(deepest_leaf(&ms, "c_new"), "c_new");
    }
}
```

- [ ] **Step 2: Wire the module**

In `shirita-core/src/lib.rs`, add `pub mod tree;` next to the other `pub mod` declarations.

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p shirita-core --lib tree::`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add shirita-core/src/tree.rs shirita-core/src/lib.rs
git commit -m "feat(core): pure active_path + deepest_leaf message-tree helpers"
```

---

## Task 4: `send_message` walks the active path + sets the leaf (with shared `assemble_request`)

**Files:**
- Modify: `shirita-core/src/conversation.rs`
- Test: `shirita-core/src/conversation.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` in `shirita-core/src/conversation.rs` (it already builds storage + `EchoProvider` for `echo_send_streams_and_persists`; reuse that harness):

```rust
    #[tokio::test]
    async fn send_chains_under_active_leaf_and_updates_it() {
        let (storage, provider, counter) = test_deps().await; // mirror the existing harness
        let session = Session::new("Chat");
        storage.create_session(&session).await.unwrap();

        // first turn
        drain(send_message(storage.clone(), provider.clone(), counter.clone(),
            "m".into(), session.id.clone(), "hi".into())).await;
        let s1 = storage.get_session(&session.id).await.unwrap().unwrap();
        let msgs1 = storage.list_messages(&session.id).await.unwrap();
        assert_eq!(msgs1.len(), 2); // user + assistant
        let assistant1 = msgs1.iter().find(|m| m.role == Role::Assistant).unwrap();
        assert_eq!(s1.active_leaf_id.as_deref(), Some(assistant1.id.as_str()));

        // second turn chains under the previous assistant (the active leaf)
        drain(send_message(storage.clone(), provider.clone(), counter.clone(),
            "m".into(), session.id.clone(), "again".into())).await;
        let msgs2 = storage.list_messages(&session.id).await.unwrap();
        let user2 = msgs2.iter().find(|m| m.role == Role::User && m.raw_content == "again").unwrap();
        assert_eq!(user2.parent_id.as_deref(), Some(assistant1.id.as_str()));
    }
```

> `test_deps()` and `drain()` are small local helpers — if the existing tests inline these, copy their bodies. `drain` just consumes the `SendEvent` stream to completion:
> ```rust
> async fn drain(stream: impl futures::Stream<Item = SendEvent>) {
>     futures::pin_mut!(stream);
>     while futures::StreamExt::next(&mut stream).await.is_some() {}
> }
> ```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core --lib send_chains_under_active_leaf_and_updates_it`
Expected: FAIL — second user's `parent_id` is `None`/wrong, and `active_leaf_id` is `None` (current code chains off `history.last()` and never sets the leaf).

- [ ] **Step 3: Extract `assemble_request` and rewrite the history walk**

In `shirita-core/src/conversation.rs`, add a private helper near `effective_nodes` (it factors the assembly currently inlined in `send_message`):

```rust
/// Build the provider request for a turn whose visible, ordered context is
/// `context` (hidden already filtered, ending with the latest user turn), plus
/// the regex rules used to clean the reply. Shared by send + regenerate.
async fn assemble_request(
    storage: &dyn Storage,
    session: &Session,
    model: String,
    context: &[ChatMessage],
) -> Result<(ChatRequest, Vec<Definition>)> {
    let nodes = effective_nodes(storage, session).await?;
    let mut defs = std::collections::HashMap::new();
    for n in &nodes {
        if let Some(did) = &n.definition_id {
            if !defs.contains_key(did) {
                if let Ok(Some(d)) = storage.get_definition(did).await {
                    defs.insert(did.clone(), d);
                }
            }
        }
    }
    let local = session
        .override_config
        .get("local_definitions")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    const MAX_SCAN_WINDOW: usize = 20;
    let mut recent: Vec<String> =
        context.iter().rev().take(MAX_SCAN_WINDOW).map(|m| m.content.clone()).collect();
    recent.reverse();

    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_entropy();
    let plan = crate::assembly::assemble_from_nodes(
        &nodes, &defs, &local, &session.current_state, &recent,
        &mut || rand::Rng::gen::<f64>(&mut rng),
    );
    let has_history_node = nodes.iter().any(|n| n.kind == NodeKind::History);
    let include_history = plan.history_enabled || !has_history_node;
    let chat_messages = crate::assembly::build_chat_messages(&plan, context, include_history);

    let regex_rules: Vec<Definition> = storage
        .list_definitions()
        .await?
        .into_iter()
        .filter(|d| d.def_type == "regex_rule")
        .collect();

    Ok((ChatRequest { model, messages: chat_messages }, regex_rules))
}
```

Then rewrite the body of `send_message`'s `async_stream!` from the "1) 落库 user 消息" step through the "build_chat_messages" step so it walks the active path:

```rust
        // 1) parent = 当前激活叶子（沿 active_leaf 的分支末端），落库 user 消息。
        let all = match storage.list_messages(&session_id).await {
            Ok(h) => h,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let path = crate::tree::active_path(&all, session.active_leaf_id.as_deref());
        let parent_id = path.last().map(|m| m.id.clone());
        let user_msg = Message::new(&session_id, parent_id, Role::User, &user_text);
        if let Err(e) = storage.create_message(&user_msg).await {
            yield SendEvent::Error(e.to_string());
            return;
        }

        // 2) 组装：context = 当前分支可见消息 + 本次 user。
        let mut context: Vec<ChatMessage> = path
            .iter()
            .filter(|m| !m.is_hidden)
            .map(|m| ChatMessage { role: m.role, content: m.raw_content.clone() })
            .collect();
        context.push(ChatMessage { role: Role::User, content: user_text.clone() });
        let (req, regex_rules) = match assemble_request(storage.as_ref(), &session, model, &context).await {
            Ok(r) => r,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
```

Keep step 3 (provider stream loop) as-is. In step 4, after persisting the assistant, **set the active leaf**:

```rust
        let mut assistant = Message::new(&session_id, Some(user_msg.id.clone()), Role::Assistant, &full);
        assistant.display_content = crate::assembly::apply_regex_rules(&full, &regex_rules);
        if let Err(e) = storage.create_message(&assistant).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
        let _ = storage.set_session_active_leaf(&session_id, Some(&assistant.id)).await;
        yield SendEvent::Done { message_id: assistant.id };
```

Remove the now-unused old `history`/`last_id`/`recent`/`hist_msgs` lines they replaced. Add `use crate::models::definition::Definition;` if not already imported.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core --lib`
Expected: PASS (new test + existing `echo_send_streams_and_persists`, `send_message_respects_per_entry_recursive`, etc. still green — assembly behavior is unchanged for a linear session).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(core): send_message walks the active path and sets active_leaf"
```

---

## Task 5: Edit + hide a message — `PUT /api/sessions/{id}/messages/{msgId}`

**Files:**
- Create: `shirita-web/src/routes/messages.rs`
- Modify: `shirita-web/src/routes/mod.rs`, `shirita-web/src/lib.rs`
- Test: `shirita-web/tests/message_tree_test.rs`

- [ ] **Step 1: Write the failing test**

Create `shirita-web/tests/message_tree_test.rs` with the standard harness (copy `test_state`, `send`, `json`, `create` from `shirita-web/tests/sessions_mgmt_test.rs`), then add helpers + the first test:

```rust
// after a turn, return (user_id, assistant_id) by reading the message list
async fn turn(state: &AppState, sid: &str, text: &str) {
    let (st, _) = send(state, "POST", &format!("/api/sessions/{sid}/messages"),
        Some(&format!(r#"{{"text":"{text}"}}"#))).await;
    assert_eq!(st, StatusCode::OK); // SSE body collected to completion
}
async fn messages(state: &AppState, sid: &str) -> Value {
    let (_, out) = send(state, "GET", &format!("/api/sessions/{sid}/messages"), None).await;
    json(&out)
}

#[tokio::test]
async fn edit_overwrites_in_place_and_recomputes_display() {
    let state = test_state().await;
    let sid = create(&state, "Chat").await;
    turn(&state, &sid, "hi").await;
    let msgs = messages(&state, &sid);
    let user = msgs.as_array().unwrap().iter().find(|m| m["role"] == "user").unwrap();
    let mid = user["id"].as_str().unwrap();

    let (st, out) = send(&state, "PUT", &format!("/api/sessions/{sid}/messages/{mid}"),
        Some(r#"{"content":"edited"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["raw_content"], "edited");

    let after = messages(&state, &sid);
    let same = after.as_array().unwrap().len();
    assert_eq!(same, 2); // no new branch — in-place edit
}

#[tokio::test]
async fn hide_toggles_is_hidden() {
    let state = test_state().await;
    let sid = create(&state, "Chat").await;
    turn(&state, &sid, "hi").await;
    let msgs = messages(&state, &sid);
    let a = msgs.as_array().unwrap().iter().find(|m| m["role"] == "assistant").unwrap();
    let mid = a["id"].as_str().unwrap();

    let (st, out) = send(&state, "PUT", &format!("/api/sessions/{sid}/messages/{mid}"),
        Some(r#"{"is_hidden":true}"#)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["is_hidden"], true);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-web --test message_tree_test edit_overwrites`
Expected: FAIL — `PUT` route returns 405/404 (handler not registered).

- [ ] **Step 3: Implement the handler**

Create `shirita-web/src/routes/messages.rs`:

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use shirita_core::Message;

use crate::AppState;

#[derive(Deserialize)]
pub struct EditBody {
    pub content: Option<String>,
    pub is_hidden: Option<bool>,
}

/// In-place edit (overwrite `raw_content`, recompute `display_content`) and/or
/// hide toggle. Does not branch (SillyTavern-style edit).
pub async fn edit_message(
    State(state): State<AppState>,
    Path((session_id, msg_id)): Path<(String, String)>,
    Json(body): Json<EditBody>,
) -> Result<Json<Message>, StatusCode> {
    let mut msg = state.storage.get_message(&msg_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if msg.session_id != session_id {
        return Err(StatusCode::NOT_FOUND);
    }
    if let Some(content) = body.content {
        let rules = state.storage.list_definitions().await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter().filter(|d| d.def_type == "regex_rule").collect::<Vec<_>>();
        msg.display_content = shirita_core::apply_regex_rules(&content, &rules);
        msg.raw_content = content;
    }
    if let Some(hidden) = body.is_hidden {
        msg.is_hidden = hidden;
    }
    state.storage.update_message(&msg).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(msg))
}
```

> If `apply_regex_rules` / `Message` aren't already re-exported from `shirita_core`, add them to `shirita-core/src/lib.rs`'s `pub use` list (the crate already re-exports `send_message`, `SendEvent`, etc.).

In `shirita-web/src/routes/mod.rs`, add `pub mod messages;`.

In `shirita-web/src/lib.rs`, register the route (alongside the existing `/sessions/{id}/messages`):

```rust
        .route(
            "/sessions/{id}/messages/{msg_id}",
            axum::routing::put(routes::messages::edit_message),
        )
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-web --test message_tree_test edit_ hide_`
Expected: PASS (both).

- [ ] **Step 5: Commit**

```bash
git add shirita-web/src/routes/messages.rs shirita-web/src/routes/mod.rs shirita-web/src/lib.rs shirita-web/tests/message_tree_test.rs
git commit -m "feat(web): PUT message — in-place edit + hide"
```

---

## Task 6: Switch the active branch — `PUT /api/sessions/{id}/active-leaf`

**Files:**
- Modify: `shirita-web/src/routes/messages.rs`, `shirita-web/src/lib.rs`
- Test: `shirita-web/tests/message_tree_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `message_tree_test.rs`:

```rust
#[tokio::test]
async fn active_leaf_switch_descends_to_deepest_leaf() {
    let state = test_state().await;
    let sid = create(&state, "Chat").await;
    turn(&state, &sid, "hi").await;             // user A -> assistant A
    let msgs = messages(&state, &sid);
    let user_a = msgs.as_array().unwrap().iter().find(|m| m["role"] == "user").unwrap();
    let uid = user_a["id"].as_str().unwrap();

    // point the active leaf back at the user message
    let (st, out) = send(&state, "PUT", &format!("/api/sessions/{sid}/active-leaf"),
        Some(&format!(r#"{{"message_id":"{uid}"}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    // descends to the deepest leaf under the user message = assistant A
    let assistant_a = messages(&state, &sid).as_array().unwrap().iter()
        .find(|m| m["role"] == "assistant").unwrap()["id"].as_str().unwrap().to_string();
    assert_eq!(json(&out)["active_leaf_id"], assistant_a);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-web --test message_tree_test active_leaf_switch`
Expected: FAIL — route not registered.

- [ ] **Step 3: Implement the handler**

Append to `shirita-web/src/routes/messages.rs`:

```rust
use shirita_core::Session;

#[derive(Deserialize)]
pub struct ActiveLeafBody {
    pub message_id: String,
}

/// Move the active branch: descend from `message_id` to its deepest leaf and
/// store that as `active_leaf_id`. Returns the updated session.
pub async fn set_active_leaf(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<ActiveLeafBody>,
) -> Result<Json<Session>, StatusCode> {
    let all = state.storage.list_messages(&session_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !all.iter().any(|m| m.id == body.message_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let leaf = shirita_core::tree::deepest_leaf(&all, &body.message_id);
    state.storage.set_session_active_leaf(&session_id, Some(&leaf)).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let session = state.storage.get_session(&session_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(session))
}
```

> Ensure `pub mod tree;` items are reachable as `shirita_core::tree::deepest_leaf` (the crate exposes modules publicly). Add a re-export if the crate prefers flat paths.

In `shirita-web/src/lib.rs`:

```rust
        .route(
            "/sessions/{id}/active-leaf",
            axum::routing::put(routes::messages::set_active_leaf),
        )
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-web --test message_tree_test active_leaf_switch`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-web/src/routes/messages.rs shirita-web/src/lib.rs shirita-web/tests/message_tree_test.rs
git commit -m "feat(web): PUT active-leaf — switch the active branch"
```

---

## Task 7: Regenerate a sibling — `POST /api/sessions/{id}/messages/{msgId}/regenerate` (SSE)

**Files:**
- Modify: `shirita-core/src/conversation.rs` (add `regenerate`), `shirita-core/src/lib.rs` (re-export)
- Modify: `shirita-web/src/routes/chat.rs`, `shirita-web/src/lib.rs`
- Test: `shirita-web/tests/message_tree_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `message_tree_test.rs`:

```rust
#[tokio::test]
async fn regenerate_creates_a_sibling_and_switches_to_it() {
    let state = test_state().await;
    let sid = create(&state, "Chat").await;
    turn(&state, &sid, "hi").await;
    let msgs = messages(&state, &sid);
    let assistant = msgs.as_array().unwrap().iter().find(|m| m["role"] == "assistant").unwrap();
    let aid = assistant["id"].as_str().unwrap();
    let parent = assistant["parent_id"].as_str().unwrap().to_string();

    let (st, body) = send(&state, "POST",
        &format!("/api/sessions/{sid}/messages/{aid}/regenerate"), Some("{}")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(body.contains(r#""type":"done""#));

    let after = messages(&state, &sid);
    let assistants: Vec<_> = after.as_array().unwrap().iter()
        .filter(|m| m["role"] == "assistant").collect();
    assert_eq!(assistants.len(), 2); // original + regenerated sibling
    // both share the same parent (the user message)
    assert!(assistants.iter().all(|m| m["parent_id"] == parent.as_str()));
    // session active leaf is the new sibling (not the original)
    let s = send(&state, "GET", &format!("/api/sessions/{sid}"), None).await;
    let leaf = json(&s.1)["active_leaf_id"].as_str().unwrap().to_string();
    assert_ne!(leaf, aid);
}
```

> If there is no `GET /api/sessions/{id}` endpoint, read the leaf via the messages instead: assert exactly one assistant has no child and the session’s leaf points at the newest one. Adjust the final two assertions to whatever read path exists.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-web --test message_tree_test regenerate_creates`
Expected: FAIL — regenerate route not registered.

- [ ] **Step 3: Add the `regenerate` core function**

In `shirita-core/src/conversation.rs`, add (mirrors `send_message`, but the context ends at the target's parent and the new assistant is a sibling):

```rust
/// Regenerate a fresh assistant reply as a *sibling* of `target_id` (same
/// parent), then point the active leaf at it. The target must be an assistant
/// message.
pub fn regenerate(
    storage: Arc<dyn Storage>,
    provider: Arc<dyn ModelProvider>,
    _counter: Arc<dyn TokenCounter>,
    model: String,
    session_id: String,
    target_id: String,
) -> impl Stream<Item = SendEvent> {
    async_stream::stream! {
        let session = match storage.get_session(&session_id).await {
            Ok(Some(s)) => s,
            Ok(None) => { yield SendEvent::Error("session not found".into()); return; }
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let all = match storage.list_messages(&session_id).await {
            Ok(h) => h,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        let target = match all.iter().find(|m| m.id == target_id) {
            Some(m) if m.role == Role::Assistant => m.clone(),
            _ => { yield SendEvent::Error("regenerate target must be an assistant message".into()); return; }
        };
        // context = path root→(target's parent = the user turn that prompted it)
        let path = crate::tree::active_path(&all, target.parent_id.as_deref());
        let context: Vec<ChatMessage> = path
            .iter()
            .filter(|m| !m.is_hidden)
            .map(|m| ChatMessage { role: m.role, content: m.raw_content.clone() })
            .collect();
        let (req, regex_rules) = match assemble_request(storage.as_ref(), &session, model, &context).await {
            Ok(r) => r,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };

        let mut full = String::new();
        let mut stream = match provider.stream_chat(req).await {
            Ok(s) => s,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
        while let Some(item) = stream.next().await {
            match item {
                Ok(delta) => { full.push_str(&delta); yield SendEvent::Delta(delta); }
                Err(e) => { yield SendEvent::Error(e.to_string()); return; }
            }
        }
        let mut sibling = Message::new(&session_id, target.parent_id.clone(), Role::Assistant, &full);
        sibling.display_content = crate::assembly::apply_regex_rules(&full, &regex_rules);
        if let Err(e) = storage.create_message(&sibling).await {
            yield SendEvent::Error(e.to_string());
            return;
        }
        let _ = storage.set_session_active_leaf(&session_id, Some(&sibling.id)).await;
        yield SendEvent::Done { message_id: sibling.id };
    }
}
```

Add `pub use conversation::regenerate;` next to the existing `send_message` re-export in `shirita-core/src/lib.rs`.

- [ ] **Step 4: Add the SSE handler**

In `shirita-web/src/routes/chat.rs`, add (the file already maps `SendEvent` to SSE in `send`; factor the mapping or repeat it):

```rust
use shirita_core::regenerate;

pub async fn regenerate_message(
    State(state): State<AppState>,
    Path((session_id, msg_id)): Path<(String, String)>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let events = regenerate(
        state.storage.clone(),
        state.provider.clone(),
        state.token_counter.clone(),
        state.model.clone(),
        session_id,
        msg_id,
    );
    let sse = events.map(|ev| {
        let payload = match ev {
            SendEvent::Delta(text) => json!({ "type": "delta", "text": text }),
            SendEvent::Done { message_id } => json!({ "type": "done", "message_id": message_id }),
            SendEvent::Error(message) => json!({ "type": "error", "message": message }),
        };
        Ok(Event::default().data(payload.to_string()))
    });
    Sse::new(sse)
}
```

In `shirita-web/src/lib.rs`:

```rust
        .route(
            "/sessions/{id}/messages/{msg_id}/regenerate",
            post(routes::chat::regenerate_message),
        )
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p shirita-web --test message_tree_test regenerate_creates`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/conversation.rs shirita-core/src/lib.rs shirita-web/src/routes/chat.rs shirita-web/src/lib.rs shirita-web/tests/message_tree_test.rs
git commit -m "feat: regenerate creates an assistant sibling and switches to it"
```

---

## Task 8: Fork from a node — `POST /api/sessions/{id}/fork`

**Files:**
- Modify: `shirita-web/src/routes/messages.rs`, `shirita-web/src/lib.rs`
- Test: `shirita-web/tests/message_tree_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `message_tree_test.rs`:

```rust
#[tokio::test]
async fn fork_copies_path_to_a_new_isolated_session() {
    let state = test_state().await;
    let sid = create(&state, "Origin").await;
    turn(&state, &sid, "one").await;
    turn(&state, &sid, "two").await; // 4 messages now
    let msgs = messages(&state, &sid);
    // fork at the FIRST assistant (path root→that node = 2 messages)
    let first_assistant = msgs.as_array().unwrap().iter()
        .filter(|m| m["role"] == "assistant")
        .min_by_key(|m| m["created_at"].as_str().unwrap().to_string()).unwrap();
    let node = first_assistant["id"].as_str().unwrap();

    let (st, out) = send(&state, "POST", &format!("/api/sessions/{sid}/fork"),
        Some(&format!(r#"{{"message_id":"{node}"}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    let new_id = json(&out)["id"].as_str().unwrap().to_string();
    assert_ne!(new_id, sid);
    assert_eq!(json(&out)["name"], "Origin (fork)");

    // new session has exactly the 2 messages up to the fork node, new ids
    let forked = messages(&state, &new_id);
    assert_eq!(forked.as_array().unwrap().len(), 2);
    assert!(forked.as_array().unwrap().iter().all(|m| m["id"].as_str().unwrap() != node));
    // its active leaf is set (the copied leaf)
    assert!(json(&out)["active_leaf_id"].is_string());
    // original is untouched (still 4)
    assert_eq!(messages(&state, &sid).as_array().unwrap().len(), 4);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-web --test message_tree_test fork_copies`
Expected: FAIL — fork route not registered.

- [ ] **Step 3: Implement the handler**

Append to `shirita-web/src/routes/messages.rs` (reuse the `clone_messages` pattern from `routes/sessions.rs` — either make that helper `pub(crate)` and import it, or inline the same id-remap loop here):

```rust
use shirita_core::tree::active_path;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct ForkBody {
    pub message_id: String,
}

/// Fork: deep-copy the linear path root→`message_id` (current branch) into a new
/// session; carries template/mounts/override_config; `current_state` = the
/// node's snapshot; `active_leaf_id` = the copied leaf. Original untouched.
pub async fn fork_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<ForkBody>,
) -> Result<Json<Session>, StatusCode> {
    let src = state.storage.get_session(&session_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let all = state.storage.list_messages(&session_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let slice = active_path(&all, Some(&body.message_id));
    if slice.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let node = slice.last().unwrap();

    let mut dup = Session::new(format!("{} (fork)", src.name));
    dup.avatar = src.avatar.clone();
    dup.template_id = src.template_id.clone();
    dup.override_config = src.override_config.clone();
    dup.current_state = node.snapshot_state.clone();
    dup.mounted_definitions = src.mounted_definitions.clone();
    state.storage.create_session(&dup).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = state.storage.copy_nodes(
        &shirita_core::OwnerKind::Session, &session_id,
        &shirita_core::OwnerKind::Session, &dup.id,
    ).await;

    // copy the path messages with fresh ids + remapped parents
    let idmap: HashMap<String, String> =
        slice.iter().map(|m| (m.id.clone(), uuid::Uuid::new_v4().to_string())).collect();
    let mut new_leaf: Option<String> = None;
    for m in &slice {
        let mut nm = (*m).clone();
        nm.id = idmap[&m.id].clone();
        nm.session_id = dup.id.clone();
        nm.parent_id = m.parent_id.as_ref().and_then(|p| idmap.get(p).cloned());
        state.storage.create_message(&nm).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        new_leaf = Some(nm.id.clone());
    }
    let _ = state.storage.set_session_active_leaf(&dup.id, new_leaf.as_deref()).await;
    let out = state.storage.get_session(&dup.id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(out))
}
```

> `OwnerKind` must be reachable as `shirita_core::OwnerKind` (it is used elsewhere in `routes/sessions.rs`). Reuse the same import path that file uses.

In `shirita-web/src/lib.rs`:

```rust
        .route("/sessions/{id}/fork", post(routes::messages::fork_session))
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-web --test message_tree_test fork_copies`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-web/src/routes/messages.rs shirita-web/src/lib.rs shirita-web/tests/message_tree_test.rs
git commit -m "feat(web): POST fork — copy the active path into a new session"
```

---

## Task 9: Per-session generation cancellation

**Files:**
- Create: `shirita-web/src/generations.rs`
- Modify: `shirita-web/src/state.rs`, `shirita-web/src/lib.rs`, `shirita-web/src/routes/chat.rs`
- Test: `shirita-web/src/generations.rs` (inline unit test)

- [ ] **Step 1: Write the failing test (inside the new file)**

Create `shirita-web/src/generations.rs`:

```rust
//! Per-session in-flight generation registry. A newer generation aborts the
//! prior one for the same session, so swiping / re-sending can't leave two
//! streams racing to write siblings or move the active leaf.

use std::collections::HashMap;
use std::sync::Mutex;

use futures::stream::AbortHandle;

#[derive(Default)]
pub struct Generations(Mutex<HashMap<String, AbortHandle>>);

impl Generations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `handle` as the in-flight generation for `session_id`, aborting
    /// and replacing any previous one.
    pub fn replace(&self, session_id: &str, handle: AbortHandle) {
        if let Some(old) = self.0.lock().unwrap().insert(session_id.to_string(), handle) {
            old.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::{self, abortable, StreamExt};

    #[tokio::test]
    async fn a_new_generation_aborts_the_previous_one() {
        let gens = Generations::new();
        let (first, h1) = abortable(stream::pending::<i32>());
        gens.replace("s", h1);

        let (_second, h2) = abortable(stream::iter(vec![1, 2, 3]));
        gens.replace("s", h2); // must abort the first

        assert!(first.is_aborted());
        // a different session is unaffected
        let (third, h3) = abortable(stream::pending::<i32>());
        gens.replace("other", h3);
        assert!(!third.is_aborted());
        let _ = first.collect::<Vec<_>>().await; // ends immediately (aborted)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-web --lib generations`
Expected: FAIL — module not declared in `lib.rs` yet (compile error).

- [ ] **Step 3: Wire the module + AppState**

In `shirita-web/src/lib.rs`, add `pub mod generations;` and re-export: `pub use generations::Generations;`.

In `shirita-web/src/state.rs`, add the field:

```rust
use crate::generations::Generations;
// ...
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage>,
    pub config: Arc<Config>,
    pub provider: Arc<dyn ModelProvider>,
    pub token_counter: Arc<dyn TokenCounter>,
    pub model: String,
    pub generations: Arc<Generations>,
}
```

Construct it wherever `AppState { … }` is built — in `app()`/`main`/tests — adding `generations: Arc::new(Generations::new())`. (For the test harness in `shirita-web/tests/*`, add the same field to `test_state`.)

- [ ] **Step 4: Run the unit test to verify it passes**

Run: `cargo test -p shirita-web --lib generations`
Expected: PASS.

- [ ] **Step 5: Wire cancellation into the SSE handlers**

In `shirita-web/src/routes/chat.rs`, wrap both `send` and `regenerate_message` streams with `abortable` + register. For `send`:

```rust
    let session_id_for_reg = session_id.clone();
    let events = send_message(
        state.storage.clone(), state.provider.clone(), state.token_counter.clone(),
        state.model.clone(), session_id, body.text,
    );
    let (events, handle) = futures::stream::abortable(events);
    state.generations.replace(&session_id_for_reg, handle);

    let sse = events.map(|ev| { /* unchanged mapping */ });
    Sse::new(sse)
```

Do the same in `regenerate_message` (register under its `session_id` before it is moved into `regenerate`). Add `use futures::stream::StreamExt;` if not already imported.

- [ ] **Step 6: Verify the whole workspace is green**

Run: `cargo test --workspace`
Expected: PASS, zero warnings (`cargo build --workspace` clean).

- [ ] **Step 7: Commit**

```bash
git add shirita-web/src/generations.rs shirita-web/src/state.rs shirita-web/src/lib.rs shirita-web/src/routes/chat.rs shirita-web/tests/message_tree_test.rs
git commit -m "feat(web): per-session generation cancellation (new aborts prior)"
```

---

## Self-Review Checklist (run before handing off to execution)

- **Spec coverage:** active_leaf column (T1) ✓; active_path/deepest_leaf (T3) ✓; assembly walks active path + sets leaf (T4) ✓; regenerate sibling (T7) ✓; in-place edit + recompute display (T5) ✓; hide (T5) ✓; active-leaf switch w/ deepest-leaf rule (T6) ✓; fork w/ snapshot + override carry (T8) ✓; cancellation §4.6 (T9) ✓. Deferred per spec: delete-branch, COW (Plan 2), frontend (Plan 1b).
- **Placeholders:** none — every step has concrete code/commands.
- **Type consistency:** `Storage::{get_message, update_message, set_session_active_leaf}` used identically in T2/T5/T6/T7/T8; `tree::{active_path, deepest_leaf}` signatures match across T3/T4/T6/T7/T8; `assemble_request(&dyn Storage, &Session, String, &[ChatMessage])` used by both `send_message` (T4) and `regenerate` (T7); `Generations::replace` matches between T9 def and use.
- **Open verification points for the implementer** (resolve against the real code, don't guess): exact name of the inline test-storage helper in `sqlite.rs`/`conversation.rs`; whether `apply_regex_rules`/`Message`/`OwnerKind`/`tree` need adding to `shirita_core`'s `pub use`; whether a `GET /api/sessions/{id}` read exists for the T7 leaf assertion (fallback noted inline).
```
