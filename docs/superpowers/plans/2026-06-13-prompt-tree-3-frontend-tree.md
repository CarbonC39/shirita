# Prompt Tree v2 — Plan 3: Frontend node tree

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the template node-tree UI to match the v2 model: placement-aware add flows (root → Add prompt / Add container; container → typed picker), a non-deletable history row, per-node delete, native drag-reorder, and container types loaded dynamically from `/api/types`.

**Architecture:** `api/types.ts` gains the `history` node kind + a `DefType`. `client.ts` gains `listTypes`/`createType`/`reorderNodes`. The `library` store caches container types. `NodePicker` lists types from the store (no hard-coded list). `NodeRow` renders palette-tinted icons, a hover-delete, and a special history row. `PromptTree` owns the add flows, drag-reorder, and delete-confirm, emitting intent up to `BookView`, which performs the API calls. All enforcement of placement rules lives in the add flows (root accepts only prompt-refs + containers + history; a type-`T` container accepts only type-`T` refs; one container per type).

**Tech Stack:** Vue 3 `<script setup>` + TypeScript, Pinia, lucide-vue-next, Tailwind v4 tokens, Vitest + `@vue/test-utils` (jsdom). Native HTML5 drag-and-drop (no new dependency). **Never use `v-html`.**

**Spec:** `docs/superpowers/specs/2026-06-13-prompt-tree-worldbook-design.md` §4 (placement), §9 (tree v2), §13 (frontend structure). Depends on **Plan 2** (`/api/types`, history node auto-created by Plan 1).

**Out of scope:** Trigger editor inside expanded refs → Plan 4. Palette/subheading polish beyond node icons → Plan 5. Backend placement validation (frontend enforces; a server-side guard is a later hardening). In-chat (session-owned) tree editing → deferred.

---

## File structure

- `shirita-ui/src/api/types.ts` — `PromptNode.kind` adds `'history'`; add `DefType` interface.
- `shirita-ui/src/api/client.ts` — add `listTypes`, `createType`, `deleteType`, `reorderNodes`.
- `shirita-ui/src/stores/library.ts` — add `containerTypes` ref + `loadTypes` + `createType`.
- `shirita-ui/src/components/NodePicker.vue` — types from store; New type… inline.
- `shirita-ui/src/components/NodeRow.vue` — palette icons, hover-delete, history variant, drag handle.
- `shirita-ui/src/components/PromptTree.vue` — rebuilt add flows, drag-reorder, delete.
- `shirita-ui/src/views/BookView.vue` — wire add-container/add-prompt/reorder/delete + `loadTypes`.
- Tests: `NodePicker.test.ts`, `NodeRow.test.ts`, `PromptTree.test.ts` (new).

Run the frontend test suite with: `cd shirita-ui && npx vitest run <file>`.

---

## Task 1: Types + client + store plumbing

**Files:**
- Modify: `shirita-ui/src/api/types.ts`, `shirita-ui/src/api/client.ts`, `shirita-ui/src/stores/library.ts`
- Test: `shirita-ui/src/api/client.test.ts` (extend)

- [ ] **Step 1: Extend types.** In `shirita-ui/src/api/types.ts`:
  - Change `kind: 'folder' | 'ref'` → `kind: 'folder' | 'ref' | 'history'`.
  - Append:

```ts
export interface DefType {
  id: string
  label: string
  sort: number
  builtin: boolean
  created_at: string
}
```

- [ ] **Step 2: Write the failing client test.** In `shirita-ui/src/api/client.test.ts`, add (mirror the existing `fetch`-mock pattern in that file):

```ts
import { listTypes, reorderNodes } from './client'

describe('types + reorder client', () => {
  it('listTypes GETs /api/types', async () => {
    const data = [{ id: 'char', label: 'Character', sort: 0, builtin: true, created_at: '' }]
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => data })
    vi.stubGlobal('fetch', fetchMock)
    const out = await listTypes()
    expect(out).toEqual(data)
    expect(fetchMock.mock.calls[0][0]).toContain('/api/types')
  })

  it('reorderNodes PUTs ordered ids', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({}) })
    vi.stubGlobal('fetch', fetchMock)
    await reorderNodes('template', 'tpl1', ['a', 'b'])
    const [url, opts] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/templates/tpl1/nodes/reorder?owner_kind=template')
    expect(opts.method).toBe('PUT')
    expect(JSON.parse(opts.body)).toEqual({ ordered_ids: ['a', 'b'] })
  })
})
```

- [ ] **Step 3: Run it, verify it fails**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts`
Expected: FAIL (`listTypes`/`reorderNodes` not exported).

- [ ] **Step 4: Implement the client functions.** In `shirita-ui/src/api/client.ts` add (after the Settings block), and `import type { ..., DefType }` at the top:

```ts
// --- Types (container type registry) ---
export function listTypes(): Promise<DefType[]> { return apiGet<DefType[]>('/types') }

export async function createType(body: { id: string; label: string; sort?: number }): Promise<DefType> {
  const res = await fetch(`${BASE}/api/types`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Create type failed: ${res.status}`)
  return res.json()
}

export async function deleteType(id: string): Promise<void> {
  const res = await fetch(`${BASE}/api/types/${id}`, { method: 'DELETE', headers: authHeaders() })
  if (!res.ok) throw new Error(`Delete type failed: ${res.status}`)
}

export async function reorderNodes(ownerKind: string, ownerId: string, orderedIds: string[]): Promise<void> {
  const res = await fetch(`${BASE}/api/templates/${ownerId}/nodes/reorder?owner_kind=${ownerKind}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ ordered_ids: orderedIds }),
  })
  if (!res.ok) throw new Error(`Reorder nodes failed: ${res.status}`)
}
```

- [ ] **Step 5: Add the store cache.** In `shirita-ui/src/stores/library.ts`:
  - Import: `import { listDefinitions, listTemplates, listTypes, createType as apiCreateType } from '../api/client'` and `import type { Definition, Template, DefType } from '../api/types'`.
  - Add `const containerTypes = ref<DefType[]>([])`.
  - Add:

```ts
  async function loadTypes() {
    try { containerTypes.value = await listTypes() } catch (e) { error.value = (e as Error).message }
  }
  async function addType(id: string, label: string) {
    const created = await apiCreateType({ id, label, sort: containerTypes.value.length })
    containerTypes.value = [...containerTypes.value, created]
    return created
  }
```
  - Add `containerTypes`, `loadTypes`, `addType` to the returned object, and include `loadTypes()` in `loadAll`'s `Promise.all([...])`.

- [ ] **Step 6: Run tests, verify pass**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/api/client.ts shirita-ui/src/stores/library.ts shirita-ui/src/api/client.test.ts
git commit -m "feat(ui): history node kind, types/reorder client, container-types store cache"
```

---

## Task 2: NodePicker — dynamic container types

The picker must list container types from the store and offer "New type…", instead of the hard-coded `typeOptions`.

**Files:**
- Modify: `shirita-ui/src/components/NodePicker.vue`
- Test: `shirita-ui/src/components/NodePicker.test.ts` (new)

- [ ] **Step 1: Write the failing test.** Create `shirita-ui/src/components/NodePicker.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import NodePicker from './NodePicker.vue'
import type { Definition, DefType } from '../api/types'

const defs: Definition[] = [
  { id: 'd1', type: 'char', name: 'Neo', content: '', meta: {} },
  { id: 'd2', type: 'world', name: 'Zion', content: '', meta: {} },
]
const types: DefType[] = [
  { id: 'char', label: 'Character', sort: 0, builtin: true, created_at: '' },
  { id: 'world', label: 'World', sort: 1, builtin: true, created_at: '' },
]

describe('NodePicker', () => {
  it('filters definitions to the picker type', () => {
    const w = mount(NodePicker, { props: { definitions: defs, filterType: 'char', types } })
    expect(w.text()).toContain('Neo')
    expect(w.text()).not.toContain('Zion')
  })

  it('emits select with the definition id', async () => {
    const w = mount(NodePicker, { props: { definitions: defs, filterType: 'char', types } })
    await w.findAll('button').find((b) => b.text().includes('Neo'))!.trigger('click')
    expect(w.emitted('select')![0]).toEqual(['d1'])
  })
})
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/NodePicker.test.ts`
Expected: FAIL (NodePicker doesn't accept a `types` prop).

- [ ] **Step 3: Implement.** In `shirita-ui/src/components/NodePicker.vue`:
  - Add `types: DefType[]` to props; import `DefType` from `../api/types`.
  - Delete the hard-coded `const typeOptions = [...]`; replace its use with `props.types.map(t => t.id)`.
  - Keep `typeTint` but make it fall back gracefully for unknown ids (already does via `|| 'bg-muted/30'`). Add `prompt: 'bg-muted/30'` (already present).
  - In the "other type" chip list, iterate `props.types`:

```html
      <button
        v-for="t in types"
        :key="t.id"
        :class="['px-2.5 py-1 text-[12px] rounded-full border transition-colors',
                 activeType === t.id ? 'bg-primary/10 text-primary border-primary/30' : 'text-muted border-line hover:text-ink']"
        @click="activeType = t.id; showTypes = false"
      >{{ t.label }}</button>
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cd shirita-ui && npx vitest run src/components/NodePicker.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/NodePicker.vue shirita-ui/src/components/NodePicker.test.ts
git commit -m "feat(ui): NodePicker lists container types from the registry"
```

---

## Task 3: NodeRow v2 — palette icons, delete, history row, drag handle

**Files:**
- Modify: `shirita-ui/src/components/NodeRow.vue`
- Test: `shirita-ui/src/components/NodeRow.test.ts` (new)

- [ ] **Step 1: Write the failing test.** Create `shirita-ui/src/components/NodeRow.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import NodeRow from './NodeRow.vue'
import type { Definition, PromptNode } from '../api/types'

const defs: Record<string, Definition> = {
  d1: { id: 'd1', type: 'char', name: 'Neo', content: 'body', meta: {} },
}
function node(p: Partial<PromptNode>): PromptNode {
  return { id: 'n1', owner_kind: 'template', owner_id: 't', parent_id: null, sort_order: 0,
    kind: 'ref', tag: null, definition_id: 'd1', enabled: true, created_at: '', ...p }
}

describe('NodeRow', () => {
  it('emits delete when the delete button is clicked', async () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 0, isExpanded: false } })
    await w.find('[data-test="node-delete"]').trigger('click')
    expect(w.emitted('delete')).toBeTruthy()
  })

  it('history row shows the Chat history label and no delete button', () => {
    const h = node({ kind: 'history', definition_id: null, tag: null })
    const w = mount(NodeRow, { props: { node: h, definitions: defs, depth: 0, isExpanded: false } })
    expect(w.text()).toContain('Chat history')
    expect(w.find('[data-test="node-delete"]').exists()).toBe(false)
  })
})
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/NodeRow.test.ts`
Expected: FAIL (no delete button / no history label).

- [ ] **Step 3: Implement.** In `shirita-ui/src/components/NodeRow.vue`:
  - Add `delete` to emits: `const emit = defineEmits<{ toggleEnabled: []; toggleExpand: []; updateContent: [content: string]; delete: [] }>()`.
  - Add computeds:

```ts
const isHistory = computed(() => props.node.kind === 'history')
const label = computed(() => {
  if (isHistory.value) return 'Chat history'
  if (isFolder.value) return props.node.tag || '(folder)'
  return def.value ? def.value.name : '(missing)'
})
// palette tint per definition/container type
const typeTint: Record<string, string> = {
  char: 'text-sky', persona: 'text-coral', world: 'text-mauve', prompt: 'text-muted',
}
const iconColor = computed(() => {
  const t = isFolder.value ? (props.node.tag ?? '') : (def.value?.type ?? '')
  return props.node.enabled ? (typeTint[t] ?? 'text-muted') : 'text-muted/40'
})
```
  - In the template row, add a history icon branch and a delete button. Replace the icon block with:

```html
      <History v-if="isHistory" :size="16" class="text-primary shrink-0" :stroke-width="1.8" />
      <Folder v-else-if="isFolder" :size="17" :class="iconColor" class="shrink-0" :stroke-width="1.8" />
      <FileText v-else :size="16" :class="iconColor" class="shrink-0" :stroke-width="1.8" />
```
  and before the expand chevron, add (history rows render no delete):

```html
      <button
        v-if="!isHistory"
        data-test="node-delete"
        class="text-muted/0 group-hover:text-muted/70 hover:!text-coral shrink-0 p-0.5 transition-colors"
        title="Delete"
        @click.stop="emit('delete')"
      ><Trash2 :size="15" /></button>
```
  - The expand chevron and inline editor stay, but guard the inline editor with `v-if="!isFolder && !isHistory && isExpanded"` (history rows have no editor).
  - Update the lucide import: `import { ChevronRight, Folder, FileText, History, Check, Maximize2, Trash2 } from 'lucide-vue-next'`.

- [ ] **Step 4: Run tests, verify pass**

Run: `cd shirita-ui && npx vitest run src/components/NodeRow.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/NodeRow.vue shirita-ui/src/components/NodeRow.test.ts
git commit -m "feat(ui): NodeRow v2 — palette icons, hover-delete, history row"
```

---

## Task 4: PromptTree — placement-aware add flows

Replace the single ambiguous "Add node" with: root "+" → **Add prompt** (prompt-ref picker) and **Add container** (type list, hiding types that already have a container); each container "+" → that type's ref picker.

**Files:**
- Modify: `shirita-ui/src/components/PromptTree.vue`
- Test: `shirita-ui/src/components/PromptTree.test.ts` (new)

- [ ] **Step 1: Write the failing test.** Create `shirita-ui/src/components/PromptTree.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import PromptTree from './PromptTree.vue'
import type { Definition, DefType, PromptNode } from '../api/types'

const types: DefType[] = [
  { id: 'char', label: 'Character', sort: 0, builtin: true, created_at: '' },
  { id: 'world', label: 'World', sort: 1, builtin: true, created_at: '' },
]
const defs: Definition[] = [
  { id: 'p1', type: 'prompt', name: 'Main', content: '', meta: {} },
  { id: 'c1', type: 'char', name: 'Neo', content: '', meta: {} },
]
function n(p: Partial<PromptNode>): PromptNode {
  return { id: 'x', owner_kind: 'template', owner_id: 't', parent_id: null, sort_order: 0,
    kind: 'ref', tag: null, definition_id: null, enabled: true, created_at: '', ...p }
}

describe('PromptTree add flows', () => {
  it('Add container lists only types without an existing container', async () => {
    const nodes = [n({ id: 'f-char', kind: 'folder', tag: 'char', definition_id: null })]
    const w = mount(PromptTree, { props: { nodes, definitions: defs, types } })
    await w.find('[data-test="root-add"]').trigger('click')
    await w.find('[data-test="add-container"]').trigger('click')
    // char already has a container → only world offered
    const labels = w.findAll('[data-test="container-type-option"]').map((b) => b.text())
    expect(labels).toEqual(['World'])
  })

  it('Add container emits addContainer with the chosen type', async () => {
    const w = mount(PromptTree, { props: { nodes: [], definitions: defs, types } })
    await w.find('[data-test="root-add"]').trigger('click')
    await w.find('[data-test="add-container"]').trigger('click')
    await w.findAll('[data-test="container-type-option"]')[0].trigger('click')
    expect(w.emitted('addContainer')![0]).toEqual(['char'])
  })
})
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/PromptTree.test.ts`
Expected: FAIL (no `root-add`/`add-container` hooks; no `addContainer` emit).

- [ ] **Step 3: Implement.** Rewrite `shirita-ui/src/components/PromptTree.vue`:
  - Props: `defineProps<{ nodes: PromptNode[]; definitions: Definition[]; types: DefType[] }>()`.
  - Emits:

```ts
const emit = defineEmits<{
  toggleEnabled: [nodeId: string]
  addPrompt: [definitionId: string]
  addRefToContainer: [parentId: string, definitionId: string]
  addContainer: [typeId: string]
  createNewInContainer: [parentId: string, typeId: string]
  createNewPrompt: []
  updateContent: [definitionId: string, content: string]
  deleteNode: [nodeId: string]
  reorder: [orderedIds: string[]]
}>()
```
  - Root menu state: `const rootMenu = ref<'closed' | 'menu' | 'addPrompt' | 'addContainer'>('closed')`.
  - Helpers:

```ts
const existingContainerTags = computed(() =>
  new Set(props.nodes.filter((nd) => nd.kind === 'folder' && nd.parent_id === null).map((nd) => nd.tag)))
const availableTypes = computed(() => props.types.filter((t) => !existingContainerTags.value.has(t.id)))
const promptDefs = computed(() => props.definitions.filter((d) => d.type === 'prompt'))
function containerDefs(tag: string | null) { return props.definitions.filter((d) => d.type === tag) }
```
  - Root markup (replace the old "Add node"): a `[data-test="root-add"]` "+" button toggling `rootMenu='menu'`; when `menu`, show two buttons `[data-test="add-prompt"]` and `[data-test="add-container"]`; when `addPrompt`, render `<NodePicker :definitions="promptDefs" :filter-type="'prompt'" :types="types" @select="(id)=>{emit('addPrompt',id);rootMenu='closed'}" @create-new="()=>{emit('createNewPrompt');rootMenu='closed'}" />`; when `addContainer`, render the available-type list:

```html
      <div v-if="rootMenu === 'addContainer'" class="px-2 pb-2 flex flex-wrap gap-1.5">
        <button
          v-for="t in availableTypes"
          :key="t.id"
          data-test="container-type-option"
          class="px-2.5 py-1 text-[12px] rounded-full border border-line text-muted hover:text-primary hover:border-primary/30"
          @click="emit('addContainer', t.id); rootMenu = 'closed'"
        >{{ t.label }}</button>
        <span v-if="availableTypes.length === 0" class="text-[12px] text-muted/70">All container types added</span>
      </div>
```
  - Container "+" (inside the `v-if="node.kind === 'folder' && isExpanded"` block): keep the per-container picker but pass typed defs + `types`, and route its events to the container emits:

```html
        <div v-if="activePickerParent === node.id" class="pl-[34px] pr-2 pb-2">
          <NodePicker
            :definitions="containerDefs(node.tag)"
            :filter-type="node.tag"
            :types="types"
            @select="(id) => { emit('addRefToContainer', node.id, id); activePickerParent = undefined }"
            @create-new="() => { emit('createNewInContainer', node.id, node.tag as string); activePickerParent = undefined }"
          />
        </div>
```
  - Wire `NodeRow`'s `@delete` to `emit('deleteNode', node.id)` for every row.
  - Keep `getChildren`, `rootNodes`, `isExpanded`, `toggleExpand`, `openPicker` (container picker toggle).

- [ ] **Step 4: Run tests, verify pass**

Run: `cd shirita-ui && npx vitest run src/components/PromptTree.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/PromptTree.vue shirita-ui/src/components/PromptTree.test.ts
git commit -m "feat(ui): PromptTree placement-aware add flows (prompt / container / typed refs)"
```

---

## Task 5: PromptTree — native drag-reorder

Same-level reorder via HTML5 DnD; emit the new ordered id list for the affected level.

**Files:**
- Modify: `shirita-ui/src/components/PromptTree.vue`
- Test: `shirita-ui/src/components/PromptTree.test.ts` (extend)

- [ ] **Step 1: Write the failing test.** Add to `PromptTree.test.ts`:

```ts
describe('PromptTree drag reorder', () => {
  it('emits reorder with the new root order on drop', async () => {
    const nodes = [
      n({ id: 'a', kind: 'folder', tag: 'char', definition_id: null, sort_order: 0 }),
      n({ id: 'b', kind: 'folder', tag: 'world', definition_id: null, sort_order: 1 }),
    ]
    const w = mount(PromptTree, { props: { nodes, definitions: defs, types } })
    const rows = w.findAll('[data-test="row-wrap"]')
    await rows[0].trigger('dragstart')
    await rows[1].trigger('drop')
    expect(w.emitted('reorder')![0]).toEqual([['b', 'a']])
  })
})
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/PromptTree.test.ts`
Expected: FAIL (no `row-wrap` drag hooks / no `reorder` emit).

- [ ] **Step 3: Implement.** In `PromptTree.vue`:
  - Add drag state + handlers (reorder only among siblings of the same `parent_id`):

```ts
const dragId = ref<string | null>(null)
function onDragStart(id: string) { dragId.value = id }
function siblingsOf(parentId: string | null) { return getChildren(parentId).map((nd) => nd.id) }
function parentOf(id: string): string | null {
  return props.nodes.find((nd) => nd.id === id)?.parent_id ?? null
}
function onDrop(targetId: string) {
  const src = dragId.value
  dragId.value = null
  if (!src || src === targetId) return
  if (parentOf(src) !== parentOf(targetId)) return // only reorder within a level
  const ids = siblingsOf(parentOf(targetId))
  const from = ids.indexOf(src)
  const to = ids.indexOf(targetId)
  if (from === -1 || to === -1) return
  ids.splice(to, 0, ids.splice(from, 1)[0])
  emit('reorder', ids)
}
```
  - Wrap each rendered row (root rows and child rows) in a draggable element carrying the hooks. For root nodes:

```html
    <div
      v-for="node in rootNodes"
      :key="node.id"
      data-test="row-wrap"
      draggable="true"
      @dragstart="onDragStart(node.id)"
      @dragover.prevent
      @drop="onDrop(node.id)"
    >
      <NodeRow ... @delete="emit('deleteNode', node.id)" />
      <!-- folder children block unchanged, but each child row also wrapped: -->
    </div>
```
  For child rows inside a container, wrap the `<NodeRow v-for="child …">` similarly with `data-test="row-wrap"`, `draggable="true"`, `@dragstart="onDragStart(child.id)"`, `@dragover.prevent`, `@drop="onDrop(child.id)"`.

  > Note: the history row is draggable like any root row (spec: history is movable). Its reorder emits the same root ordered-id list — `BookView` persists it via `reorderNodes`.

- [ ] **Step 4: Run tests, verify pass**

Run: `cd shirita-ui && npx vitest run src/components/PromptTree.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/PromptTree.vue
git commit -m "feat(ui): PromptTree native drag-reorder within a level"
```

---

## Task 6: BookView wiring + delete confirm + load types

Wire the new emits to API calls, load container types, and confirm before deleting a non-empty container.

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue`

- [ ] **Step 1: Load types on mount.** In `BookView.vue` `onMounted`, change the `Promise.all` to include types:

```ts
  try { await Promise.all([library.loadTemplates(), library.loadDefinitions(), library.loadTypes()]) }
```

- [ ] **Step 2: Replace the tree wiring.** Update the `<PromptTree>` usage and handlers. Import the new client fns:

```ts
import { listNodes, createNode, updateNode, deleteNode, reorderNodes, updateDefinition, createDefinition, deleteDefinition, createTemplate, duplicateTemplate, deleteTemplate } from '../api/client'
```
  Replace `handleAddNode`/`handleCreateNew` with the placement-aware handlers:

```ts
async function reload() {
  if (selectedTemplateId.value) nodes.value = await listNodes('template', selectedTemplateId.value)
}
async function addPrompt(definitionId: string) {
  if (!selectedTemplateId.value) return
  await createNode('template', selectedTemplateId.value, { parent_id: null, kind: 'ref', definition_id: definitionId })
  await reload()
}
async function addContainer(typeId: string) {
  if (!selectedTemplateId.value) return
  await createNode('template', selectedTemplateId.value, { parent_id: null, kind: 'folder', tag: typeId })
  await reload()
}
async function addRefToContainer(parentId: string, definitionId: string) {
  if (!selectedTemplateId.value) return
  await createNode('template', selectedTemplateId.value, { parent_id: parentId, kind: 'ref', definition_id: definitionId })
  await reload()
}
async function createNewPrompt() {
  if (!selectedTemplateId.value) return
  const def = await createDefinition({ type: 'prompt', name: 'New prompt', content: '', meta: {} })
  await library.loadDefinitions()
  await createNode('template', selectedTemplateId.value, { parent_id: null, kind: 'ref', definition_id: def.id })
  await reload()
}
async function createNewInContainer(parentId: string, typeId: string) {
  if (!selectedTemplateId.value) return
  const def = await createDefinition({ type: typeId, name: `New ${typeId}`, content: '', meta: {} })
  await library.loadDefinitions()
  await createNode('template', selectedTemplateId.value, { parent_id: parentId, kind: 'ref', definition_id: def.id })
  await reload()
}
async function handleDeleteNode(nodeId: string) {
  const node = nodes.value.find((n) => n.id === nodeId)
  if (!node) return
  const childCount = nodes.value.filter((n) => n.parent_id === nodeId).length
  if (node.kind === 'folder' && childCount > 0
      && !confirm(`Delete this container and its ${childCount} item(s)?`)) return
  await deleteNode(nodeId)
  await reload()
}
async function handleReorder(orderedIds: string[]) {
  if (!selectedTemplateId.value) return
  await reorderNodes('template', selectedTemplateId.value, orderedIds)
  await reload()
}
```
  Wrap each in the existing `try/catch (e) { error.value = … }` style used by the other handlers.

  Update the template markup:

```html
      <PromptTree v-if="selectedTemplateId" :nodes="nodes" :definitions="library.definitions" :types="library.containerTypes"
        @toggle-enabled="handleToggleEnabled"
        @add-prompt="addPrompt" @add-container="addContainer" @add-ref-to-container="addRefToContainer"
        @create-new-prompt="createNewPrompt" @create-new-in-container="createNewInContainer"
        @update-content="handleUpdateContent" @delete-node="handleDeleteNode" @reorder="handleReorder" />
```

- [ ] **Step 3: Build + typecheck.**

Run: `cd shirita-ui && npx vue-tsc -b`
Expected: no type errors. (Fixes any leftover `handleAddNode`/`handleCreateNew` references — delete them.)

- [ ] **Step 4: Run the whole UI suite.**

Run: `cd shirita-ui && npx vitest run`
Expected: PASS (existing + new component tests).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/BookView.vue
git commit -m "feat(ui): BookView wires v2 add/container/reorder/delete + loads types"
```

---

## Task 7: Manual smoke + full verification

**Files:** none (verification only)

- [ ] **Step 1: Typecheck + tests + build.**

Run: `cd shirita-ui && npx vue-tsc -b && npx vitest run && npx vite build`
Expected: all green.

- [ ] **Step 2: Manual smoke (dev servers).** With backend on `:8787` and `npm run dev` on `:5173`: open `/book`, create a template, confirm the **Chat history** row appears, add a **Character** container, add **Neo** into it, add a root **prompt**, drag the prompt above the history row, delete a ref. Confirm no console errors and the tree persists on reload.

- [ ] **Step 3: Commit** (only if a fix was needed during smoke; otherwise skip).

---

## Self-review checklist

- **Spec coverage (§4, §9):** history kind in TS model (T1) ✓ · types/reorder client + store cache (T1) ✓ · NodePicker dynamic types (T2) ✓ · palette icons + hover-delete + history row (T3, §9) ✓ · root Add prompt / Add container with one-container-per-type + typed container picker (T4, §4/§9) ✓ · native drag-reorder within a level incl. movable history (T5, §9) ✓ · delete with non-empty-container confirm, history non-deletable (T3 hides delete + T6 confirm) ✓ · dynamic container list from `/api/types` (T2/T6) ✓. **Deferred (noted):** trigger editor in expanded refs (Plan 4), broader palette/subheading polish (Plan 5), backend placement validation, session-tree editing.
- **Placeholder scan:** every component step shows real props/emits/markup + test code; no "add styling/validation" placeholders.
- **Type consistency:** `DefType{id,label,sort,builtin,created_at}`, `PromptNode.kind ∈ folder|ref|history`, emits `addPrompt/addContainer/addRefToContainer/createNewPrompt/createNewInContainer/deleteNode/reorder`, `reorderNodes(ownerKind,ownerId,orderedIds)`, store `containerTypes`/`loadTypes`/`addType` — identical across tasks. NodePicker gains a required `types` prop (all mount sites pass it: PromptTree T4, tests T2).
- **`v-html`:** none introduced.
