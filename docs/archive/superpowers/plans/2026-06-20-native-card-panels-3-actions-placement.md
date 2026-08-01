# Native Card Panels — Plan 3: Actions + ChatView placement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire panel interactivity end-to-end: `<PanelView>` emits declarative `data-diff` / `data-insert` / `data-send` actions; `ChatView` renders a collapsible panel per mounted Pack that has a panel, feeds it the live variable values, and performs each action (gated by the pack's declared `caps`) — diff → `POST …/state-updates` + refresh, insert → composer, send → send.

**Architecture:** `PanelView` stays a pure renderer that *reports* user intent: one `click` listener on the shadow root maps an action element to an `action` event (interpolating `{{var}}` for insert/send). `ChatView` is the privileged host: it owns the composer and send flow, so it gates each action by `pack.meta.panel.caps` and performs it. A new `applyStateUpdates` client function calls the Plan-1 endpoint; the returned `values` flow straight back into `sessionState`, re-rendering the panel via morphdom (Plan 2).

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, Vitest + `@vue/test-utils` (jsdom shadow-DOM event dispatch), the Plan-1 `state-updates` endpoint, Plan-2 `PanelView`.

## Global Constraints

- **Host-gated capabilities**: `PanelView` emits every action; **`ChatView` enforces `caps`** at the privileged boundary (declared == granted in v1). `caps.write` → diff, `caps.insert` → insert, `caps.send` → send.
- Diffs go through the Plan-1 endpoint (typed, pack-scoped, hidden state-carrier node); the response `values` replace `sessionState.values` so the panel re-renders.
- `data-insert` / `data-send` attribute text is `{{var}}`-interpolated against the current values before emitting.
- Event handling is **one delegated listener on the shadow root** (survives morphdom re-renders).
- Single column preserved — the panel stack is inline at the top of the chat column, one collapsible per mounted pack, in mount order. (Cinema mode = point 3, not here.)
- Comments/commits in English. Tests: `npm --prefix shirita-ui test -- <pattern>`; build: `npm --prefix shirita-ui run build`.

---

## File Structure

- `shirita-ui/src/api/types.ts` — `PanelAction` union; `Session.mounted_packs?`. (Tasks 1 & 3)
- `shirita-ui/src/components/PanelView.vue` — `action` emit + shadow-root click delegation. (Task 1)
- `shirita-ui/src/components/PanelView.test.ts` — action-emit tests. (Task 1)
- `shirita-ui/src/api/client.ts` — `applyStateUpdates`. (Task 2)
- `shirita-ui/src/api/client.test.ts` — client test. (Task 2)
- `shirita-ui/src/views/ChatView.vue` — panel stack + action handler. (Task 3)
- `shirita-ui/src/views/ChatView.test.ts` — placement test. (Task 3)

---

### Task 1: `PanelView` action emission

**Files:**
- Modify: `shirita-ui/src/api/types.ts`
- Modify: `shirita-ui/src/components/PanelView.vue`
- Test: `shirita-ui/src/components/PanelView.test.ts`

**Interfaces:**
- Produces: `PanelAction` (in types.ts); `PanelView` now `defineEmits<{ action: [PanelAction] }>()`. (`ChatView` (Task 3) listens.)

- [ ] **Step 1: Add the `PanelAction` type**

In `shirita-ui/src/api/types.ts`, after the `Panel` interface (added in Plan 2), add:

```ts
/** A user interaction reported by a panel; the host decides whether to honor it. */
export type PanelAction =
  | { kind: 'diff'; key: string; op: string; value: string | null }
  | { kind: 'insert'; text: string }
  | { kind: 'send'; text: string }
```

- [ ] **Step 2: Write the failing tests**

Append to `shirita-ui/src/components/PanelView.test.ts` (the `shadowOf` helper already exists in this file from Plan 2):

```ts
describe('PanelView actions', () => {
  it('emits a diff action when a data-diff element is clicked', async () => {
    const w = mount(PanelView, {
      props: { html: '<button data-diff-key="hp" data-diff-op="sub" data-diff-value="1">-</button>', css: '', values: {} },
    })
    await nextTick()
    shadowOf(w).querySelector('button')!.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(w.emitted('action')!.at(-1)![0]).toEqual({ kind: 'diff', key: 'hp', op: 'sub', value: '1' })
  })

  it('emits an interpolated insert action', async () => {
    const w = mount(PanelView, {
      props: { html: '<button data-insert="Go to {{loc}}">go</button>', css: '', values: { loc: 'The Dark Forest' } },
    })
    await nextTick()
    shadowOf(w).querySelector('button')!.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(w.emitted('action')!.at(-1)![0]).toEqual({ kind: 'insert', text: 'Go to The Dark Forest' })
  })

  it('emits a send action', async () => {
    const w = mount(PanelView, { props: { html: '<button data-send="hi">x</button>', css: '', values: {} } })
    await nextTick()
    shadowOf(w).querySelector('button')!.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(w.emitted('action')!.at(-1)![0]).toEqual({ kind: 'send', text: 'hi' })
  })
})
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `npm --prefix shirita-ui test -- PanelView`
Expected: FAIL — `PanelView` emits no `action` yet (`w.emitted('action')` is undefined).

- [ ] **Step 4: Add the emit + delegated click handler to `PanelView.vue`**

In `shirita-ui/src/components/PanelView.vue`:

(a) Extend the imports and add the emit + a `PanelAction` import. Change the top of `<script setup>` so the vue import includes `onUnmounted` and add the type import:

```ts
import { ref, onMounted, onUnmounted, watch } from 'vue'
import morphdom from 'morphdom'
import { sanitizePanelHtml, fenceCss } from '../utils/panel'
import type { PanelAction } from '../api/types'
```

(b) After the `defineProps<…>()` call, add:

```ts
const emit = defineEmits<{ action: [PanelAction] }>()

function interpolate(text: string): string {
  return text.replace(/\{\{\s*(\w+)\s*\}\}/g, (_, k) => String(props.values[k] ?? ''))
}

function onClick(e: Event) {
  const target = e.target as HTMLElement | null
  const el = target?.closest?.('[data-diff-key],[data-insert],[data-send]') as HTMLElement | null
  if (!el) return
  if (el.hasAttribute('data-diff-key')) {
    emit('action', {
      kind: 'diff',
      key: el.getAttribute('data-diff-key') || '',
      op: el.getAttribute('data-diff-op') || 'set',
      value: el.getAttribute('data-diff-value'),
    })
  } else if (el.hasAttribute('data-insert')) {
    emit('action', { kind: 'insert', text: interpolate(el.getAttribute('data-insert') || '') })
  } else if (el.hasAttribute('data-send')) {
    emit('action', { kind: 'send', text: interpolate(el.getAttribute('data-send') || '') })
  }
}
```

(c) In `onMounted`, after `shadow.appendChild(styleEl)`, register the delegated listener:

```ts
  shadow.addEventListener('click', onClick)
```

(d) Add cleanup after the `onMounted` block:

```ts
onUnmounted(() => { shadow?.removeEventListener('click', onClick) })
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `npm --prefix shirita-ui test -- PanelView`
Expected: PASS — the three action-emit tests plus the five Plan-2 render/binding tests (8 total).

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/components/PanelView.vue shirita-ui/src/components/PanelView.test.ts
git commit -m "feat(ui): PanelView emits declarative diff/insert/send actions"
```

---

### Task 2: `applyStateUpdates` client function

**Files:**
- Modify: `shirita-ui/src/api/client.ts`
- Test: `shirita-ui/src/api/client.test.ts`

**Interfaces:**
- Consumes: the Plan-1 endpoint `POST /api/sessions/{id}/state-updates`.
- Produces: `applyStateUpdates(sessionId: string, updates: { action: string; key: string; value?: string | null }[]): Promise<{ values: Record<string, unknown> }>`. (`ChatView` (Task 3) calls it on a `diff` action.)

- [ ] **Step 1: Write the failing test**

In `shirita-ui/src/api/client.test.ts`, add `applyStateUpdates` to the existing `from './client'` import line, then add this test inside the `describe('api client', …)` block:

```ts
  it('applyStateUpdates POSTs /state-updates and returns the new values', async () => {
    const fm = mockFetch(200, { values: { hp: 90 } })
    vi.stubGlobal('fetch', fm)

    const out = await applyStateUpdates('s1', [{ action: 'sub', key: 'hp', value: '10' }])

    expect(fm).toHaveBeenCalledWith(
      expect.stringContaining('/api/sessions/s1/state-updates'),
      expect.objectContaining({ method: 'POST' }),
    )
    expect(JSON.parse((fm.mock.calls[0][1] as RequestInit).body as string))
      .toEqual({ updates: [{ action: 'sub', key: 'hp', value: '10' }] })
    expect(out).toEqual({ values: { hp: 90 } })
  })
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm --prefix shirita-ui test -- client`
Expected: FAIL — `applyStateUpdates` is not exported.

- [ ] **Step 3: Implement `applyStateUpdates`**

In `shirita-ui/src/api/client.ts`, add (next to the other `POST` helpers such as `setSessionPacks` / `createSession`, which use the same `BASE` + `authHeaders()` pattern):

```ts
export async function applyStateUpdates(
  sessionId: string,
  updates: { action: string; key: string; value?: string | null }[],
): Promise<{ values: Record<string, unknown> }> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/state-updates`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ updates }),
  })
  if (!res.ok) throw new Error(`State update failed: ${res.status}`)
  return res.json()
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm --prefix shirita-ui test -- client`
Expected: PASS — the new `applyStateUpdates` test and the existing client tests.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/api/client.ts shirita-ui/src/api/client.test.ts
git commit -m "feat(ui): applyStateUpdates client (POST /sessions/{id}/state-updates)"
```

---

### Task 3: `ChatView` panel stack + action wiring

**Files:**
- Modify: `shirita-ui/src/api/types.ts` (add `Session.mounted_packs?`)
- Modify: `shirita-ui/src/views/ChatView.vue`
- Test: `shirita-ui/src/views/ChatView.test.ts`

**Interfaces:**
- Consumes: `getSession`, `getPack`, `applyStateUpdates` (Task 2), `PanelView` (Plan 2 + Task 1), types `Pack` / `Panel` / `PanelAction`.
- Produces: a `data-test="panel-stack"` block rendering one collapsible `PanelView` per mounted pack that has `meta.panel`, in mount order; `onPanelAction(pack, action)` gating by `caps`.

- [ ] **Step 1: Add `mounted_packs` to the `Session` type (optional, so existing literals don't break)**

In `shirita-ui/src/api/types.ts`, inside the `Session` interface, after the `mounted_definitions: string[]` line, add:

```ts
  mounted_packs?: string[]
```

- [ ] **Step 2: Write the failing test**

In `shirita-ui/src/views/ChatView.test.ts`, add this test inside the `describe('ChatView', …)` block:

```ts
  it('renders a panel for each mounted pack that has a panel', async () => {
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's1', active_leaf_id: null, mounted_packs: ['p1'] } as never)
    vi.spyOn(client, 'getPack').mockResolvedValue({
      id: 'p1', name: 'Alice', identity: { display_name: null, avatar: null },
      meta: { panel: { html: '<span data-bind="hp">x</span>', css: '', caps: {} } },
      created_at: '', updated_at: '',
    } as never)
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const w = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(w.find('[data-test="panel-stack"]').exists()).toBe(true)
    expect(w.find('[data-test="panel-host"]').exists()).toBe(true)
  })
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `npm --prefix shirita-ui test -- ChatView`
Expected: FAIL — no `panel-stack` exists in `ChatView` yet.

- [ ] **Step 4: Add pack loading + the action handler to `ChatView.vue` script**

In `shirita-ui/src/views/ChatView.vue`:

(a) Extend the client import (currently `import { getSessionState, getSessionIdentity } from '../api/client'`):

```ts
import { getSessionState, getSessionIdentity, getSession, getPack, applyStateUpdates } from '../api/client'
```

(b) Extend the types import and add the `PanelView` component import (next to the other component imports):

```ts
import type { SessionState, Identity, Pack, Panel, PanelAction } from '../api/types'
import PanelView from '../components/PanelView.vue'
```

(c) Add the panel state + loader after `loadIdentity` (before `effectiveIdentity`):

```ts
// Mounted packs that ship a panel, in mount order.
const panelPacks = ref<Pack[]>([])
function panelOf(p: Pack): Panel {
  return (p.meta as { panel: Panel }).panel
}
async function loadPanels() {
  try {
    const session = await getSession(sessionId)
    const ids = session.mounted_packs ?? []
    const packs = await Promise.all(ids.map((pid) => getPack(pid)))
    panelPacks.value = packs.filter((p) => (p.meta as { panel?: Panel }).panel)
  } catch {
    panelPacks.value = []
  }
}

async function onPanelAction(pack: Pack, action: PanelAction) {
  const caps = panelOf(pack).caps || {}
  if (action.kind === 'diff') {
    if (!caps.write) return
    try {
      const res = await applyStateUpdates(sessionId, [{ action: action.op, key: action.key, value: action.value }])
      sessionState.value = { ...sessionState.value, values: res.values }
    } catch { /* surfaced via chat error elsewhere; panel stays on last good state */ }
  } else if (action.kind === 'insert') {
    if (caps.insert) composerRef.value?.setText(action.text)
  } else if (action.kind === 'send') {
    if (caps.send) await handleSend(action.text, [])
  }
}
```

(d) Add `loadPanels()` to the first `onMounted` (the one calling `chat.loadMessages` / `loadState` / `loadIdentity`):

```ts
onMounted(() => {
  chat.loadMessages(sessionId)
  loadState()
  loadIdentity()
  loadPanels()
})
```

- [ ] **Step 5: Add the panel stack to the `ChatView` template**

In `shirita-ui/src/views/ChatView.vue`, immediately after the header `<div>` (the one containing the back link + `headerName`, closing at the `</div>` before `<p v-if="chat.error" …>`), insert:

```html
    <div v-if="panelPacks.length" data-test="panel-stack" class="flex flex-col gap-2 py-2">
      <details v-for="p in panelPacks" :key="p.id" open class="rounded-xl border border-line bg-card/50 overflow-hidden">
        <summary class="cursor-pointer select-none px-3 py-2 text-[12px] font-semibold text-muted">{{ p.identity.display_name || p.name }}</summary>
        <div class="px-2 pb-2">
          <PanelView :html="panelOf(p).html" :css="panelOf(p).css" :values="sessionState.values" @action="onPanelAction(p, $event)" />
        </div>
      </details>
    </div>
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `npm --prefix shirita-ui test -- ChatView`
Expected: PASS — the panel-stack test plus the pre-existing ChatView tests (the default `getSession` mock returns no `mounted_packs`, so those render no stack and are unaffected).

- [ ] **Step 7: Typecheck/build**

Run: `npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: clean — `Pack`/`Panel`/`PanelAction` resolve, no unused symbols.

- [ ] **Step 8: Commit**

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/views/ChatView.vue shirita-ui/src/views/ChatView.test.ts
git commit -m "feat(ui): chat panel stack — render mounted-pack panels + handle actions"
```

---

## Final Verification

- [ ] **UI test + build sweep**

Run: `npm --prefix shirita-ui test 2>&1 | tail -8 && npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: all Vitest suites pass; build clean.

---

## Self-Review

**Spec coverage (spec §4 actions, §5 endpoint use, §6 placement):**
- `data-diff-key/op/value` → typed `Update` via the endpoint — Task 1 (emit) + Task 2 (client) + Task 3 (`onPanelAction` diff branch).
- `data-insert` (interpolated) → composer — Task 1 + Task 3 (`caps.insert` → `setText`).
- `data-send` (interpolated) → send — Task 1 + Task 3 (`caps.send` → `handleSend`).
- Capability gating (declared == granted in v1) at the privileged host — Task 3 (`onPanelAction` checks `caps`).
- One delegated listener on the shadow root (survives morph) — Task 1.
- Diff response `values` re-render the panel — Task 3 (`sessionState.value = { …, values: res.values }`, consumed by Plan-2 morphdom binding).
- Inline collapsible panel stack at the top of the chat column, per mounted pack with a panel, in mount order — Task 3.

**Placeholder scan:** none — full emit/handler code, complete client function, exact template block, complete test code, exact commands.

**Type consistency:** `PanelAction` union is defined once (Task 1) and consumed by `PanelView`'s emit and `ChatView`'s `onPanelAction`. `applyStateUpdates(sessionId, updates)` signature matches its call in `onPanelAction` (`{ action: op, key, value }`) and the Plan-1 body shape `{ updates: [{action,key,value}] }`. `panelOf(p).caps` reads `PanelCaps` (Plan 2). `Session.mounted_packs?: string[]` matches `loadPanels`' `session.mounted_packs ?? []`. `PanelView` props `{ html, css, values }` + `@action` match Plan 2 + Task 1.
