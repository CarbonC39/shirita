# M4 Plan 1b — Message Tree (Frontend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the chat UI navigate the message tree — render only the active branch, show real `‹ n/m ›` swipes on regenerated assistants, and wire regenerate / in-place edit / hide / fork to the Plan 1a endpoints.

**Architecture:** A pure `utils/tree.ts` (mirroring core `active_path`) turns `(messages, activeLeafId)` into the displayed branch + per-message sibling info. The chat store tracks `activeLeafId` (seeded from the session, updated from endpoint responses), exposes the active path, and gains actions for swipe/regenerate/edit/hide/fork. `MessageItem` (which already has the indicator + buttons as placeholders) gets real swipe props and an inline edit field. Fork navigates to the new session.

**Tech Stack:** Vue 3 `<script setup>` + TS, Pinia, vue-router, Vitest. Depends on Plan 1a's backend endpoints.

**Scope note:** Frontend half of M4 subsystem A. Backend = Plan 1a (must land first). Copy-on-write = Plan 2.

**Upstream:** `docs/superpowers/specs/2026-06-14-m4-message-tree-design.md` (§4.5), Plan 1a.

---

## File Structure

- `shirita-web/src/routes/sessions.rs` + `shirita-web/src/lib.rs` — **modify**: add `GET /api/sessions/{id}` (one tiny handler) so the UI can read the persisted `active_leaf_id` on load.
- `shirita-ui/src/api/types.ts` — **modify**: add `active_leaf_id` to `Session`.
- `shirita-ui/src/api/client.ts` — **modify**: `getSession`, `editMessage`, `setActiveLeaf`, `regenerateMessage` (SSE), `forkSession`.
- `shirita-ui/src/utils/tree.ts` — **create**: `activePath`, `siblings`.
- `shirita-ui/src/stores/chat.ts` — **modify**: `activeLeafId`, `displayed` path, `swipe`/`regenerate`/`editMessage`/`toggleHidden`/`fork` actions.
- `shirita-ui/src/components/MessageItem.vue` — **modify**: real swipe props/emit, inline edit, hide button, edit available on user rows too.
- `shirita-ui/src/components/MessageList.vue` — **modify**: pass sibling info + new emits through.
- `shirita-ui/src/views/ChatView.vue` — **modify**: render the active path, wire all actions, fork-navigate.
- Tests: `shirita-ui/src/utils/tree.test.ts`, `shirita-ui/src/stores/chat.test.ts`, `shirita-ui/src/components/MessageItem.test.ts` (extend existing).

---

## Task 1: `GET /api/sessions/{id}` + `getSession` client + `active_leaf_id` type

**Files:**
- Modify: `shirita-web/src/routes/sessions.rs`, `shirita-web/src/lib.rs`
- Modify: `shirita-ui/src/api/types.ts`, `shirita-ui/src/api/client.ts`
- Test: `shirita-web/tests/message_tree_test.rs` (extend)

- [ ] **Step 1: Write the failing backend test**

Add to `shirita-web/tests/message_tree_test.rs`:

```rust
#[tokio::test]
async fn get_session_returns_active_leaf() {
    let state = test_state().await;
    let sid = create(&state, "Chat").await;
    turn(&state, &sid, "hi").await;
    let (st, out) = send(&state, "GET", &format!("/api/sessions/{sid}"), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(json(&out)["name"], "Chat");
    assert!(json(&out)["active_leaf_id"].is_string());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-web --test message_tree_test get_session_returns`
Expected: FAIL — no `GET /api/sessions/{id}` route (405/404).

- [ ] **Step 3: Implement the handler + route**

In `shirita-web/src/routes/sessions.rs` add:

```rust
pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Session>, StatusCode> {
    state.storage.get_session(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}
```

In `shirita-web/src/lib.rs`, extend the `/sessions/{id}` route (it currently only has `delete`) to add `get`:

```rust
        .route(
            "/sessions/{id}",
            axum::routing::get(routes::sessions::get_session)
                .delete(routes::sessions::delete_session),
        )
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p shirita-web --test message_tree_test get_session_returns`
Expected: PASS.

- [ ] **Step 5: Add the TS type + client fn**

In `shirita-ui/src/api/types.ts`, add to `Session`:

```ts
  /** Leaf message of the active branch (set by the message-tree endpoints). */
  active_leaf_id?: string | null
```

In `shirita-ui/src/api/client.ts`, add near the session functions:

```ts
export function getSession(id: string): Promise<Session> {
  return apiGet<Session>(`/sessions/${id}`)
}
```

- [ ] **Step 6: Typecheck + commit**

Run: `cd shirita-ui && npx vue-tsc --noEmit` → clean.

```bash
git add shirita-web/src/routes/sessions.rs shirita-web/src/lib.rs shirita-web/tests/message_tree_test.rs shirita-ui/src/api/types.ts shirita-ui/src/api/client.ts
git commit -m "feat: GET /sessions/{id} + getSession client + active_leaf_id type"
```

---

## Task 2: Client functions for the message-tree endpoints

**Files:**
- Modify: `shirita-ui/src/api/client.ts`

- [ ] **Step 1: Add the functions**

In `shirita-ui/src/api/client.ts` (after `sendMessage`):

```ts
export async function editMessage(
  sessionId: string,
  msgId: string,
  patch: { content?: string; is_hidden?: boolean },
): Promise<Message> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/messages/${msgId}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(patch),
  })
  if (!res.ok) throw new Error(`Edit message failed: ${res.status}`)
  return res.json()
}

export async function setActiveLeaf(sessionId: string, messageId: string): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/active-leaf`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ message_id: messageId }),
  })
  if (!res.ok) throw new Error(`Set active leaf failed: ${res.status}`)
  return res.json()
}

export async function forkSession(sessionId: string, messageId: string): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/fork`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ message_id: messageId }),
  })
  if (!res.ok) throw new Error(`Fork failed: ${res.status}`)
  return res.json()
}

/** SSE regenerate — same event shape as sendMessage. */
export async function* regenerateMessage(
  sessionId: string,
  msgId: string,
): AsyncGenerator<SseEvent> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/messages/${msgId}/regenerate`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: '{}',
  })
  if (!res.ok) throw new Error(`Regenerate failed: ${res.status}`)
  if (!res.body) throw new Error('No response body for SSE stream')
  const reader = res.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''
      for (const line of lines) {
        if (line.startsWith('data: ')) yield JSON.parse(line.slice(6)) as SseEvent
      }
    }
  } finally {
    reader.releaseLock()
  }
}
```

> The SSE read loop duplicates `sendMessage`'s. Optional DRY: extract `async function* readSse(res: Response)` and have both call it. Acceptable either way.

- [ ] **Step 2: Typecheck + commit**

Run: `cd shirita-ui && npx vue-tsc --noEmit` → clean.

```bash
git add shirita-ui/src/api/client.ts
git commit -m "feat(ui): client fns — editMessage/setActiveLeaf/forkSession/regenerateMessage"
```

---

## Task 3: Pure `utils/tree.ts` — active path + siblings

**Files:**
- Create: `shirita-ui/src/utils/tree.ts`
- Test: `shirita-ui/src/utils/tree.test.ts`

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/utils/tree.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { activePath, siblings } from './tree'
import type { Message } from '../api/types'

function m(id: string, parent: string | null, created: string, role: Message['role'] = 'assistant'): Message {
  return { id, session_id: 's', parent_id: parent, role, raw_content: id, display_content: null, is_hidden: false, snapshot_state: {}, created_at: created }
}

describe('activePath', () => {
  it('walks root to the active leaf', () => {
    const ms = [m('a', null, '1', 'user'), m('b', 'a', '2'), m('b2', 'a', '3')]
    expect(activePath(ms, 'b2').map((x) => x.id)).toEqual(['a', 'b2'])
  })
  it('falls back to the newest message when leaf is null', () => {
    const ms = [m('a', null, '1', 'user'), m('b', 'a', '2')]
    expect(activePath(ms, null).map((x) => x.id)).toEqual(['a', 'b'])
  })
})

describe('siblings', () => {
  it('lists same-parent nodes ordered by created_at', () => {
    const ms = [m('a', null, '1', 'user'), m('b', 'a', '3'), m('b2', 'a', '2')]
    const sib = siblings(ms, ms[1])
    expect(sib.map((x) => x.id)).toEqual(['b2', 'b'])
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/utils/tree.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

Create `shirita-ui/src/utils/tree.ts`:

```ts
import type { Message } from '../api/types'

function newest(messages: Message[]): Message | null {
  if (messages.length === 0) return null
  return messages.reduce((a, b) =>
    a.created_at > b.created_at || (a.created_at === b.created_at && a.id > b.id) ? a : b,
  )
}

/** Root→active-leaf branch. Falls back to the newest message when leaf unknown. */
export function activePath(messages: Message[], activeLeafId: string | null): Message[] {
  const byId = new Map(messages.map((m) => [m.id, m]))
  let cur: Message | null = (activeLeafId ? byId.get(activeLeafId) : undefined) ?? newest(messages)
  const path: Message[] = []
  while (cur) {
    path.push(cur)
    cur = cur.parent_id ? byId.get(cur.parent_id) ?? null : null
  }
  return path.reverse()
}

/** Same-parent siblings of `msg`, ordered created_at asc then id (swipe order). */
export function siblings(messages: Message[], msg: Message): Message[] {
  return messages
    .filter((m) => (m.parent_id ?? null) === (msg.parent_id ?? null))
    .sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id))
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd shirita-ui && npx vitest run src/utils/tree.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/utils/tree.ts shirita-ui/src/utils/tree.test.ts
git commit -m "feat(ui): pure activePath + siblings tree helpers"
```

---

## Task 4: Chat store — active leaf, displayed path, tree actions

**Files:**
- Modify: `shirita-ui/src/stores/chat.ts`
- Test: `shirita-ui/src/stores/chat.test.ts` (create)

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/stores/chat.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

vi.mock('../api/client', () => ({
  listMessages: vi.fn(),
  getSession: vi.fn(),
  sendMessage: vi.fn(),
  regenerateMessage: vi.fn(),
  editMessage: vi.fn(),
  setActiveLeaf: vi.fn(),
  forkSession: vi.fn(),
}))

import { useChatStore } from './chat'
import * as api from '../api/client'
import type { Message } from '../api/types'

function m(id: string, parent: string | null, created: string, role: Message['role'] = 'assistant'): Message {
  return { id, session_id: 's', parent_id: parent, role, raw_content: id, display_content: null, is_hidden: false, snapshot_state: {}, created_at: created }
}

describe('chat store active path', () => {
  beforeEach(() => setActivePinia(createPinia()))

  it('displays only the active branch and seeds the leaf from the session', async () => {
    ;(api.listMessages as any).mockResolvedValue([m('a', null, '1', 'user'), m('b', 'a', '2'), m('b2', 'a', '3')])
    ;(api.getSession as any).mockResolvedValue({ id: 's', active_leaf_id: 'b2' })
    const store = useChatStore()
    await store.loadMessages('s')
    expect(store.displayed.map((x: Message) => x.id)).toEqual(['a', 'b2'])
  })

  it('switchLeaf updates the leaf from the endpoint response', async () => {
    ;(api.listMessages as any).mockResolvedValue([m('a', null, '1', 'user'), m('b', 'a', '2'), m('b2', 'a', '3')])
    ;(api.getSession as any).mockResolvedValue({ id: 's', active_leaf_id: 'b2' })
    ;(api.setActiveLeaf as any).mockResolvedValue({ id: 's', active_leaf_id: 'b' })
    const store = useChatStore()
    await store.loadMessages('s')
    await store.switchLeaf('b')
    expect(store.activeLeafId).toBe('b')
    expect(store.displayed.map((x: Message) => x.id)).toEqual(['a', 'b'])
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/stores/chat.test.ts`
Expected: FAIL — `displayed` / `switchLeaf` / `activeLeafId` don't exist.

- [ ] **Step 3: Extend the store**

Rewrite `shirita-ui/src/stores/chat.ts` to add the leaf + path + actions (keep existing streaming logic):

```ts
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { Message } from '../api/types'
import {
  listMessages, getSession, sendMessage, regenerateMessage,
  editMessage, setActiveLeaf, forkSession,
} from '../api/client'
import { activePath } from '../utils/tree'

export const useChatStore = defineStore('chat', () => {
  const messages = ref<Message[]>([])
  const activeLeafId = ref<string | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)
  const isStreaming = ref(false)
  const streamingText = ref('')
  const streamingError = ref<string | null>(null)
  const activeSessionId = ref<string | null>(null)

  const displayed = computed(() => activePath(messages.value, activeLeafId.value))

  async function loadMessages(sessionId: string) {
    loading.value = true
    error.value = null
    activeSessionId.value = sessionId
    try {
      const [msgs, session] = await Promise.all([listMessages(sessionId), getSession(sessionId)])
      messages.value = msgs
      activeLeafId.value = session.active_leaf_id ?? null
    } catch (e) {
      error.value = (e as Error).message
    } finally {
      loading.value = false
    }
  }

  async function consume(stream: AsyncGenerator<{ type: string; text?: string; message?: string }>, sessionId: string) {
    isStreaming.value = true
    streamingText.value = ''
    streamingError.value = null
    try {
      for await (const event of stream as any) {
        if (event.type === 'delta') streamingText.value += event.text
        else if (event.type === 'done') { streamingText.value = ''; await loadMessages(sessionId) }
        else if (event.type === 'error') { streamingError.value = event.message; isStreaming.value = false; return }
      }
    } catch (e) {
      streamingError.value = (e as Error).message
    } finally {
      isStreaming.value = false
    }
  }

  async function send(sessionId: string, text: string) {
    await consume(sendMessage(sessionId, text), sessionId)
  }
  async function regenerate(sessionId: string, msgId: string) {
    await consume(regenerateMessage(sessionId, msgId), sessionId)
  }
  async function switchLeaf(messageId: string) {
    if (!activeSessionId.value) return
    const s = await setActiveLeaf(activeSessionId.value, messageId)
    activeLeafId.value = s.active_leaf_id ?? null
  }
  async function editMsg(msgId: string, content: string) {
    if (!activeSessionId.value) return
    const updated = await editMessage(activeSessionId.value, msgId, { content })
    const i = messages.value.findIndex((m) => m.id === msgId)
    if (i !== -1) messages.value = [...messages.value.slice(0, i), updated, ...messages.value.slice(i + 1)]
  }
  async function toggleHidden(msgId: string) {
    if (!activeSessionId.value) return
    const m = messages.value.find((x) => x.id === msgId)
    if (!m) return
    const updated = await editMessage(activeSessionId.value, msgId, { is_hidden: !m.is_hidden })
    const i = messages.value.findIndex((x) => x.id === msgId)
    if (i !== -1) messages.value = [...messages.value.slice(0, i), updated, ...messages.value.slice(i + 1)]
  }
  async function fork(msgId: string): Promise<string | null> {
    if (!activeSessionId.value) return null
    const s = await forkSession(activeSessionId.value, msgId)
    return s.id
  }

  function clearStreaming() {
    isStreaming.value = false
    streamingText.value = ''
    streamingError.value = null
  }

  return {
    messages, activeLeafId, displayed, loading, error,
    isStreaming, streamingText, streamingError, activeSessionId,
    loadMessages, send, regenerate, switchLeaf, editMsg, toggleHidden, fork, clearStreaming,
  }
})
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd shirita-ui && npx vitest run src/stores/chat.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/stores/chat.ts shirita-ui/src/stores/chat.test.ts
git commit -m "feat(ui): chat store tracks active leaf + tree actions"
```

---

## Task 5: `MessageItem` — real swipe, inline edit, hide

**Files:**
- Modify: `shirita-ui/src/components/MessageItem.vue`
- Test: `shirita-ui/src/components/MessageItem.test.ts` (extend)

- [ ] **Step 1: Write the failing test**

Add to `shirita-ui/src/components/MessageItem.test.ts`:

```ts
import { mount } from '@vue/test-utils'
import MessageItem from './MessageItem.vue'

it('shows the real swipe count and emits swipe on the arrows', async () => {
  const msg = { id: 'b2', session_id: 's', parent_id: 'a', role: 'assistant', raw_content: 'hi', display_content: null, is_hidden: false, snapshot_state: {}, created_at: '2' }
  const w = mount(MessageItem, { props: { message: msg, style: 'bubble', siblingIndex: 1, siblingCount: 2 } })
  expect(w.find('[data-test="swipe-indicator"]').text()).toContain('2/2')
  await w.find('[data-test="swipe-prev"]').trigger('click')
  expect(w.emitted('swipe')![0]).toEqual([-1])
})

it('edits in place and emits edit-save', async () => {
  const msg = { id: 'u', session_id: 's', parent_id: null, role: 'user', raw_content: 'hello', display_content: null, is_hidden: false, snapshot_state: {}, created_at: '1' }
  const w = mount(MessageItem, { props: { message: msg, style: 'flat' } })
  await w.find('[data-test="edit-btn"]').trigger('click')
  const ta = w.find('[data-test="edit-area"]')
  await ta.setValue('hello edited')
  await w.find('[data-test="edit-save"]').trigger('click')
  expect(w.emitted('edit-save')![0]).toEqual(['hello edited'])
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/MessageItem.test.ts`
Expected: FAIL — swipe is hardcoded `1/1`; no `swipe-prev`/`edit-area`/`edit-save`.

- [ ] **Step 3: Update the component**

Edit `shirita-ui/src/components/MessageItem.vue`:

Script — add props/emit + local edit state:

```ts
import { computed, ref } from 'vue'
import { Copy, RefreshCw, GitFork, Pencil, EyeOff, Eye, ChevronLeft, ChevronRight, Check, X } from 'lucide-vue-next'
import type { Message } from '../api/types'

const props = withDefaults(defineProps<{
  message: Message
  style: 'bubble' | 'flat'
  isStreaming?: boolean
  siblingIndex?: number   // 0-based position among siblings
  siblingCount?: number
}>(), { siblingCount: 1, siblingIndex: 0 })

const emit = defineEmits<{
  copy: [text: string]
  regenerate: []
  fork: []
  'edit-save': [text: string]
  'toggle-hidden': []
  swipe: [delta: -1 | 1]
}>()

const isAssistant = computed(() => props.message.role === 'assistant')
const isUser = computed(() => props.message.role === 'user')
const label = computed(() => (isAssistant.value ? 'Assistant' : 'You'))
const hasSwipes = computed(() => isAssistant.value && (props.siblingCount ?? 1) > 1)

const editing = ref(false)
const draft = ref('')
function startEdit() { draft.value = props.message.raw_content; editing.value = true }
function saveEdit() { editing.value = false; emit('edit-save', draft.value) }
function cancelEdit() { editing.value = false }
```

Template — replace the hardcoded swipe indicator (in **both** bubble and flat blocks) with:

```html
        <span v-if="hasSwipes" data-test="swipe-indicator" class="flex items-center gap-1 text-[12px]">
          <button data-test="swipe-prev" class="hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) <= 0" @click="emit('swipe', -1)"><ChevronLeft :size="14" :stroke-width="2.2" /></button>
          <span>{{ (siblingIndex ?? 0) + 1 }}/{{ siblingCount }}</span>
          <button data-test="swipe-next" class="hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) >= (siblingCount ?? 1) - 1" @click="emit('swipe', 1)"><ChevronRight :size="14" :stroke-width="2.2" /></button>
        </span>
```

Add a **hide** button and route **edit** through inline editing in the action rows (both modes). Replace the edit button's `@click="emit('edit')"` with `@click="startEdit"`, and add a hide button:

```html
        <button data-test="hide-btn" class="hover:text-ink" :title="message.is_hidden ? 'Unhide' : 'Hide'" @click="emit('toggle-hidden')">
          <component :is="message.is_hidden ? Eye : EyeOff" :size="15" :stroke-width="1.8" />
        </button>
```

Make the message body switch to a textarea while editing (both modes); replace the `{{ message.raw_content }}` body with:

```html
        <template v-if="editing">
          <textarea data-test="edit-area" v-model="draft" rows="3" class="w-full bg-card border border-line rounded-[10px] px-3 py-2 text-[15px] outline-none focus:border-primary/50" />
          <div class="flex gap-2 mt-1.5">
            <button data-test="edit-save" class="text-primary hover:text-primary-strong" title="Save" @click="saveEdit"><Check :size="16" /></button>
            <button class="text-muted hover:text-ink" title="Cancel" @click="cancelEdit"><X :size="16" /></button>
          </div>
        </template>
        <template v-else>{{ message.raw_content }}<span v-if="isStreaming" ... /></template>
```

Show the action row (currently `v-if="isAssistant"`) for **user rows too**, but render only Copy + Edit + Hide for users (no swipe/regenerate/fork). Wrap the assistant-only buttons in `v-if="isAssistant"`.

- [ ] **Step 4: Run to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/MessageItem.test.ts`
Expected: PASS (existing + 2 new).

- [ ] **Step 5: Typecheck + commit**

Run: `cd shirita-ui && npx vue-tsc --noEmit` → clean.

```bash
git add shirita-ui/src/components/MessageItem.vue shirita-ui/src/components/MessageItem.test.ts
git commit -m "feat(ui): MessageItem real swipe + inline edit + hide"
```

---

## Task 6: Wire ChatView + MessageList to the active path and actions

**Files:**
- Modify: `shirita-ui/src/components/MessageList.vue`, `shirita-ui/src/views/ChatView.vue`

- [ ] **Step 1: Pass sibling info + emits through MessageList**

In `shirita-ui/src/components/MessageList.vue`: change the `messages` prop to the displayed path and compute sibling info per item with `siblings()`. Update emits to forward the new events.

Script:

```ts
import { computed } from 'vue'
import type { Message } from '../api/types'
import { siblings } from '../utils/tree'
import MessageItem from './MessageItem.vue'

const props = defineProps<{
  messages: Message[]        // the active path (displayed)
  allMessages: Message[]     // full set, for sibling counts
  style: 'bubble' | 'flat'
  isStreaming?: boolean
  streamingText?: string
  streamingError?: string | null
}>()

const emit = defineEmits<{
  copy: [text: string]
  regenerate: [id: string]
  fork: [id: string]
  'edit-save': [id: string, text: string]
  'toggle-hidden': [id: string]
  swipe: [id: string, delta: -1 | 1]
}>()

function sibInfo(msg: Message) {
  const sibs = siblings(props.allMessages, msg)
  return { index: sibs.findIndex((s) => s.id === msg.id), count: sibs.length, sibs }
}
// streamingMsg computed stays as-is
```

Template — render the path with per-item sibling info and forward events:

```html
    <MessageItem
      v-for="msg in messages"
      :key="msg.id"
      :message="msg"
      :style="style"
      :sibling-index="sibInfo(msg).index"
      :sibling-count="sibInfo(msg).count"
      @copy="emit('copy', $event)"
      @regenerate="emit('regenerate', msg.id)"
      @fork="emit('fork', msg.id)"
      @edit-save="(t) => emit('edit-save', msg.id, t)"
      @toggle-hidden="emit('toggle-hidden', msg.id)"
      @swipe="(d) => emit('swipe', msg.id, d)"
    />
```

- [ ] **Step 2: Wire ChatView**

In `shirita-ui/src/views/ChatView.vue`: feed the store's `displayed` as `messages`, `chat.messages` as `allMessages`, and implement handlers. Replace `handleRegenerate` and add the rest:

```ts
import { useRouter } from 'vue-router'
const router = useRouter()

function handleRegenerate(id: string) { chat.regenerate(sessionId, id) }
function handleEditSave(id: string, text: string) { chat.editMsg(id, text) }
function handleToggleHidden(id: string) { chat.toggleHidden(id) }
async function handleSwipe(id: string, delta: -1 | 1) {
  const sibs = siblings(chat.messages, chat.messages.find((m) => m.id === id)!)
  const i = sibs.findIndex((s) => s.id === id)
  const target = sibs[i + delta]
  if (target) await chat.switchLeaf(target.id)
}
async function handleFork(id: string) {
  const newId = await chat.fork(id)
  if (newId) router.push(`/chat/${newId}`)
}
```

Add `import { siblings } from '../utils/tree'`. Update the convo-token total to use `chat.displayed` (the active branch) instead of all messages. Update the template:

```html
    <MessageList
      v-else
      :messages="chat.displayed"
      :all-messages="chat.messages"
      :style="ui.messageStyle"
      :is-streaming="chat.isStreaming"
      :streaming-text="chat.streamingText"
      :streaming-error="chat.streamingError"
      @copy="handleCopy"
      @regenerate="handleRegenerate"
      @fork="handleFork"
      @edit-save="handleEditSave"
      @toggle-hidden="handleToggleHidden"
      @swipe="handleSwipe"
    />
```

- [ ] **Step 3: Run the full UI suite + typecheck**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npx vitest run`
Expected: PASS — fix any existing ChatView/MessageList test that passed the old `messages` prop (update it to pass `:messages`/`:all-messages` and the new events).

- [ ] **Step 4: Commit**

```bash
git add shirita-ui/src/components/MessageList.vue shirita-ui/src/views/ChatView.vue shirita-ui/src/views/ChatView.test.ts shirita-ui/src/components/MessageList.test.ts
git commit -m "feat(ui): chat renders the active branch + wires swipe/regenerate/edit/hide/fork"
```

---

## Task 7: Manual verification (browser)

- [ ] **Step 1:** Start the stack (`cargo run -p shirita-web` + `cd shirita-ui && npm run dev`), open a chat, send two turns.
- [ ] **Step 2:** Regenerate the last assistant → a `‹ 2/2 ›` indicator appears; arrows switch between the two replies; the transcript below follows the chosen branch.
- [ ] **Step 3:** Edit a user message in place → text changes, no new branch; regenerate → still branches the assistant.
- [ ] **Step 4:** Hide a message → it greys/drops from the branch; the next send's reply ignores it (verify by content).
- [ ] **Step 5:** Fork at an earlier assistant → navigates to a new chat containing only the history up to that point; original chat unchanged.
- [ ] **Step 6:** Reload the page → the previously active branch is restored (active_leaf persisted).

---

## Self-Review Checklist

- **Spec coverage (§4.5):** active-path rendering (T4/T6) ✓; real swipes (T5/T6) ✓; regenerate (T2/T4/T6) ✓; in-place edit (T5/T6) ✓; hide (T5/T6) ✓; fork + navigate (T2/T4/T6) ✓; persisted leaf on reload (T1/T4) ✓.
- **Placeholders:** none.
- **Type consistency:** `activePath(messages, activeLeafId)` / `siblings(messages, msg)` identical across T3/T4/T6; store actions (`switchLeaf`, `regenerate`, `editMsg`, `toggleHidden`, `fork`, `displayed`, `activeLeafId`) match between T4 def and T6 use; `MessageItem` props (`siblingIndex`, `siblingCount`) and emits (`swipe`, `edit-save`, `toggle-hidden`, `fork`, `regenerate`) match between T5 def and T6 wiring; client fns match T2 defs.
- **Open verification points:** the existing `MessageList.test.ts` / `ChatView.test.ts` (if present) must be updated to the new prop/emit shape (T6 step 3 covers this).
```
