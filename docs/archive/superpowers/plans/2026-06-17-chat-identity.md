# Chat Identity (avatars + display names) + session rename — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show real per-side identity (display name + avatar) for the assistant and user in chat, sourced from definitions/session with optional branch-variable overrides, and let users rename a chat title.

**Architecture:** A pure core resolver picks the identity `char`/`persona` definition (name-matches-template, else first-in-tree). A web read endpoint exposes `{assistant,user}` name+avatar; a PATCH endpoint renames/re-avatars a session. The frontend layers branch-variable overrides (`$avatar`, `$assistant_name`) from already-fetched snapshot state, then renders identity in the header and message rows.

**Tech Stack:** Rust (axum, sqlx), Vue 3 + TS (Pinia, vue-i18n), vitest, cargo test.

## Global Constraints

- No DB migration: persona avatar lives in `definition.meta.avatar`; `$assistant_name` is a code-declared system variable (old snapshots backfill initials via `effective_state`).
- Code comments and git commit messages in English.
- Any new i18n key MUST be added to all four locales (`en`, `zh-Hans`, `zh-Hant`, `ja`); `en` is the source schema or `locales/parity.test.ts` fails.
- No `v-html`.
- Avatar value form is the asset relative path (same as `session.avatar` / `$avatar`); the frontend renders it as `/assets/${path}`.
- Precedence — assistant name: `$assistant_name` › char-def name › i18n `chat.assistant`; assistant avatar: `$avatar` › `session.avatar` › placeholder; user name: persona-def name › i18n `chat.you`; user avatar: persona-def `meta.avatar` › none.

---

### Task 1: Add `$assistant_name` system variable (core)

**Files:**
- Modify: `shirita-core/src/state.rs` (`system_variables()` and its test)

**Interfaces:**
- Produces: `$assistant_name` present in `system_variables()` (String, scope "system", empty initial).

- [ ] **Step 1: Add an assertion to the existing system-variable test**

In `shirita-core/src/state.rs`, find the test asserting `names.contains(&"$avatar")` and add below it:

```rust
        assert!(names.contains(&"$assistant_name"));
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p shirita-core state:: 2>&1 | tail -20`
Expected: FAIL — `$assistant_name` not present.

- [ ] **Step 3: Add the variable**

In `system_variables()`, after the `$background` `VarDecl`, add:

```rust
        VarDecl {
            name: "$assistant_name".into(),
            var_type: VarType::String,
            initial: Value::String(String::new()),
            scope: Some("system".into()),
        },
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core state:: 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/state.rs
git commit -m "feat(core): add \$assistant_name system variable"
```

---

### Task 2: `resolve_identity` pure resolver (core)

**Files:**
- Create: `shirita-core/src/identity.rs`
- Modify: `shirita-core/src/lib.rs` (add `pub mod identity;`)

**Interfaces:**
- Consumes: `models::prompt_node::PromptNode` (fields `kind: NodeKind`, `enabled: bool`, `definition_id: Option<String>`), `models::definition::Definition` (`def_type: String`, `name: String`, `meta: serde_json::Value`).
- Produces:
  ```rust
  pub struct SideIdentity { pub name: Option<String>, pub avatar: Option<String> }
  pub struct Identity { pub assistant: SideIdentity, pub user: SideIdentity }
  pub fn resolve_identity(
      nodes: &[PromptNode],
      defs: &std::collections::HashMap<String, Definition>,
      template_name: Option<&str>,
      session_avatar: Option<&str>,
  ) -> Identity
  ```

- [ ] **Step 1: Write the failing test**

Create `shirita-core/src/identity.rs`:

```rust
//! Resolve per-side chat identity (display name + avatar) from a session's
//! definitions. Pure: the web layer gathers nodes/defs/template and calls this.

use std::collections::HashMap;

use serde::Serialize;

use crate::models::definition::Definition;
use crate::models::prompt_node::{NodeKind, PromptNode};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SideIdentity {
    pub name: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Identity {
    pub assistant: SideIdentity,
    pub user: SideIdentity,
}

/// Pick the identity definition of `def_type` among enabled ref nodes (in tree
/// order): the one whose name equals `template_name`, else the first.
fn pick<'a>(
    nodes: &[PromptNode],
    defs: &'a HashMap<String, Definition>,
    def_type: &str,
    template_name: Option<&str>,
) -> Option<&'a Definition> {
    let candidates: Vec<&Definition> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Ref && n.enabled)
        .filter_map(|n| n.definition_id.as_ref())
        .filter_map(|id| defs.get(id))
        .filter(|d| d.def_type == def_type)
        .collect();
    if let Some(tn) = template_name {
        if let Some(m) = candidates.iter().find(|d| d.name == tn) {
            return Some(m);
        }
    }
    candidates.into_iter().next()
}

pub fn resolve_identity(
    nodes: &[PromptNode],
    defs: &HashMap<String, Definition>,
    template_name: Option<&str>,
    session_avatar: Option<&str>,
) -> Identity {
    let assistant_name = pick(nodes, defs, "char", template_name).map(|d| d.name.clone());
    let persona = pick(nodes, defs, "persona", template_name);
    Identity {
        assistant: SideIdentity {
            name: assistant_name,
            avatar: session_avatar.map(|s| s.to_string()),
        },
        user: SideIdentity {
            name: persona.map(|d| d.name.clone()),
            avatar: persona
                .and_then(|d| d.meta.get("avatar"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::prompt_node::{OwnerKind, PromptNode};

    fn refn(def_id: &str, sort: i64, enabled: bool) -> PromptNode {
        let mut n = PromptNode::new_ref(OwnerKind::Template, "t", None, sort, def_id);
        n.enabled = enabled;
        n
    }

    fn def(id: &str, ty: &str, name: &str, avatar: Option<&str>) -> Definition {
        let mut d = Definition::new(ty, name, "");
        d.id = id.to_string();
        if let Some(a) = avatar {
            d.meta = serde_json::json!({ "avatar": a });
        }
        d
    }

    fn map(defs: Vec<Definition>) -> HashMap<String, Definition> {
        defs.into_iter().map(|d| (d.id.clone(), d)).collect()
    }

    #[test]
    fn assistant_name_prefers_template_name_match() {
        let nodes = vec![refn("d1", 0, true), refn("d2", 1, true)];
        let defs = map(vec![
            def("d1", "char", "Neo·personality", None),
            def("d2", "char", "Neo", None),
        ]);
        let id = resolve_identity(&nodes, &defs, Some("Neo"), Some("a.png"));
        assert_eq!(id.assistant.name.as_deref(), Some("Neo"));
        assert_eq!(id.assistant.avatar.as_deref(), Some("a.png"));
    }

    #[test]
    fn falls_back_to_first_char_and_reads_persona_avatar() {
        let nodes = vec![refn("p", 0, true), refn("c", 1, true)];
        let defs = map(vec![
            def("p", "persona", "Me", Some("u.png")),
            def("c", "char", "Alice", None),
        ]);
        let id = resolve_identity(&nodes, &defs, Some("Mismatch"), None);
        assert_eq!(id.assistant.name.as_deref(), Some("Alice")); // first char
        assert_eq!(id.user.name.as_deref(), Some("Me"));
        assert_eq!(id.user.avatar.as_deref(), Some("u.png"));
    }

    #[test]
    fn no_definitions_yields_nulls() {
        let id = resolve_identity(&[], &HashMap::new(), None, None);
        assert_eq!(id.assistant.name, None);
        assert_eq!(id.user.name, None);
        assert_eq!(id.user.avatar, None);
    }

    #[test]
    fn disabled_ref_is_ignored() {
        let nodes = vec![refn("c", 0, false)];
        let defs = map(vec![def("c", "char", "Ghost", None)]);
        let id = resolve_identity(&nodes, &defs, None, None);
        assert_eq!(id.assistant.name, None);
    }
}
```

- [ ] **Step 2: Wire the module and run the test (verify fail → compile error/missing module)**

Add to `shirita-core/src/lib.rs` near the other `pub mod` lines:

```rust
pub mod identity;
```

Run: `cargo test -p shirita-core identity:: 2>&1 | tail -25`
Expected: the test module compiles and tests PASS (this resolver is self-contained). If any assertion fails, fix `identity.rs` until green. (Confirm `PromptNode::new_ref` / `NodeKind::Ref` names by checking `models/prompt_node.rs`; adjust if the enum variant differs.)

- [ ] **Step 3: Run the full core suite**

Run: `cargo test -p shirita-core 2>&1 | tail -6`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add shirita-core/src/identity.rs shirita-core/src/lib.rs
git commit -m "feat(core): resolve_identity (char/persona name + avatar)"
```

---

### Task 3: `PATCH /sessions/{id}` — rename / re-avatar (web + storage)

**Files:**
- Modify: `shirita-core/src/storage/mod.rs` (trait), `shirita-core/src/storage/sqlite.rs` (impl)
- Modify: `shirita-web/src/routes/sessions.rs` (handler), `shirita-web/src/lib.rs` (route)
- Test: `shirita-web/tests/sessions_mgmt_test.rs`

**Interfaces:**
- Produces: `Storage::update_session_profile(session_id: &str, name: &str, avatar: Option<&str>)`; `PATCH /api/sessions/{id}` body `{ name?: string, avatar?: string|null }` → updated `Session`.

- [ ] **Step 1: Write the failing web test**

Append to `shirita-web/tests/sessions_mgmt_test.rs`:

```rust
#[tokio::test]
async fn patch_renames_session() {
    let state = test_state().await;
    let id = create(&state, "Old").await;
    let (st, out) = send(&state, "PATCH", &format!("/api/sessions/{id}"), Some(r#"{"name":"New"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["name"], "New");
    let (_, got) = send(&state, "GET", &format!("/api/sessions/{id}"), None).await;
    assert_eq!(json(&got)["name"], "New");
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p shirita-web --test sessions_mgmt_test patch_renames_session 2>&1 | tail -20`
Expected: FAIL (405/404 — route not present).

- [ ] **Step 3: Add the storage trait method + impl**

In `shirita-core/src/storage/mod.rs`, after `set_mounted_definitions`:

```rust
    async fn update_session_profile(&self, session_id: &str, name: &str, avatar: Option<&str>) -> Result<()>;
```

In `shirita-core/src/storage/sqlite.rs`, alongside the other `chat_sessions` updates:

```rust
    async fn update_session_profile(&self, session_id: &str, name: &str, avatar: Option<&str>) -> Result<()> {
        let now = crate::now_rfc3339();
        sqlx::query("UPDATE chat_sessions SET name = ?, avatar = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(avatar)
            .bind(now)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```

(Check the timestamp helper actually used in this file — e.g. `crate::now_rfc3339()` or an inline `OffsetDateTime` — and match it; reuse whatever `set_session_active_leaf`/`reorder_sessions` use.)

- [ ] **Step 4: Add the handler + route**

In `shirita-web/src/routes/sessions.rs`:

```rust
#[derive(Deserialize)]
pub struct PatchSession {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub avatar: Option<Option<String>>,
}

pub async fn patch_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PatchSession>,
) -> Result<Json<Session>, StatusCode> {
    let mut session = state
        .storage
        .get_session(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if let Some(name) = body.name {
        session.name = name;
    }
    if let Some(avatar) = body.avatar {
        session.avatar = avatar;
    }
    state
        .storage
        .update_session_profile(&id, &session.name, session.avatar.as_deref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(session))
}
```

If a `double_option` helper isn't already available, replace the `avatar` field with `pub avatar: Option<String>` and `if let Some(a) = body.avatar { session.avatar = Some(a); }` (simpler; cannot clear to null, which is fine for v1).

In `shirita-web/src/lib.rs`, extend the `/sessions/{id}` route:

```rust
        .route(
            "/sessions/{id}",
            get(routes::sessions::get_session)
                .patch(routes::sessions::patch_session)
                .delete(routes::sessions::delete_session),
        )
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p shirita-web --test sessions_mgmt_test patch_renames_session 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 6: Run the full web suite**

Run: `cargo test -p shirita-web 2>&1 | grep "test result:" | tail -1` and `cargo check --workspace 2>&1 | tail -3`
Expected: all green (no other Storage impls to update besides sqlite).

- [ ] **Step 7: Commit**

```bash
git add shirita-core/src/storage/mod.rs shirita-core/src/storage/sqlite.rs shirita-web/src/routes/sessions.rs shirita-web/src/lib.rs shirita-web/tests/sessions_mgmt_test.rs
git commit -m "feat(web): PATCH /sessions/:id to rename / re-avatar a session"
```

---

### Task 4: `GET /sessions/{id}/identity` (web)

**Files:**
- Modify: `shirita-web/src/routes/sessions.rs` (handler), `shirita-web/src/lib.rs` (route)
- Test: `shirita-web/tests/sessions_test.rs`

**Interfaces:**
- Consumes: `shirita_core::identity::{resolve_identity, Identity}`, `shirita_core::conversation::effective_nodes`.
- Produces: `GET /api/sessions/{id}/identity` → `{ assistant:{name,avatar}, user:{name,avatar} }`.

- [ ] **Step 1: Write the failing web test**

Append to `shirita-web/tests/sessions_test.rs`:

```rust
#[tokio::test]
async fn identity_resolves_char_name_and_session_avatar() {
    let state = test_state().await;
    // char def named after the template + a persona with an avatar
    let (_, c) = send(&state, "POST", "/api/definitions", Some(r#"{"type":"char","name":"Neo","content":"desc"}"#)).await;
    let cid = json(&c)["id"].as_str().unwrap().to_string();
    let (_, p) = send(&state, "POST", "/api/definitions", Some(r#"{"type":"persona","name":"Me","content":"","meta":{"avatar":"u.png"}}"#)).await;
    let pid = json(&p)["id"].as_str().unwrap().to_string();
    let (_, t) = send(&state, "POST", "/api/templates", Some(r#"{"name":"Neo"}"#)).await;
    let tid = json(&t)["id"].as_str().unwrap().to_string();
    for did in [&cid, &pid] {
        let body = format!(r#"{{"kind":"ref","definition_id":"{did}"}}"#);
        send(&state, "POST", &format!("/api/templates/{tid}/nodes?owner_kind=template"), Some(&body)).await;
    }
    let (_, s) = send(&state, "POST", "/api/sessions", Some(&format!(r#"{{"name":"chat","template_id":"{tid}","avatar":"face.png"}}"#))).await;
    let sid = json(&s)["id"].as_str().unwrap().to_string();

    let (st, out) = send(&state, "GET", &format!("/api/sessions/{sid}/identity"), None).await;
    assert_eq!(st, StatusCode::OK);
    let v = json(&out);
    assert_eq!(v["assistant"]["name"], "Neo");
    assert_eq!(v["assistant"]["avatar"], "face.png");
    assert_eq!(v["user"]["name"], "Me");
    assert_eq!(v["user"]["avatar"], "u.png");
}

#[tokio::test]
async fn identity_is_null_without_a_template() {
    let state = test_state().await;
    let (_, s) = send(&state, "POST", "/api/sessions", Some(r#"{"name":"free"}"#)).await;
    let sid = json(&s)["id"].as_str().unwrap().to_string();
    let (st, out) = send(&state, "GET", &format!("/api/sessions/{sid}/identity"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(json(&out)["assistant"]["name"].is_null());
}
```

If `sessions_test.rs` has no `json()` helper, add `fn json(s: &str) -> serde_json::Value { serde_json::from_str(s).unwrap() }`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-web --test sessions_test identity 2>&1 | tail -20`
Expected: FAIL (404 — route missing).

- [ ] **Step 3: Add the handler**

In `shirita-web/src/routes/sessions.rs` (imports at top: `use shirita_core::identity::resolve_identity; use shirita_core::conversation::effective_nodes; use std::collections::HashMap;`):

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
    let nodes = effective_nodes(state.storage.as_ref(), &session)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut defs = HashMap::new();
    for n in &nodes {
        if let Some(did) = &n.definition_id {
            if !defs.contains_key(did) {
                if let Ok(Some(d)) = state.storage.get_definition(did).await {
                    defs.insert(did.clone(), d);
                }
            }
        }
    }
    let template_name = match &session.template_id {
        Some(tid) => state.storage.get_template(tid).await.ok().flatten().map(|t| t.name),
        None => None,
    };
    let identity = resolve_identity(&nodes, &defs, template_name.as_deref(), session.avatar.as_deref());
    Ok(Json(identity))
}
```

- [ ] **Step 4: Wire the route**

In `shirita-web/src/lib.rs`, near the other `/sessions/{id}/...` routes:

```rust
        .route("/sessions/{id}/identity", get(routes::sessions::get_session_identity))
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p shirita-web --test sessions_test identity 2>&1 | tail -15`
Expected: PASS (both).

- [ ] **Step 6: Commit**

```bash
git add shirita-web/src/routes/sessions.rs shirita-web/src/lib.rs shirita-web/tests/sessions_test.rs
git commit -m "feat(web): GET /sessions/:id/identity"
```

---

### Task 5: API client + types (ui)

**Files:**
- Modify: `shirita-ui/src/api/types.ts`, `shirita-ui/src/api/client.ts`
- Test: `shirita-ui/src/api/client.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export interface SideIdentity { name: string | null; avatar: string | null }
  export interface Identity { assistant: SideIdentity; user: SideIdentity }
  export function getSessionIdentity(id: string): Promise<Identity>
  export function patchSession(id: string, body: { name?: string; avatar?: string | null }): Promise<Session>
  ```

- [ ] **Step 1: Write the failing test**

Append to `shirita-ui/src/api/client.test.ts` (mirror the existing `mockFetch`/`vi.stubGlobal` pattern already in this file):

```ts
it('getSessionIdentity GETs /api/sessions/:id/identity', async () => {
  const body = { assistant: { name: 'Neo', avatar: 'a.png' }, user: { name: 'Me', avatar: null } }
  vi.stubGlobal('fetch', mockFetch(200, body))
  const { getSessionIdentity } = await import('./client')
  await expect(getSessionIdentity('s1')).resolves.toEqual(body)
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts 2>&1 | tail -8`
Expected: FAIL — `getSessionIdentity` is not a function.

- [ ] **Step 3: Add types + client functions**

In `shirita-ui/src/api/types.ts`:

```ts
export interface SideIdentity {
  name: string | null
  avatar: string | null
}

export interface Identity {
  assistant: SideIdentity
  user: SideIdentity
}
```

In `shirita-ui/src/api/client.ts` (import `Identity` in the type import block; add near the session functions):

```ts
export function getSessionIdentity(id: string): Promise<Identity> {
  return apiGet<Identity>(`/sessions/${id}/identity`)
}

export async function patchSession(id: string, body: { name?: string; avatar?: string | null }): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions/${id}`, {
    method: 'PATCH',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Patch session failed: ${res.status}`)
  return res.json()
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `npx vitest run src/api/client.test.ts 2>&1 | tail -6`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/api/client.ts shirita-ui/src/api/client.test.ts
git commit -m "feat(ui): getSessionIdentity + patchSession client"
```

---

### Task 6: Persona avatar picker in DefinitionEditor (ui)

**Files:**
- Modify: `shirita-ui/src/components/DefinitionEditor.vue`
- Test: `shirita-ui/src/components/DefinitionEditor.test.ts`

**Interfaces:**
- Produces: when `definition.type === 'persona'`, an `AssetPicker` writes `emit('update:meta', { ...meta, avatar })`.

- [ ] **Step 1: Write the failing test**

Append to `shirita-ui/src/components/DefinitionEditor.test.ts`:

```ts
describe('DefinitionEditor persona avatar', () => {
  it('shows an avatar picker for persona and emits update:meta on pick', async () => {
    const d = { id: 'p1', type: 'persona', name: 'Me', content: '', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true } })
    const picker = w.findComponent({ name: 'AssetPicker' })
    expect(picker.exists()).toBe(true)
    picker.vm.$emit('update:model-value', 'u.png')
    await w.vm.$nextTick()
    const ev = w.emitted('update:meta')
    expect(ev).toBeTruthy()
    expect((ev![ev!.length - 1][0] as Record<string, unknown>).avatar).toBe('u.png')
  })

  it('does not show the avatar picker for a char definition', () => {
    const d = { id: 'c1', type: 'char', name: 'Neo', content: '', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true } })
    expect(w.findComponent({ name: 'AssetPicker' }).exists()).toBe(false)
  })
})
```

(If `AssetPicker` lacks a `name`, target it via a `data-test="persona-avatar"` wrapper added in Step 3 and assert on that instead.)

- [ ] **Step 2: Run to verify it fails**

Run: `npx vitest run src/components/DefinitionEditor.test.ts 2>&1 | tail -12`
Expected: FAIL — no AssetPicker for persona.

- [ ] **Step 3: Add the picker**

In `DefinitionEditor.vue` script, ensure `AssetPicker` is imported:

```ts
import AssetPicker from './AssetPicker.vue'
```

In the template, inside the `v-if="active"` body (near the scan/meta controls), add:

```html
<div v-if="definition.type === 'persona'" data-test="persona-avatar" class="mb-3">
  <label class="text-[12px] text-muted block mb-1.5">{{ $t('definition.avatar') }}</label>
  <AssetPicker
    shape="circle"
    :model-value="(definition.meta as any).avatar || ''"
    @update:model-value="emit('update:meta', { ...definition.meta, avatar: $event })"
  />
</div>
```

Add the i18n key `definition.avatar` to ALL four locales (value e.g. en `'Avatar'`, zh-Hans `'头像'`, zh-Hant `'頭像'`, ja `'アバター'`). Confirm the `definition` block exists in each locale; if not, add the key under the appropriate existing section and keep the four files in parity.

- [ ] **Step 4: Run tests (component + i18n parity) to verify pass**

Run: `npx vitest run src/components/DefinitionEditor.test.ts src/locales/parity.test.ts 2>&1 | tail -8`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/DefinitionEditor.vue shirita-ui/src/components/DefinitionEditor.test.ts shirita-ui/src/locales/*.ts
git commit -m "feat(ui): persona avatar picker (meta.avatar) in DefinitionEditor"
```

---

### Task 7: Render identity in MessageItem (ui)

**Files:**
- Modify: `shirita-ui/src/components/MessageItem.vue`
- Test: `shirita-ui/src/components/MessageItem.test.ts`

**Interfaces:**
- Consumes: `Identity` from `../api/types`.
- Produces: optional prop `identity?: Identity`; assistant rows render `identity.assistant` name/avatar, user rows render `identity.user`; falls back to i18n names + placeholder circle.

- [ ] **Step 1: Write the failing test**

Append to `shirita-ui/src/components/MessageItem.test.ts`:

```ts
const identity = { assistant: { name: 'Neo', avatar: 'a.png' }, user: { name: 'Me', avatar: 'u.png' } }

it('renders the assistant identity avatar and name in bubble mode', () => {
  const w = mount(MessageItem, {
    props: { message: makeMsg({ role: 'assistant' }), style: 'bubble', identity },
  })
  const img = w.find('[data-test="assistant-avatar"] img')
  expect(img.exists()).toBe(true)
  expect(img.attributes('src')).toContain('a.png')
})

it('uses the identity names in flat mode', () => {
  const a = mount(MessageItem, { props: { message: makeMsg({ role: 'assistant' }), style: 'flat', identity } })
  expect(a.text()).toContain('Neo')
  const u = mount(MessageItem, { props: { message: makeMsg({ role: 'user' }), style: 'flat', identity } })
  expect(u.text()).toContain('Me')
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `npx vitest run src/components/MessageItem.test.ts 2>&1 | tail -12`
Expected: FAIL — no img / names are still "Assistant"/"You".

- [ ] **Step 3: Implement**

In `MessageItem.vue` script, add to imports and props:

```ts
import type { Message, Identity } from '../api/types'
```
```ts
const props = withDefaults(defineProps<{
  message: Message
  style: 'bubble' | 'flat'
  isStreaming?: boolean
  siblingIndex?: number
  siblingCount?: number
  identity?: Identity
}>(), { siblingCount: 1, siblingIndex: 0 })
```

Add computeds (after the existing `label`):

```ts
const side = computed(() => (isAssistant.value ? props.identity?.assistant : props.identity?.user))
const displayName = computed(() => side.value?.name || (isAssistant.value ? t('chat.assistant') : t('chat.you')))
const avatarUrl = computed(() => (side.value?.avatar ? `/assets/${side.value.avatar}` : ''))
```

Replace `label` usages in the flat header with `displayName`. Replace the bubble assistant avatar placeholder (`<div data-test="assistant-avatar" class="...bg-sky/40..." />`) with:

```html
<div v-if="isAssistant" data-test="assistant-avatar" class="w-8 h-8 rounded-full bg-sky/40 shrink-0 mt-0.5 overflow-hidden">
  <img v-if="avatarUrl" :src="avatarUrl" class="w-full h-full object-cover" alt="" />
</div>
```

In the flat header circle, wrap an `<img v-if="avatarUrl" ...>` similarly inside the existing colored circle div.

- [ ] **Step 4: Run to verify it passes (and nothing regressed)**

Run: `npx vitest run src/components/MessageItem.test.ts 2>&1 | tail -8`
Expected: PASS (existing tests still pass — `identity` is optional and defaults to i18n names).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/MessageItem.vue shirita-ui/src/components/MessageItem.test.ts
git commit -m "feat(ui): render per-side identity (name + avatar) in MessageItem"
```

---

### Task 8: Pass identity through MessageList (ui)

**Files:**
- Modify: `shirita-ui/src/components/MessageList.vue`
- Test: `shirita-ui/src/components/MessageList.test.ts`

**Interfaces:**
- Produces: optional prop `identity?: Identity`, forwarded to every `MessageItem`.

- [ ] **Step 1: Write the failing test**

Append to `shirita-ui/src/components/MessageList.test.ts` (mirror its existing mount/fixtures):

```ts
it('forwards identity to message items', () => {
  const identity = { assistant: { name: 'Neo', avatar: null }, user: { name: 'Me', avatar: null } }
  const messages = [{ id: 'm1', session_id: 's', parent_id: null, role: 'assistant', raw_content: 'hi', display_content: null, is_hidden: false, is_anchor: false, snapshot_state: {}, created_at: '1' }]
  const w = mount(MessageList, { props: { messages, style: 'flat', identity } })
  expect(w.text()).toContain('Neo')
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `npx vitest run src/components/MessageList.test.ts 2>&1 | tail -10`
Expected: FAIL — shows "Assistant", not "Neo".

- [ ] **Step 3: Implement**

In `MessageList.vue` add `identity?: Identity` to `defineProps` (import the type), and pass `:identity="identity"` on both `<MessageItem>` usages (the list item and the streaming item).

- [ ] **Step 4: Run to verify it passes**

Run: `npx vitest run src/components/MessageList.test.ts 2>&1 | tail -6`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/MessageList.vue shirita-ui/src/components/MessageList.test.ts
git commit -m "feat(ui): forward identity through MessageList"
```

---

### Task 9: ChatView — fetch identity, apply overrides, header (ui)

**Files:**
- Modify: `shirita-ui/src/views/ChatView.vue`
- Test: `shirita-ui/src/views/ChatView.test.ts`

**Interfaces:**
- Consumes: `getSessionIdentity`, `Identity`; `sessionState.values['$avatar']`, `['$assistant_name']`.
- Produces: an `effectiveIdentity` computed passed to `MessageList` and shown in the header.

- [ ] **Step 1: Write the failing test**

Append to `shirita-ui/src/views/ChatView.test.ts` (mirror existing `vi.spyOn(client, ...)` setup; also stub `getSessionIdentity`):

```ts
it('shows the character name in the header from identity', async () => {
  vi.spyOn(client, 'listMessages').mockResolvedValue([])
  vi.spyOn(client, 'getSessionIdentity').mockResolvedValue({ assistant: { name: 'Neo', avatar: 'a.png' }, user: { name: null, avatar: null } })
  const router = makeRouter(); router.push('/chat/s1'); await router.isReady()
  const w = mount(ChatView, { global: { plugins: [router] } })
  await flushPromises()
  expect(w.text()).toContain('Neo')
})
```

Ensure any other client methods ChatView calls on mount (e.g. `getSessionState`, `getSession`) are stubbed in this file's `beforeEach` as they already are.

- [ ] **Step 2: Run to verify it fails**

Run: `npx vitest run src/views/ChatView.test.ts 2>&1 | tail -12`
Expected: FAIL — header shows the generic title.

- [ ] **Step 3: Implement**

In `ChatView.vue` script: import `getSessionIdentity` and `Identity`; add:

```ts
const identity = ref<Identity>({ assistant: { name: null, avatar: null }, user: { name: null, avatar: null } })
async function loadIdentity() {
  try { identity.value = await getSessionIdentity(sessionId) } catch { /* keep fallback */ }
}
const effectiveIdentity = computed<Identity>(() => {
  const v = sessionState.value.values
  const dyn = (k: string) => (typeof v[k] === 'string' && v[k] ? (v[k] as string) : null)
  return {
    assistant: {
      name: dyn('$assistant_name') ?? identity.value.assistant.name,
      avatar: dyn('$avatar') ?? identity.value.assistant.avatar,
    },
    user: identity.value.user,
  }
})
const headerName = computed(() => effectiveIdentity.value.assistant.name || t('chat.title'))
const headerAvatar = computed(() => {
  const a = effectiveIdentity.value.assistant.avatar
  return a ? `/assets/${a}` : ''
})
```

Call `loadIdentity()` in `onMounted` (alongside `loadState()`). Replace the header avatar source and title:

```html
<img v-if="headerAvatar" :src="headerAvatar" class="w-6 h-6 rounded-full object-cover shrink-0" alt="" />
<span class="font-semibold text-ink truncate">{{ headerName }}</span>
```

Pass identity to the list: `<MessageList ... :identity="effectiveIdentity" />`.

- [ ] **Step 4: Run to verify it passes**

Run: `npx vitest run src/views/ChatView.test.ts 2>&1 | tail -8`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/ChatView.vue shirita-ui/src/views/ChatView.test.ts
git commit -m "feat(ui): chat header + messages use resolved identity with var overrides"
```

---

### Task 10: Rename a session from the home list (ui)

**Files:**
- Modify: `shirita-ui/src/components/ChatCard.vue`, `shirita-ui/src/views/HomeView.vue`, `shirita-ui/src/stores/sessions.ts`, all four `shirita-ui/src/locales/*.ts`
- Test: `shirita-ui/src/views/HomeView.test.ts`

**Interfaces:**
- Consumes: `patchSession`.
- Produces: ChatCard `rename: [id: string, name: string]` event; `sessions` store `rename(id, name)`.

- [ ] **Step 1: Write the failing test**

Append to `shirita-ui/src/views/HomeView.test.ts` (mirror its store-mocking setup):

```ts
it('renames a session via the store', async () => {
  const spy = vi.spyOn(api, 'patchSession').mockResolvedValue({} as never)
  const store = useSessionsStore()
  await store.rename('s1', 'New title')
  expect(spy).toHaveBeenCalledWith('s1', { name: 'New title' })
})
```

(Import `useSessionsStore` and `* as api` to match the file's existing imports.)

- [ ] **Step 2: Run to verify it fails**

Run: `npx vitest run src/views/HomeView.test.ts 2>&1 | tail -10`
Expected: FAIL — `store.rename` is not a function.

- [ ] **Step 3: Implement store + UI**

In `shirita-ui/src/stores/sessions.ts` add (import `patchSession`):

```ts
async function rename(id: string, name: string) {
  await patchSession(id, { name })
  const s = items.value.find((x) => x.id === id)
  if (s) s.name = name
}
```
Add `rename` to the store's returned object. (Match the store's actual state variable name for the list — e.g. `items`/`sessions`.)

In `ChatCard.vue`: add `rename: [id: string, name: string]` to `defineEmits`, and a "Rename" item in the `⋯` menu that prompts/inline-edits and emits:

```html
<button class="w-full flex items-center gap-2 px-3 py-2 text-[13px] text-ink hover:bg-surface text-left transition-colors"
        @click="act($event, () => { const n = prompt($t('common.rename'), session.name); if (n && n.trim()) emit('rename', session.id, n.trim()) })">
  <Pencil :size="14" /> {{ $t('common.rename') }}
</button>
```
(Import `Pencil` from `lucide-vue-next`.)

In `HomeView.vue`: bind `@rename="(id, name) => store.rename(id, name)"` on `<ChatCard>`.

Add i18n key `common.rename` to all four locales (en `'Rename'`, zh-Hans `'重命名'`, zh-Hant `'重新命名'`, ja `'名前を変更'`).

- [ ] **Step 4: Run tests (incl. parity) to verify pass**

Run: `npx vitest run src/views/HomeView.test.ts src/locales/parity.test.ts 2>&1 | tail -8`
Expected: PASS.

- [ ] **Step 5: Full suite + build**

Run: `npx vitest run 2>&1 | tail -4 && npm run build 2>&1 | tail -3`
Expected: all UI tests pass; build succeeds.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/components/ChatCard.vue shirita-ui/src/views/HomeView.vue shirita-ui/src/stores/sessions.ts shirita-ui/src/views/HomeView.test.ts shirita-ui/src/locales/*.ts
git commit -m "feat(ui): rename a chat from the home-list menu"
```

---

## Final verification

- [ ] `cargo test -p shirita-core -p shirita-web 2>&1 | grep "test result:"` — all pass
- [ ] `cd shirita-ui && npx vitest run 2>&1 | tail -4` — all pass
- [ ] `cd shirita-ui && npm run build 2>&1 | tail -3` — build succeeds
- [ ] Manual smoke (optional, via /run): import a card, start a chat → header + assistant bubbles show the character name/avatar; set a persona avatar in Book → user bubbles show it; rename a chat from the home list.
