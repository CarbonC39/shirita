# Pack/Preset Separation — Plan 6: Frontend Foundations (pack API + store + select=one radio + EntityPicker) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the reusable, isolated-testable frontend pieces the Book split and new-chat need: the pack API client + store, the `select=one` radio-style child control, and a reusable search-box picker — without touching the heavy BookView wiring yet.

**Architecture:** Four independent units. (1) `api/client` gains pack CRUD + session-pack mounting, and `createSession` learns `pack_ids` (node CRUD already works for `owner_kind=pack` via the existing `/templates/{id}/nodes` route — the handler keys off the query param, not the path). (2) `library` store gains `packs` + `loadPacks`. (3) `NodeRow`/`PromptTree` render the enable control of a `select=one` folder's children as a radio. (4) a new `EntityPicker.vue` provides the type-to-filter / pick / create-new search box, reused by the Book's Template and Pack sections in Plan 7.

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, Pinia, vue-i18n, lucide-vue-next, Vitest, `@vue/test-utils`.

## Global Constraints

- Plumbing must be **back-compatible**: `createSession`'s new `pack_ids` is an optional 4th parameter defaulting to `[]`; existing 3-arg callers keep working.
- Pack JSON: `Pack { id, name, identity: PackIdentity, meta, created_at, updated_at }`, `PackIdentity { display_name: string|null, avatar: string|null }` (already in `api/types.ts` from Plan 5). Backend `PackBody` is `{ name, identity?, meta? }` (name required).
- Endpoints exist (Plan 3): `GET/POST /api/packs`, `GET/PUT/DELETE /api/packs/{id}`, `POST /api/packs/{id}/duplicate`, `PUT /api/sessions/{id}/packs` (`{ pack_ids }`), and `POST /api/sessions` accepts `pack_ids`.
- No new i18n keys in this plan (`EntityPicker` receives display strings as props; Book labels land in Plan 7). Comments/commits in English.
- Test commands run without `cd`: `npm --prefix shirita-ui test -- <pattern>`; typecheck/build: `npm --prefix shirita-ui run build`.
- The test fetch token is `test-token` (see existing `client.test.ts`).

---

## File Structure

- `shirita-ui/src/api/client.ts` — add pack CRUD + `setSessionPacks`; extend `createSession`. (Task 1)
- `shirita-ui/src/api/client.test.ts` — pack client tests. (Task 1)
- `shirita-ui/src/stores/library.ts` — `packs` + `loadPacks`. (Task 2)
- `shirita-ui/src/stores/library.test.ts` — new, store test. (Task 2)
- `shirita-ui/src/components/NodeRow.vue` + `.test.ts` — radio enable control under `single-select`. (Task 3)
- `shirita-ui/src/components/PromptTree.vue` + `.test.ts` — pass `single-select` to `select=one` children. (Task 3)
- `shirita-ui/src/components/EntityPicker.vue` + `EntityPicker.test.ts` — new reusable search picker. (Task 4)

---

### Task 1: Pack API client + `createSession` pack_ids

**Files:**
- Modify: `shirita-ui/src/api/client.ts:1-13` (type import), `:188-196` (`createSession`), and append a `// --- Packs ---` block
- Test: `shirita-ui/src/api/client.test.ts`

**Interfaces:**
- Consumes: existing `apiGet`, `authHeaders`, `BASE`; types `Pack`, `PackIdentity` from `./types`.
- Produces: `listPacks(): Promise<Pack[]>`, `getPack(id): Promise<Pack>`, `createPack(body: { name: string; identity?: PackIdentity; meta?: Record<string, unknown> }): Promise<Pack>`, `updatePack(id, body: { name: string; identity?: PackIdentity; meta?: Record<string, unknown> }): Promise<Pack>`, `deletePack(id): Promise<void>`, `duplicatePack(id): Promise<Pack>`, `setSessionPacks(sessionId, packIds: string[]): Promise<void>`, and `createSession(name, templateId?, avatar?, packIds: string[] = [])`.

- [ ] **Step 1: Write the failing tests**

Append to `shirita-ui/src/api/client.test.ts` (before the final closing — add a new `describe`):

```ts
import { listPacks, createPack, setSessionPacks, createSession } from './client'

describe('packs client', () => {
  beforeEach(() => { vi.restoreAllMocks() })

  it('listPacks GETs /api/packs', async () => {
    const data = [{ id: 'p1', name: 'Alice', identity: { display_name: null, avatar: null }, meta: {}, created_at: '', updated_at: '' }]
    const fm = mockFetch(200, data)
    vi.stubGlobal('fetch', fm)
    await expect(listPacks()).resolves.toEqual(data)
    expect(fm).toHaveBeenCalledWith('/api/packs', { headers: { Authorization: 'Bearer test-token' } })
  })

  it('createPack POSTs name + identity', async () => {
    const fm = mockFetch(200, { id: 'p1' })
    vi.stubGlobal('fetch', fm)
    await createPack({ name: 'Alice', identity: { display_name: 'Alice', avatar: 'a.png' } })
    const [url, opts] = fm.mock.calls[0]
    expect(url).toContain('/api/packs')
    expect(opts.method).toBe('POST')
    expect(JSON.parse(opts.body)).toEqual({ name: 'Alice', identity: { display_name: 'Alice', avatar: 'a.png' } })
  })

  it('setSessionPacks PUTs the pack id list', async () => {
    const fm = mockFetch(200, {})
    vi.stubGlobal('fetch', fm)
    await setSessionPacks('s1', ['p1', 'p2'])
    const [url, opts] = fm.mock.calls[0]
    expect(url).toContain('/api/sessions/s1/packs')
    expect(opts.method).toBe('PUT')
    expect(JSON.parse(opts.body)).toEqual({ pack_ids: ['p1', 'p2'] })
  })

  it('createSession includes pack_ids in the body', async () => {
    const fm = mockFetch(200, { id: 's1' })
    vi.stubGlobal('fetch', fm)
    await createSession('Chat', 't1', null, ['p1'])
    const body = JSON.parse(fm.mock.calls[0][1].body)
    expect(body.pack_ids).toEqual(['p1'])
    expect(body.template_id).toBe('t1')
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm --prefix shirita-ui test -- client`
Expected: FAIL — `listPacks` / `createPack` / `setSessionPacks` are not exported; `createSession` 4th arg unused so `pack_ids` is absent.

- [ ] **Step 3: Extend the type import**

In `shirita-ui/src/api/client.ts`, add `Pack` and `PackIdentity` to the import from `./types` (the block at lines 1-13). It becomes:

```ts
import type {
  Definition,
  DefType,
  Identity,
  ImportSummary,
  Message,
  OnConflict,
  Pack,
  PackIdentity,
  PromptNode,
  Session,
  SessionState,
  Template,
  VarDecl,
} from './types'
```

- [ ] **Step 4: Extend `createSession` with `pack_ids`**

Replace the `createSession` function (lines 188-196) with:

```ts
// --- Sessions ---
export async function createSession(
  name: string,
  templateId?: string | null,
  avatar?: string | null,
  packIds: string[] = [],
): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, template_id: templateId || undefined, avatar: avatar || undefined, pack_ids: packIds }),
  })
  if (!res.ok) throw new Error(`Create session failed: ${res.status}`)
  return res.json()
}
```

- [ ] **Step 5: Append the Packs block**

Add at the end of `shirita-ui/src/api/client.ts`:

```ts
// --- Packs ---
export function listPacks(): Promise<Pack[]> { return apiGet<Pack[]>('/packs') }

export function getPack(id: string): Promise<Pack> { return apiGet<Pack>(`/packs/${id}`) }

export async function createPack(body: { name: string; identity?: PackIdentity; meta?: Record<string, unknown> }): Promise<Pack> {
  const res = await fetch(`${BASE}/api/packs`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Create pack failed: ${res.status}`)
  return res.json()
}

export async function updatePack(id: string, body: { name: string; identity?: PackIdentity; meta?: Record<string, unknown> }): Promise<Pack> {
  const res = await fetch(`${BASE}/api/packs/${id}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Update pack failed: ${res.status}`)
  return res.json()
}

export async function deletePack(id: string): Promise<void> {
  const res = await fetch(`${BASE}/api/packs/${id}`, { method: 'DELETE', headers: authHeaders() })
  if (!res.ok) throw new Error(`Delete pack failed: ${res.status}`)
}

export async function duplicatePack(id: string): Promise<Pack> {
  const res = await fetch(`${BASE}/api/packs/${id}/duplicate`, { method: 'POST', headers: authHeaders() })
  if (!res.ok) throw new Error(`Duplicate pack failed: ${res.status}`)
  return res.json()
}

export async function setSessionPacks(sessionId: string, packIds: string[]): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/packs`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ pack_ids: packIds }),
  })
  if (!res.ok) throw new Error(`Set session packs failed: ${res.status}`)
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `npm --prefix shirita-ui test -- client`
Expected: PASS — the 4 new pack tests plus all pre-existing client tests.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/api/client.ts shirita-ui/src/api/client.test.ts
git commit -m "feat(ui): pack API client + createSession pack_ids"
```

---

### Task 2: `library` store — packs + loadPacks

**Files:**
- Modify: `shirita-ui/src/stores/library.ts`
- Test: `shirita-ui/src/stores/library.test.ts` (new)

**Interfaces:**
- Consumes: `listPacks` from `../api/client` (Task 1); `Pack` type.
- Produces: `library.packs: Pack[]`, `library.loadPacks(): Promise<void>`; `loadAll()` now also loads packs.

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/stores/library.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

vi.mock('../api/client', () => ({
  listDefinitions: vi.fn().mockResolvedValue([]),
  listTemplates: vi.fn().mockResolvedValue([]),
  listTypes: vi.fn().mockResolvedValue([]),
  listPacks: vi.fn().mockResolvedValue([
    { id: 'p1', name: 'Alice', identity: { display_name: null, avatar: null }, meta: {}, created_at: '', updated_at: '' },
  ]),
  createType: vi.fn(),
  deleteType: vi.fn(),
}))

import { useLibraryStore } from './library'

describe('library store packs', () => {
  beforeEach(() => { setActivePinia(createPinia()) })

  it('loadPacks fills packs from the API', async () => {
    const lib = useLibraryStore()
    expect(lib.packs).toEqual([])
    await lib.loadPacks()
    expect(lib.packs.map((p) => p.id)).toEqual(['p1'])
  })

  it('loadAll also loads packs', async () => {
    const lib = useLibraryStore()
    await lib.loadAll()
    expect(lib.packs.length).toBe(1)
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm --prefix shirita-ui test -- library`
Expected: FAIL — `lib.packs` / `lib.loadPacks` do not exist.

- [ ] **Step 3: Add packs to the store**

In `shirita-ui/src/stores/library.ts`:

(a) Extend imports (lines 3-4):

```ts
import type { Definition, Template, DefType, Pack } from '../api/types'
import { listDefinitions, listTemplates, listTypes, listPacks, createType as apiCreateType, deleteType as apiDeleteType } from '../api/client'
```

(b) Add the ref after `containerTypes` (line 9):

```ts
  const packs = ref<Pack[]>([])
```

(c) Add `loadPacks` after `loadTypes` (after line 29):

```ts
  async function loadPacks() {
    try { packs.value = await listPacks() } catch (e) { error.value = (e as Error).message }
  }
```

(d) Include packs in `loadAll` (line 44) and the return (line 48):

```ts
  async function loadAll() {
    loading.value = true; error.value = null
    try { await Promise.all([loadDefinitions(), loadTemplates(), loadTypes(), loadPacks()]) } catch (e) { error.value = (e as Error).message }
    finally { loading.value = false }
  }

  return { definitions, templates, containerTypes, packs, loading, error, loadDefinitions, loadTemplates, loadTypes, loadPacks, addType, removeType, loadAll }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm --prefix shirita-ui test -- library`
Expected: PASS — both new store tests.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/stores/library.ts shirita-ui/src/stores/library.test.ts
git commit -m "feat(ui): library store packs + loadPacks"
```

---

### Task 3: `select=one` children render a radio enable control

**Files:**
- Modify: `shirita-ui/src/components/NodeRow.vue`
- Modify: `shirita-ui/src/components/PromptTree.vue`
- Test: `shirita-ui/src/components/NodeRow.test.ts`, `shirita-ui/src/components/PromptTree.test.ts`

**Interfaces:**
- Consumes: existing `NodeRow` props/emits.
- Produces: `NodeRow` gains an optional `singleSelect?: boolean` prop; when true and the node is a `ref`, its enable control renders as a radio with `data-test="enable-radio"` instead of `data-test="enable-checkbox"`. `PromptTree` passes `:single-select="(folder.meta.select === 'one')"` to the child rows it renders under a folder.

- [ ] **Step 1: Write the failing tests**

Add to `shirita-ui/src/components/NodeRow.test.ts` inside `describe('NodeRow', …)`:

```ts
  it('renders a radio enable control for a ref when single-select is set', () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 1, isExpanded: false, singleSelect: true } })
    expect(w.find('[data-test="enable-radio"]').exists()).toBe(true)
    expect(w.find('[data-test="enable-checkbox"]').exists()).toBe(false)
  })

  it('keeps the square checkbox when single-select is not set', () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 1, isExpanded: false } })
    expect(w.find('[data-test="enable-checkbox"]').exists()).toBe(true)
    expect(w.find('[data-test="enable-radio"]').exists()).toBe(false)
  })

  it('radio still emits toggleEnabled on click', async () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 1, isExpanded: false, singleSelect: true } })
    await w.find('[data-test="enable-radio"]').trigger('click')
    expect(w.emitted('toggleEnabled')).toBeTruthy()
  })
```

Add to `shirita-ui/src/components/PromptTree.test.ts` a new `describe`:

```ts
describe('PromptTree select=one children', () => {
  it('passes single-select to the children of a select=one folder', async () => {
    const nodes = [
      n({ id: 'f', kind: 'folder', tag: 'style', definition_id: null, meta: { select: 'one' } }),
      n({ id: 'c', kind: 'ref', parent_id: 'f', definition_id: 'c1' }),
    ]
    const w = mount(PromptTree, { props: { nodes, definitions: defs, types } })
    // expand the folder (its row is first; click its expand chevron)
    await w.find('[data-test="expand-btn"]').trigger('click')
    expect(w.find('[data-test="enable-radio"]').exists()).toBe(true)
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm --prefix shirita-ui test -- NodeRow PromptTree`
Expected: FAIL — `enable-radio` is never rendered (prop unknown / not wired).

- [ ] **Step 3: Add the `singleSelect` prop + radio control to NodeRow**

In `shirita-ui/src/components/NodeRow.vue`:

(a) Extend `defineProps` (lines 11-16):

```ts
const props = defineProps<{
  node: PromptNode
  definitions: Record<string, Definition>
  depth: number
  isExpanded: boolean
  singleSelect?: boolean
}>()
```

(b) Add a computed after `isContent` (added in Plan 5):

```ts
// A ref inside a select=one folder shows a radio instead of a checkbox.
const enableAsRadio = computed(() => props.singleSelect === true && props.node.kind === 'ref')
```

(c) Replace the existing enable checkbox button (the `data-test="enable-checkbox"` block) with a radio/checkbox branch:

```html
      <!-- enable control: radio for select=one children, else a rounded-square checkbox -->
      <button
        v-if="enableAsRadio"
        data-test="enable-radio"
        :aria-pressed="node.enabled"
        :class="['w-[18px] h-[18px] rounded-full grid place-items-center shrink-0 border transition-colors',
                 node.enabled ? 'border-primary' : 'border-[#d4d6da] bg-card']"
        @click="emit('toggleEnabled')"
      >
        <span v-if="node.enabled" class="w-[10px] h-[10px] rounded-full bg-primary" />
      </button>
      <button
        v-else
        data-test="enable-checkbox"
        :aria-pressed="node.enabled"
        :class="['w-[18px] h-[18px] rounded-[5px] grid place-items-center shrink-0 transition-colors',
                 node.enabled ? 'bg-primary' : 'bg-card border border-[#d4d6da]']"
        @click="emit('toggleEnabled')"
      >
        <Check v-if="node.enabled" :size="12" class="text-white" :stroke-width="3" />
      </button>
```

- [ ] **Step 4: Pass `single-select` from PromptTree to folder children**

In `shirita-ui/src/components/PromptTree.vue`, the child `NodeRow` (the one rendered inside `node.kind === 'folder'`, around lines 158-169) gains the prop. Add to that `<NodeRow :node="child" …>`:

```html
            :single-select="(node.meta as Record<string, unknown>).select === 'one'"
```

(Place it alongside the existing `:depth="1"` / `:is-expanded` props on the child NodeRow. The root NodeRow does not get the prop, so root rows keep the square checkbox.)

- [ ] **Step 5: Run the tests to verify they pass**

Run: `npm --prefix shirita-ui test -- NodeRow PromptTree`
Expected: PASS — the 3 new NodeRow tests, the new PromptTree test, and all pre-existing NodeRow/PromptTree tests (root rows and `all`-folder children still use `enable-checkbox`).

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/components/NodeRow.vue shirita-ui/src/components/NodeRow.test.ts shirita-ui/src/components/PromptTree.vue shirita-ui/src/components/PromptTree.test.ts
git commit -m "feat(ui): radio enable control for select=one folder children"
```

---

### Task 4: `EntityPicker.vue` — reusable search-box picker

**Files:**
- Create: `shirita-ui/src/components/EntityPicker.vue`
- Test: `shirita-ui/src/components/EntityPicker.test.ts` (new)

**Interfaces:**
- Consumes: nothing app-specific (i18n-agnostic — caller passes display strings).
- Produces: `<EntityPicker :items="{ id: string; name: string }[]" :placeholder="string" :create-label="string" @select="(id) => …" @create="(query) => …" />`. Filters `items` by case-insensitive name match; emits `select` with an item id, or `create` with the current trimmed query (always offered, even when empty — the caller supplies a fallback name).

- [ ] **Step 1: Write the failing tests**

Create `shirita-ui/src/components/EntityPicker.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import EntityPicker from './EntityPicker.vue'

const items = [
  { id: 't1', name: 'RP preset' },
  { id: 't2', name: 'Assistant' },
]

describe('EntityPicker', () => {
  it('lists items and emits select on click', async () => {
    const w = mount(EntityPicker, { props: { items, placeholder: 'pick…', createLabel: 'New' } })
    await w.find('[data-test="entity-search"]').trigger('focus')
    const rows = w.findAll('[data-test="entity-item"]')
    expect(rows.length).toBe(2)
    await rows[0].trigger('mousedown')
    expect(w.emitted('select')![0]).toEqual(['t1'])
  })

  it('filters by query (case-insensitive)', async () => {
    const w = mount(EntityPicker, { props: { items, placeholder: 'pick…', createLabel: 'New' } })
    await w.find('[data-test="entity-search"]').trigger('focus')
    await w.find('[data-test="entity-search"]').setValue('assist')
    const rows = w.findAll('[data-test="entity-item"]')
    expect(rows.length).toBe(1)
    expect(rows[0].text()).toContain('Assistant')
  })

  it('emits create with the trimmed query', async () => {
    const w = mount(EntityPicker, { props: { items, placeholder: 'pick…', createLabel: 'New' } })
    await w.find('[data-test="entity-search"]').trigger('focus')
    await w.find('[data-test="entity-search"]').setValue('  Villain ')
    await w.find('[data-test="entity-create"]').trigger('mousedown')
    expect(w.emitted('create')![0]).toEqual(['Villain'])
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm --prefix shirita-ui test -- EntityPicker`
Expected: FAIL — the component file does not exist (import error).

- [ ] **Step 3: Create the component**

Create `shirita-ui/src/components/EntityPicker.vue`:

```vue
<script setup lang="ts">
import { ref, computed } from 'vue'
import { Search, ChevronDown, Plus } from 'lucide-vue-next'

const props = withDefaults(
  defineProps<{ items: { id: string; name: string }[]; placeholder?: string; createLabel?: string }>(),
  { placeholder: '', createLabel: 'New' },
)
const emit = defineEmits<{ select: [id: string]; create: [name: string] }>()

const query = ref('')
const open = ref(false)

const matches = computed(() => {
  const q = query.value.trim().toLowerCase()
  const list = q ? props.items.filter((i) => i.name.toLowerCase().includes(q)) : props.items
  return list.slice(0, 8)
})

function pick(id: string) { emit('select', id); open.value = false }
function create() { emit('create', query.value.trim()); query.value = ''; open.value = false }
</script>

<template>
  <div class="relative" @focusout="open = false">
    <div class="flex items-center gap-2.5 border border-line rounded-[10px] bg-card px-3 py-2.5 focus-within:border-primary/50">
      <Search :size="16" class="text-muted shrink-0" />
      <input
        v-model="query"
        type="text"
        data-test="entity-search"
        :placeholder="placeholder"
        class="flex-1 bg-transparent outline-none text-[14px] text-ink placeholder:text-muted/60"
        @focus="open = true"
      />
      <button class="text-muted shrink-0" tabindex="-1" @mousedown.prevent="open = !open"><ChevronDown :size="16" /></button>
    </div>
    <transition name="expand">
      <div v-if="open" class="absolute left-0 right-0 top-full mt-1 bg-card border border-line rounded-[10px] shadow-lg overflow-hidden z-20">
        <button
          data-test="entity-create"
          class="w-full flex items-center gap-2 text-left px-3 py-2 text-[13.5px] text-primary hover:bg-surface"
          @mousedown.prevent="create"
        >
          <Plus :size="15" class="shrink-0" />
          <span>{{ createLabel }}<template v-if="query.trim()"> “{{ query.trim() }}”</template></span>
        </button>
        <button
          v-for="i in matches"
          :key="i.id"
          data-test="entity-item"
          class="w-full flex items-center gap-2 px-3 py-2 text-left text-[13.5px] hover:bg-surface border-t border-line"
          @mousedown.prevent="pick(i.id)"
        >
          <span class="flex-1 truncate text-ink">{{ i.name }}</span>
        </button>
      </div>
    </transition>
  </div>
</template>
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm --prefix shirita-ui test -- EntityPicker`
Expected: PASS — all 3 tests.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/EntityPicker.vue shirita-ui/src/components/EntityPicker.test.ts
git commit -m "feat(ui): reusable EntityPicker search box"
```

---

## Final Verification

- [ ] **Full UI test + typecheck sweep**

Run: `npm --prefix shirita-ui test 2>&1 | tail -8 && npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: all Vitest suites pass; build succeeds with no type errors.

---

## Self-Review

**Spec coverage (this plan = the foundations subset of spec §5 plumbing + §3.4 visual):**
- Pack API client (`listPacks/getPack/createPack/updatePack/deletePack/duplicatePack`, `setSessionPacks`) + `createSession` `pack_ids` — Task 1. (Node CRUD reuses the existing owner-agnostic `/templates/{id}/nodes` route — no new functions, per the client finding.)
- `library.packs` + `loadPacks` (+ `loadAll`) — Task 2.
- `select=one` children render a radio enable control (component level) — Task 3.
- Reusable search-box picker for Template/Pack sections — Task 4 (`EntityPicker`).
- Deferred to **Plan 7**: Book PACK section assembly (identity + pack tree + variables), Template-first reorder, swapping pickers to `EntityPicker`, section color-coding, `select=one` mutual-exclusion in Book handlers. Deferred to **Plan 8**: single-screen new-chat.

**Placeholder scan:** none — every step has full code and exact commands.

**Type consistency:** `Pack`/`PackIdentity` field names match `api/types.ts` (Plan 5) and the backend. `createSession(name, templateId?, avatar?, packIds=[])` signature is identical in client and test. `setSessionPacks(sessionId, packIds)` body `{ pack_ids }` matches the backend `SetPacks`. `EntityPicker` props/emits (`items`, `placeholder`, `createLabel`, `select`, `create`) and `data-test` hooks are identical between the component and its test. `singleSelect` prop + `enable-radio`/`enable-checkbox` data-tests are consistent between NodeRow, PromptTree, and both test files.
