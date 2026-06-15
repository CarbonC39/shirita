# M4 Plan 2 — Copy-on-Write (Local Definition / Template Overrides) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a conversation edit its definitions and template tree without touching the global library — edits land as a field-level patch in `override_config.local_definitions` (and as session-owned nodes for the tree), surfaced on the Book page as a **local (this chat) / global** split, with "sync to global" and "revert".

**Architecture:** The assembler already merges `override_config.local_definitions` per field over the global definition (`effective_trigger`/`effective_scan`/`effective_def_content`). Plan 2 adds the write side: endpoints to set/clear a local definition patch, promote it to global, and materialize a session-owned node tree on first local tree edit. The Book page reads the active chat (lifted into the ui store) and renders a **local section** (top) over the **global section** (bottom); the local section edits through the new endpoints and shows a "changed in this chat" chip strip.

**Tech Stack:** Rust/Axum/sqlx backend; Vue 3 + Pinia frontend. Builds on M3 (Book page, DefinitionEditor, PromptTree, node `owner_kind=session`) and Plan 1 (active chat in chat flow). Independent of Plan 1 code-wise.

**Upstream:** `docs/superpowers/specs/2026-06-14-m4-message-tree-design.md` (§5).

---

## File Structure

- `shirita-web/src/routes/local_overrides.rs` — **create**: `set_local_definition`, `clear_local_definition`, `promote_local_definition`, `materialize_nodes`.
- `shirita-web/src/routes/mod.rs`, `shirita-web/src/lib.rs` — **modify**: register routes.
- `shirita-ui/src/stores/ui.ts` — **modify**: hold `activeChatId`.
- `shirita-ui/src/components/AppShell.vue` — **modify**: write `activeChatId` into the ui store.
- `shirita-ui/src/api/client.ts` — **modify**: `setLocalDefinition`, `clearLocalDefinition`, `promoteLocalDefinition`, `materializeNodes`.
- `shirita-ui/src/views/BookView.vue` — **modify**: local/global sections; chip strip; scope-aware saves.
- Tests: `shirita-web/tests/local_overrides_test.rs`; `shirita-ui/src/stores/ui.test.ts` (extend); `shirita-ui/src/views/BookView.test.ts` (create or extend).

---

## Task 1: Local-definition endpoints — set / clear / promote

**Files:**
- Create: `shirita-web/src/routes/local_overrides.rs`
- Modify: `shirita-web/src/routes/mod.rs`, `shirita-web/src/lib.rs`
- Test: `shirita-web/tests/local_overrides_test.rs`

- [ ] **Step 1: Write the failing test**

Create `shirita-web/tests/local_overrides_test.rs` (copy the `test_state`/`send`/`json`/`create` harness from `sessions_mgmt_test.rs`), then:

```rust
async fn create_def(state: &AppState, name: &str, content: &str) -> String {
    let (st, out) = send(state, "POST", "/api/definitions",
        Some(&format!(r#"{{"type":"prompt","name":"{name}","content":"{content}","meta":{{}}}}"#))).await;
    assert_eq!(st, StatusCode::OK);
    json(&out)["id"].as_str().unwrap().to_string()
}
async fn get_session(state: &AppState, sid: &str) -> Value {
    let (_, out) = send(state, "GET", &format!("/api/sessions/{sid}"), None).await;
    json(&out)
}

#[tokio::test]
async fn set_clear_and_promote_local_definition() {
    let state = test_state().await;
    let sid = create(&state, "Chat").await;
    let did = create_def(&state, "Lore", "global text").await;

    // set a local content override
    let (st, _) = send(&state, "PUT", &format!("/api/sessions/{sid}/local-definitions/{did}"),
        Some(r#"{"content":"local text"}"#)).await;
    assert_eq!(st, StatusCode::OK);
    let s = get_session(&state, &sid).await;
    assert_eq!(s["override_config"]["local_definitions"][&did]["content"], "local text");
    // global is untouched
    let (_, gdef) = send(&state, "GET", &format!("/api/definitions/{did}"), None).await;
    assert_eq!(json(&gdef)["content"], "global text");

    // promote -> global takes the local content, override cleared
    let (st2, _) = send(&state, "POST",
        &format!("/api/sessions/{sid}/local-definitions/{did}/promote"), Some("{}")).await;
    assert_eq!(st2, StatusCode::OK);
    let (_, gdef2) = send(&state, "GET", &format!("/api/definitions/{did}"), None).await;
    assert_eq!(json(&gdef2)["content"], "local text");
    let s2 = get_session(&state, &sid).await;
    assert!(s2["override_config"]["local_definitions"].get(&did).is_none()
        || s2["override_config"]["local_definitions"][&did].is_null());

    // set again then clear (revert)
    send(&state, "PUT", &format!("/api/sessions/{sid}/local-definitions/{did}"),
        Some(r#"{"content":"temp"}"#)).await;
    let (st3, _) = send(&state, "DELETE",
        &format!("/api/sessions/{sid}/local-definitions/{did}"), None).await;
    assert_eq!(st3, StatusCode::OK);
    let s3 = get_session(&state, &sid).await;
    assert!(s3["override_config"]["local_definitions"].get(&did).is_none()
        || s3["override_config"]["local_definitions"][&did].is_null());
}
```

> Requires `GET /api/sessions/{id}` (added in Plan 1b Task 1). If Plan 1b hasn't landed, add that tiny handler here first.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-web --test local_overrides_test`
Expected: FAIL — routes not registered.

- [ ] **Step 3: Implement the handlers**

Create `shirita-web/src/routes/local_overrides.rs`:

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

fn ensure_obj(v: &mut Value) -> &mut serde_json::Map<String, Value> {
    if !v.is_object() {
        *v = json!({});
    }
    v.as_object_mut().unwrap()
}

/// Read the session's override_config and mutate `local_definitions` via `f`.
async fn with_local_defs<F>(state: &AppState, session_id: &str, f: F) -> Result<(), StatusCode>
where
    F: FnOnce(&mut serde_json::Map<String, Value>),
{
    let session = state.storage.get_session(session_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let mut cfg = session.override_config.clone();
    let cfg_obj = ensure_obj(&mut cfg);
    let mut locals = cfg_obj.get("local_definitions").cloned().unwrap_or_else(|| json!({}));
    f(ensure_obj(&mut locals));
    cfg_obj.insert("local_definitions".into(), locals);
    state.storage.update_session_override_config(session_id, &cfg).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Write/replace the field-level patch for `def_id` (only the changed fields).
pub async fn set_local_definition(
    State(state): State<AppState>,
    Path((session_id, def_id)): Path<(String, String)>,
    Json(patch): Json<Value>,
) -> Result<StatusCode, StatusCode> {
    with_local_defs(&state, &session_id, |locals| { locals.insert(def_id.clone(), patch); }).await?;
    Ok(StatusCode::OK)
}

/// Revert: drop the local patch for `def_id`.
pub async fn clear_local_definition(
    State(state): State<AppState>,
    Path((session_id, def_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    with_local_defs(&state, &session_id, |locals| { locals.remove(&def_id); }).await?;
    Ok(StatusCode::OK)
}

/// Sync to global: fold the patch into the global definition, then clear it.
pub async fn promote_local_definition(
    State(state): State<AppState>,
    Path((session_id, def_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    let session = state.storage.get_session(&session_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let patch = session.override_config
        .get("local_definitions").and_then(|l| l.get(&def_id)).cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut def = state.storage.get_definition(&def_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;

    if let Some(c) = patch.get("content").and_then(|v| v.as_str()) { def.content = c.to_string(); }
    if let Some(n) = patch.get("name").and_then(|v| v.as_str()) { def.name = n.to_string(); }
    // trigger / scan live under the definition's meta object
    if !def.meta.is_object() { def.meta = json!({}); }
    let meta = def.meta.as_object_mut().unwrap();
    if let Some(t) = patch.get("trigger") { meta.insert("trigger".into(), t.clone()); }
    if let Some(s) = patch.get("scan") { meta.insert("scan".into(), s.clone()); }

    state.storage.update_definition(&def).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    with_local_defs(&state, &session_id, |locals| { locals.remove(&def_id); }).await?;
    Ok(StatusCode::OK)
}
```

In `shirita-web/src/routes/mod.rs` add `pub mod local_overrides;`. In `shirita-web/src/lib.rs`:

```rust
        .route(
            "/sessions/{id}/local-definitions/{def_id}",
            axum::routing::put(routes::local_overrides::set_local_definition)
                .delete(routes::local_overrides::clear_local_definition),
        )
        .route(
            "/sessions/{id}/local-definitions/{def_id}/promote",
            post(routes::local_overrides::promote_local_definition),
        )
```

> `Definition.meta` is `serde_json::Value` (same field DefinitionEditor edits). If it is a typed struct in this codebase, adapt the `meta` mutation to set its `trigger`/`scan` fields instead.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p shirita-web --test local_overrides_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-web/src/routes/local_overrides.rs shirita-web/src/routes/mod.rs shirita-web/src/lib.rs shirita-web/tests/local_overrides_test.rs
git commit -m "feat(web): local-definition override endpoints (set/clear/promote)"
```

---

## Task 2: `materialize-nodes` — copy template tree to session on first local tree edit

**Files:**
- Modify: `shirita-web/src/routes/local_overrides.rs`, `shirita-web/src/lib.rs`
- Test: `shirita-web/tests/local_overrides_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `local_overrides_test.rs`:

```rust
async fn create_template(state: &AppState, name: &str) -> String {
    let (_, out) = send(state, "POST", "/api/templates", Some(&format!(r#"{{"name":"{name}"}}"#))).await;
    json(&out)["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn materialize_copies_template_nodes_once() {
    let state = test_state().await;
    let tid = create_template(&state, "T").await;
    // add one root node to the template
    send(&state, "POST", &format!("/api/templates/{tid}/nodes?owner_kind=template"),
        Some(r#"{"parent_id":null,"kind":"history"}"#)).await;
    // a session using that template
    let (_, sout) = send(&state, "POST", "/api/sessions",
        Some(&format!(r#"{{"name":"Chat","template_id":"{tid}"}}"#))).await;
    let sid = json(&sout)["id"].as_str().unwrap().to_string();

    // before: session has no own nodes
    let (_, before) = send(&state, "GET", &format!("/api/templates/{sid}/nodes?owner_kind=session"), None).await;
    assert_eq!(json(&before).as_array().unwrap().len(), 0);

    let (st, _) = send(&state, "POST", &format!("/api/sessions/{sid}/materialize-nodes"), Some("{}")).await;
    assert_eq!(st, StatusCode::OK);

    let (_, after) = send(&state, "GET", &format!("/api/templates/{sid}/nodes?owner_kind=session"), None).await;
    assert_eq!(json(&after).as_array().unwrap().len(), 1);

    // idempotent: a second call doesn't double the tree
    send(&state, "POST", &format!("/api/sessions/{sid}/materialize-nodes"), Some("{}")).await;
    let (_, after2) = send(&state, "GET", &format!("/api/templates/{sid}/nodes?owner_kind=session"), None).await;
    assert_eq!(json(&after2).as_array().unwrap().len(), 1);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-web --test local_overrides_test materialize_copies`
Expected: FAIL — route not registered.

- [ ] **Step 3: Implement**

Append to `shirita-web/src/routes/local_overrides.rs`:

```rust
use shirita_core::OwnerKind;

/// Ensure the session owns a node tree: if it has none yet, deep-copy its
/// template's nodes into `owner_kind=session`. Idempotent.
pub async fn materialize_nodes(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let existing = state.storage.list_nodes(&OwnerKind::Session, &session_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !existing.is_empty() {
        return Ok(StatusCode::OK); // already materialized
    }
    let session = state.storage.get_session(&session_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    if let Some(tid) = session.template_id.as_deref() {
        let _ = state.storage.copy_nodes(
            &OwnerKind::Template, tid, &OwnerKind::Session, &session_id,
        ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(StatusCode::OK)
}
```

In `shirita-web/src/lib.rs`:

```rust
        .route("/sessions/{id}/materialize-nodes", post(routes::local_overrides::materialize_nodes))
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p shirita-web --test local_overrides_test materialize_copies`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-web/src/routes/local_overrides.rs shirita-web/src/lib.rs shirita-web/tests/local_overrides_test.rs
git commit -m "feat(web): materialize-nodes — copy template tree to session (idempotent)"
```

---

## Task 3: Lift `activeChatId` to the ui store + COW client functions

**Files:**
- Modify: `shirita-ui/src/stores/ui.ts`, `shirita-ui/src/components/AppShell.vue`, `shirita-ui/src/api/client.ts`
- Test: `shirita-ui/src/stores/ui.test.ts` (extend)

- [ ] **Step 1: Write the failing test**

Add to `shirita-ui/src/stores/ui.test.ts`:

```ts
it('tracks the active chat id', () => {
  const ui = useUiStore()
  expect(ui.activeChatId).toBeNull()
  ui.setActiveChatId('abc')
  expect(ui.activeChatId).toBe('abc')
  ui.setActiveChatId(null)
  expect(ui.activeChatId).toBeNull()
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/stores/ui.test.ts`
Expected: FAIL — `activeChatId`/`setActiveChatId` missing.

- [ ] **Step 3: Add to the ui store**

In `shirita-ui/src/stores/ui.ts`, add a ref + setter and include them in the returned object:

```ts
  const activeChatId = ref<string | null>(null)
  function setActiveChatId(id: string | null) { activeChatId.value = id }
```

(add `activeChatId, setActiveChatId` to the store's `return { ... }`).

- [ ] **Step 4: Point AppShell at the store**

In `shirita-ui/src/components/AppShell.vue`, replace the local `activeChatId` ref with the store value:

```ts
const activeChatId = computed(() => ui.activeChatId)
watch(
  () => route.fullPath,
  () => {
    if (route.name === 'chat') ui.setActiveChatId(route.params.id as string)
    else if (route.path === '/') ui.setActiveChatId(null)
  },
  { immediate: true },
)
```

(`chatTo` keeps using `activeChatId.value`.)

- [ ] **Step 5: Add the client functions**

In `shirita-ui/src/api/client.ts`:

```ts
export async function setLocalDefinition(sessionId: string, defId: string, patch: Record<string, unknown>): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/local-definitions/${defId}`, {
    method: 'PUT', headers: { ...authHeaders(), 'Content-Type': 'application/json' }, body: JSON.stringify(patch),
  })
  if (!res.ok) throw new Error(`Set local definition failed: ${res.status}`)
}
export async function clearLocalDefinition(sessionId: string, defId: string): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/local-definitions/${defId}`, { method: 'DELETE', headers: authHeaders() })
  if (!res.ok) throw new Error(`Clear local definition failed: ${res.status}`)
}
export async function promoteLocalDefinition(sessionId: string, defId: string): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/local-definitions/${defId}/promote`, { method: 'POST', headers: authHeaders() })
  if (!res.ok) throw new Error(`Promote failed: ${res.status}`)
}
export async function materializeNodes(sessionId: string): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/materialize-nodes`, { method: 'POST', headers: authHeaders() })
  if (!res.ok) throw new Error(`Materialize nodes failed: ${res.status}`)
}
```

- [ ] **Step 6: Typecheck + run tests + commit**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npx vitest run src/stores/ui.test.ts`
Expected: PASS.

```bash
git add shirita-ui/src/stores/ui.ts shirita-ui/src/components/AppShell.vue shirita-ui/src/api/client.ts shirita-ui/src/stores/ui.test.ts
git commit -m "feat(ui): activeChatId in ui store + copy-on-write client fns"
```

---

## Task 4: Book page — local (this chat) / global sections

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue`
- Test: `shirita-ui/src/views/BookView.test.ts` (create)

**Design:** Refactor the existing single-scope Book body into a reusable inner unit, then render it twice when `ui.activeChatId` is set: a **local** instance (top, bound to the active chat) and a **global** instance (bottom, today's behavior). When there is no active chat, render only global (unchanged).

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/views/BookView.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import { useUiStore } from '../stores/ui'

vi.mock('../api/client', () => ({
  listNodes: vi.fn().mockResolvedValue([]),
  getSession: vi.fn().mockResolvedValue({ id: 'c1', template_id: null, override_config: {} }),
  // ...stub every client fn BookView imports, returning empty/resolved values
}))
vi.mock('../stores/library', () => ({
  useLibraryStore: () => ({
    templates: [], definitions: [], containerTypes: [],
    loadTemplates: vi.fn(), loadDefinitions: vi.fn(), loadTypes: vi.fn(),
  }),
}))

import BookView from './BookView.vue'

describe('BookView scopes', () => {
  beforeEach(() => setActivePinia(createPinia()))

  it('shows only the global section when there is no active chat', async () => {
    const ui = useUiStore(); ui.setActiveChatId(null)
    const w = mount(BookView)
    await flushPromises()
    expect(w.find('[data-test="book-local"]').exists()).toBe(false)
    expect(w.find('[data-test="book-global"]').exists()).toBe(true)
  })

  it('shows the local section above global when a chat is active', async () => {
    const ui = useUiStore(); ui.setActiveChatId('c1')
    const w = mount(BookView)
    await flushPromises()
    expect(w.find('[data-test="book-local"]').exists()).toBe(true)
    expect(w.find('[data-test="book-global"]').exists()).toBe(true)
  })
})

// import { flushPromises } from '@vue/test-utils'
```

> Stub all `../api/client` imports BookView uses (mirror its import list) so mounting doesn't hit the network.

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/views/BookView.test.ts`
Expected: FAIL — no `book-local`/`book-global` markers.

- [ ] **Step 3: Wrap the existing body in a `book-global` section + conditional `book-local`**

In `shirita-ui/src/views/BookView.vue`:
- Read the ui store: `import { useUiStore } from '../stores/ui'; const ui = useUiStore()`.
- Wrap the current template body (template picker + name + PromptTree + DefinitionEditor) in `<section data-test="book-global"> … </section>`.
- Above it, add the local section, shown only when active:

```html
    <section v-if="ui.activeChatId" data-test="book-local" class="mb-6">
      <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2.5">局部 · This conversation</h3>
      <!-- local chip strip (Task 5) + local DefinitionEditor / tree (Task 5/6) -->
    </section>
    <div v-if="ui.activeChatId" class="h-px bg-line my-6" />
```

For this task the local section can be a stub container; Tasks 5 and 6 fill it. Add `data-test="book-global"` to the existing section.

- [ ] **Step 4: Run to verify it passes**

Run: `cd shirita-ui && npx vitest run src/views/BookView.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/BookView.vue shirita-ui/src/views/BookView.test.ts
git commit -m "feat(ui): Book page local/global sections (scaffold)"
```

---

## Task 5: Local section — scope-aware definition editing + "changed in this chat" chip strip

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue`

- [ ] **Step 1: Load the active chat's session + locals**

In `BookView.vue` script, when `ui.activeChatId` is set (watch it, immediate), fetch the session and keep its `override_config.local_definitions`:

```ts
import { getSession, setLocalDefinition, clearLocalDefinition, promoteLocalDefinition } from '../api/client'
import type { Session } from '../api/types'

const localSession = ref<Session | null>(null)
const localDefs = computed<Record<string, Record<string, unknown>>>(() =>
  (localSession.value?.override_config as any)?.local_definitions ?? {},
)
async function loadLocal() {
  if (!ui.activeChatId) { localSession.value = null; return }
  localSession.value = await getSession(ui.activeChatId)
}
watch(() => ui.activeChatId, loadLocal, { immediate: true })
```

- [ ] **Step 2: Render the "changed in this chat" chip strip (only when non-empty)**

Inside `<section data-test="book-local">`, above the local editor:

```html
      <div v-if="Object.keys(localDefs).length" data-test="local-chips" class="flex flex-wrap items-center gap-2 mb-3">
        <span class="text-[12px] text-muted">本对话已改</span>
        <span v-for="(_patch, defId) in localDefs" :key="defId" class="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-2.5 py-1 text-[12px]">
          <button class="text-ink" @click="editLocal(defId)">{{ defName(defId) }}</button>
          <button class="text-muted hover:text-primary" title="同步到全局" @click="promoteLocal(defId)">↥</button>
          <button class="text-muted hover:text-coral" title="还原为全局" @click="revertLocal(defId)">×</button>
        </span>
      </div>
```

Helpers:

```ts
function defName(defId: string): string {
  return library.definitions.find((d) => d.id === defId)?.name ?? defId
}
async function promoteLocal(defId: string) {
  if (!ui.activeChatId) return
  if (!confirm('Sync this definition to the global library?')) return
  await promoteLocalDefinition(ui.activeChatId, defId)
  await Promise.all([library.loadDefinitions(), loadLocal()])
}
async function revertLocal(defId: string) {
  if (!ui.activeChatId) return
  await clearLocalDefinition(ui.activeChatId, defId)
  await loadLocal()
}
function editLocal(defId: string) { /* select def into the local editor (Step 3) */ }
```

- [ ] **Step 3: Local DefinitionEditor writes patches (not the global library)**

Render a `DefinitionEditor` (reuse the component) inside the local section bound to a local working copy. On save in local scope, send only the changed fields as a patch via `setLocalDefinition`, then reload:

```ts
const localEditDef = reactive<Definition>(blankDef())
function editLocal(defId: string) {
  const base = library.definitions.find((d) => d.id === defId)
  const patch = localDefs.value[defId] ?? {}
  if (base) Object.assign(localEditDef, { ...base, ...patch, meta: { ...base.meta, ...(patch as any) } })
}
async function saveLocal() {
  if (!ui.activeChatId || !localEditDef.id) return
  // patch = fields that differ from the global definition
  const base = library.definitions.find((d) => d.id === localEditDef.id)
  const patch: Record<string, unknown> = {}
  if (base && localEditDef.content !== base.content) patch.content = localEditDef.content
  if (base && localEditDef.name !== base.name) patch.name = localEditDef.name
  const t = (localEditDef.meta as any).trigger; if (t) patch.trigger = t
  const s = (localEditDef.meta as any).scan; if (s) patch.scan = s
  await setLocalDefinition(ui.activeChatId, localEditDef.id, patch)
  await loadLocal()
}
```

Wire a `DefinitionEditor` instance in the local section to `localEditDef` with `@save="saveLocal"` and `@select-definition` calling `editLocal`. (Mirror the bindings the global `DefinitionEditor` already uses; only `@save` differs.)

- [ ] **Step 4: Typecheck + run the Book test + commit**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npx vitest run src/views/BookView.test.ts`
Expected: PASS (extend the test to assert the chip strip appears once a local override exists, by stubbing `getSession` to return `override_config.local_definitions` with one entry).

```bash
git add shirita-ui/src/views/BookView.vue shirita-ui/src/views/BookView.test.ts
git commit -m "feat(ui): local definition editing + changed-in-this-chat chips"
```

---

## Task 6: Local tree edits materialize session-owned nodes

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue`

- [ ] **Step 1: Local PromptTree on the session's owned tree**

In the local section, render the active chat's tree. Load it with `owner_kind=session`:

```ts
import { materializeNodes } from '../api/client'
const localNodes = ref<PromptNode[]>([])
async function loadLocalNodes() {
  if (!ui.activeChatId) { localNodes.value = []; return }
  localNodes.value = await listNodes('session', ui.activeChatId)
}
watch(() => ui.activeChatId, loadLocalNodes, { immediate: true })
```

If `localNodes` is empty, the local tree shows the template's nodes read-only with an "Edit locally" affordance; the first edit calls `materialize` then re-reads:

```ts
async function ensureMaterialized() {
  if (!ui.activeChatId) return
  if (localNodes.value.length === 0) {
    await materializeNodes(ui.activeChatId)
    await loadLocalNodes()
  }
}
```

- [ ] **Step 2: Route local node mutations through `owner_kind=session`**

Add session-scoped variants of the BookView node handlers (add/delete/reorder/toggle/content/trigger) that (a) `await ensureMaterialized()` first, then (b) call the same `createNode`/`updateNode`/`deleteNode`/`reorderNodes` client fns with `'session'` + `ui.activeChatId`, then (c) `await loadLocalNodes()`. Bind a `PromptTree` in the local section to `localNodes` with these handlers.

```ts
async function localAddPrompt(definitionId: string) {
  await ensureMaterialized()
  await createNode('session', ui.activeChatId!, { parent_id: null, kind: 'ref', definition_id: definitionId })
  await loadLocalNodes()
}
// ...mirror the other handlers (addContainer, addRefToContainer, toggleEnabled,
//    updateContent, updateTrigger, deleteNode, reorder) the same way.
```

> These mirror the existing global handlers in BookView one-for-one, differing only by `'session'` owner kind, `ui.activeChatId` owner id, `ensureMaterialized()` first, and `loadLocalNodes()` after. The content/trigger handlers update the local *definition* override (Task 5 `setLocalDefinition`), not the global definition.

- [ ] **Step 3: Typecheck + commit**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npx vitest run`
Expected: PASS (no regressions).

```bash
git add shirita-ui/src/views/BookView.vue
git commit -m "feat(ui): local template-tree edits materialize session-owned nodes"
```

---

## Task 7: Manual verification (browser)

- [ ] **Step 1:** Open a chat, go to Book → a **局部 · This conversation** section appears above **Global**; the local section defaults to the chat's template + definitions.
- [ ] **Step 2:** Edit a definition's content in the local section → a "本对话已改" chip appears; the global library entry is unchanged (check the global section / a second chat).
- [ ] **Step 3:** Click the chip's **同步到全局** (confirm) → global now shows the edited content, the chip disappears.
- [ ] **Step 4:** Edit again, then **还原为全局** → the local override is gone, the chat falls back to global.
- [ ] **Step 5:** Edit a node in the local tree → the session materializes its own node tree (the global template is untouched; verify by opening the template in the global section).
- [ ] **Step 6:** Leave the chat (return to the list) → Book shows only the global section.

---

## Self-Review Checklist

- **Spec coverage (§5):** field-level patch write (T1) ✓; promote → global + clear (T1) ✓; revert (T1) ✓; materialize session nodes, idempotent (T2) ✓; active chat in store (T3) ✓; local/global sections (T4) ✓; chip strip only when non-empty + sync/revert (T5) ✓; local definition edits as patches (T5) ✓; local tree edits via session nodes (T6) ✓.
- **Placeholders:** none for logic; the local-section Vue wiring in T5/T6 reuses existing BookView handlers (explicitly: same calls, `'session'` owner + `ensureMaterialized`).
- **Type consistency:** `setLocalDefinition(sessionId, defId, patch)` / `clearLocalDefinition` / `promoteLocalDefinition` / `materializeNodes` match between T3 defs and T5/T6 use; `ui.activeChatId`/`setActiveChatId` match T3 def and T4/T5/T6 use; backend `with_local_defs` mutation helper reused by set/clear/promote.
- **Open verification points:** confirm `Definition.meta` is `serde_json::Value` (promote merge, T1); confirm BookView's exact node-handler names to mirror (T6); the local `DefinitionEditor` reuses the component's existing prop/emit contract (only `@save` differs).
```
