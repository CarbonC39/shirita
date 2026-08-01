# Pack/Preset Separation — Plan 7: PackEditor component + select=one exclusion helper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the self-contained `PackEditor.vue` (identity + content tree + variables for one pack) plus the pure `select=one` sibling-disable helper it uses, so Plan 8 only has to drop the editor into the Book.

**Architecture:** `PackEditor` takes a `pack` object, loads/edits its `owner_kind=pack` node tree (mirroring BookView's template-tree wiring), edits its identity (`AssetPicker` avatar + display-name input → `updatePack`) and its variables (`VariablesEditor` → `updatePack` `meta.variables`), and emits `changed` so the parent reloads the pack list. The `select=one` mutual-exclusion lives as a pure helper in `utils/tree.ts` (`selectOneSiblingsToDisable`) so it's unit-tested once and reused by PackEditor here and by the template/session toggle handlers in Plan 8.

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, Pinia, vue-i18n, lucide-vue-next, Vitest, `@vue/test-utils`.

## Global Constraints

- Reuse existing components: `PromptTree`, `VariablesEditor`, `AssetPicker` (the persona-avatar pattern: `<AssetPicker shape="circle" kind="avatar" :model-value @update:model-value>`). No new heavy components beyond `PackEditor`.
- `PackIdentity` fields are `display_name`/`avatar` (`string | null`); empty input → `null`. `updatePack(id, { name, identity, meta })` requires `name` (send the pack's current name).
- Pack node CRUD uses the existing owner-agnostic client functions with `'pack'`: `listNodes('pack', id)`, `createNode('pack', …)`, `reorderNodes('pack', …)`, plus `updateNode`/`deleteNode` (Plan 6 + earlier).
- i18n: new keys in all four locales (`en` source); `parity.test.ts` must stay green. English copy, flexible-width. Comments/commits in English.
- Test commands run without `cd`: `npm --prefix shirita-ui test -- <pattern>`; build: `npm --prefix shirita-ui run build`.

---

## File Structure

- `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` — add a `pack` i18n namespace. (Task 1)
- `shirita-ui/src/utils/tree.ts` + `tree.test.ts` — `selectOneSiblingsToDisable`. (Task 2)
- `shirita-ui/src/components/PackEditor.vue` + `PackEditor.test.ts` — the new component. (Task 3)

---

### Task 1: `pack` i18n namespace

**Files:**
- Modify: `shirita-ui/src/locales/en.ts`, `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts`
- Test: `shirita-ui/src/locales/parity.test.ts` (existing — must stay green)

**Interfaces:**
- Produces: `pack.identity`, `pack.displayName`, `pack.displayNamePlaceholder`, `pack.avatar`, `pack.contentTree`, `pack.variables` in every locale.

- [ ] **Step 1: Add the namespace to `en.ts`**

In `shirita-ui/src/locales/en.ts`, immediately after the `book: { … }` block's closing `},` (the block ending with `deleteTemplateOrphans: …`), add:

```ts
  pack: {
    identity: 'Identity',
    displayName: 'Display name',
    displayNamePlaceholder: 'Name shown in chat…',
    avatar: 'Avatar',
    contentTree: 'Content',
    variables: 'Variables',
  },
```

- [ ] **Step 2: Add the namespace to `zh-Hans.ts`** (after its `book` block)

```ts
  pack: {
    identity: '身份',
    displayName: '显示名',
    displayNamePlaceholder: '聊天中显示的名称…',
    avatar: '头像',
    contentTree: '内容',
    variables: '变量',
  },
```

- [ ] **Step 3: Add the namespace to `zh-Hant.ts`** (after its `book` block)

```ts
  pack: {
    identity: '身分',
    displayName: '顯示名稱',
    displayNamePlaceholder: '聊天中顯示的名稱…',
    avatar: '頭像',
    contentTree: '內容',
    variables: '變數',
  },
```

- [ ] **Step 4: Add the namespace to `ja.ts`** (after its `book` block)

```ts
  pack: {
    identity: 'アイデンティティ',
    displayName: '表示名',
    displayNamePlaceholder: 'チャットに表示される名前…',
    avatar: 'アバター',
    contentTree: '内容',
    variables: '変数',
  },
```

- [ ] **Step 5: Run the parity test to verify it passes**

Run: `npm --prefix shirita-ui test -- locales`
Expected: PASS — `parity.test.ts` confirms all four locales share the new `pack.*` keys (a key missing from one locale fails it).

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts
git commit -m "feat(ui): i18n pack namespace (identity/variables labels)"
```

---

### Task 2: `selectOneSiblingsToDisable` helper

**Files:**
- Modify: `shirita-ui/src/utils/tree.ts`
- Test: `shirita-ui/src/utils/tree.test.ts`

**Interfaces:**
- Consumes: `PromptNode` from `../api/types`.
- Produces: `selectOneSiblingsToDisable(nodes: PromptNode[], nodeId: string): string[]` — given the node about to be **enabled**, returns the ids of its currently-enabled `ref` siblings when (and only when) its parent is a folder with `meta.select === 'one'`; otherwise `[]`.

- [ ] **Step 1: Write the failing tests**

Add to `shirita-ui/src/utils/tree.test.ts`:

```ts
import { selectOneSiblingsToDisable } from './tree'
import type { PromptNode } from '../api/types'

function pnode(p: Partial<PromptNode>): PromptNode {
  return { id: 'x', owner_kind: 'pack', owner_id: 'o', parent_id: null, sort_order: 0,
    kind: 'ref', tag: null, definition_id: 'd', enabled: true, created_at: '', meta: {}, ...p }
}

describe('selectOneSiblingsToDisable', () => {
  const folderOne = pnode({ id: 'f', kind: 'folder', definition_id: null, meta: { select: 'one' } })
  const folderAll = pnode({ id: 'g', kind: 'folder', definition_id: null, meta: {} })

  it('returns enabled siblings under a select=one folder', () => {
    const nodes = [folderOne,
      pnode({ id: 'a', parent_id: 'f', enabled: true }),
      pnode({ id: 'b', parent_id: 'f', enabled: true }),
      pnode({ id: 'c', parent_id: 'f', enabled: false })]
    // enabling 'a' should disable the other enabled sibling 'b' (not the already-off 'c', not itself)
    expect(selectOneSiblingsToDisable(nodes, 'a')).toEqual(['b'])
  })

  it('returns nothing for an all-select folder', () => {
    const nodes = [folderAll, pnode({ id: 'a', parent_id: 'g' }), pnode({ id: 'b', parent_id: 'g' })]
    expect(selectOneSiblingsToDisable(nodes, 'a')).toEqual([])
  })

  it('returns nothing for a root node (no parent)', () => {
    const nodes = [pnode({ id: 'a', parent_id: null })]
    expect(selectOneSiblingsToDisable(nodes, 'a')).toEqual([])
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm --prefix shirita-ui test -- tree`
Expected: FAIL — `selectOneSiblingsToDisable` is not exported.

- [ ] **Step 3: Implement the helper**

In `shirita-ui/src/utils/tree.ts`, change the import line (line 1) to also import `PromptNode`:

```ts
import type { Message, PromptNode } from '../api/types'
```

Then append:

```ts
/** When `nodeId` (a ref) is about to be enabled inside a `select=one` folder,
 *  the ids of its currently-enabled ref siblings that should be turned off.
 *  Empty unless the parent is a folder with `meta.select === 'one'`. */
export function selectOneSiblingsToDisable(nodes: PromptNode[], nodeId: string): string[] {
  const node = nodes.find((n) => n.id === nodeId)
  if (!node || !node.parent_id) return []
  const parent = nodes.find((n) => n.id === node.parent_id)
  if (!parent || parent.kind !== 'folder') return []
  if ((parent.meta as Record<string, unknown>).select !== 'one') return []
  return nodes
    .filter((n) => n.parent_id === parent.id && n.id !== nodeId && n.kind === 'ref' && n.enabled)
    .map((n) => n.id)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm --prefix shirita-ui test -- tree`
Expected: PASS — the 3 new tests plus the pre-existing `activePath`/`siblings` tests.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/utils/tree.ts shirita-ui/src/utils/tree.test.ts
git commit -m "feat(ui): selectOneSiblingsToDisable tree helper"
```

---

### Task 3: `PackEditor.vue`

**Files:**
- Create: `shirita-ui/src/components/PackEditor.vue`
- Test: `shirita-ui/src/components/PackEditor.test.ts` (new)

**Interfaces:**
- Consumes: `getPack`/`updatePack`/`listNodes`/`createNode`/`updateNode`/`deleteNode`/`reorderNodes`/`createDefinition`/`updateDefinition` from `../api/client`; `selectOneSiblingsToDisable` (Task 2); `useLibraryStore`; `PromptTree`, `VariablesEditor`, `AssetPicker`; the `pack` i18n namespace (Task 1).
- Produces: `<PackEditor :pack="Pack" @changed="() => …" />`. Loads the pack's `owner_kind=pack` tree; identity edits and variable edits call `updatePack(pack.id, { name: pack.name, identity, meta })` then emit `changed`. Tree edits mutate the pack tree and reload locally.

- [ ] **Step 1: Write the failing tests**

Create `shirita-ui/src/components/PackEditor.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'

vi.mock('../api/client', () => ({
  getPack: vi.fn(),
  updatePack: vi.fn().mockResolvedValue({}),
  listNodes: vi.fn().mockResolvedValue([]),
  createNode: vi.fn().mockResolvedValue({}),
  updateNode: vi.fn().mockResolvedValue({}),
  deleteNode: vi.fn().mockResolvedValue(undefined),
  reorderNodes: vi.fn().mockResolvedValue(undefined),
  createDefinition: vi.fn().mockResolvedValue({}),
  updateDefinition: vi.fn().mockResolvedValue({}),
}))
vi.mock('../stores/library', () => ({
  useLibraryStore: () => ({ definitions: [], containerTypes: [], loadDefinitions: vi.fn(), addType: vi.fn() }),
}))

import PackEditor from './PackEditor.vue'
import * as api from '../api/client'

const pack = { id: 'p1', name: 'Alice', identity: { display_name: 'Alice', avatar: null }, meta: {}, created_at: '', updated_at: '' }
const stubs = { AssetPicker: true, PromptTree: true, VariablesEditor: true }

describe('PackEditor', () => {
  beforeEach(() => { setActivePinia(createPinia()); vi.clearAllMocks() })

  it('loads the pack node tree on mount', async () => {
    mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    expect(api.listNodes).toHaveBeenCalledWith('pack', 'p1')
  })

  it('shows the current display name', async () => {
    const w = mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    expect((w.find('[data-test="pack-display-name"]').element as HTMLInputElement).value).toBe('Alice')
  })

  it('editing the display name updates identity and emits changed', async () => {
    const w = mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    const input = w.find('[data-test="pack-display-name"]')
    await input.setValue('Alice 2')
    await input.trigger('change')
    await flushPromises()
    expect(api.updatePack).toHaveBeenCalledWith('p1', {
      name: 'Alice',
      identity: { display_name: 'Alice 2', avatar: null },
      meta: {},
    })
    expect(w.emitted('changed')).toBeTruthy()
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm --prefix shirita-ui test -- PackEditor`
Expected: FAIL — the component file does not exist (import error).

- [ ] **Step 3: Create the component**

Create `shirita-ui/src/components/PackEditor.vue`:

```vue
<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useLibraryStore } from '../stores/library'
import {
  updatePack, listNodes, createNode, updateNode, deleteNode, reorderNodes,
  createDefinition, updateDefinition,
} from '../api/client'
import { selectOneSiblingsToDisable } from '../utils/tree'
import type { Pack, PromptNode, VarDecl, Trigger } from '../api/types'
import PromptTree from './PromptTree.vue'
import VariablesEditor from './VariablesEditor.vue'
import AssetPicker from './AssetPicker.vue'

const props = defineProps<{ pack: Pack }>()
const emit = defineEmits<{ changed: [] }>()

const library = useLibraryStore()
const nodes = ref<PromptNode[]>([])
const error = ref<string | null>(null)

async function reload() {
  try { nodes.value = await listNodes('pack', props.pack.id) } catch { nodes.value = [] }
}
watch(() => props.pack.id, reload, { immediate: true })

// ── identity + variables: persist via updatePack (name is required) ──
async function save(patch: { identity?: Pack['identity']; meta?: Record<string, unknown> }) {
  try {
    await updatePack(props.pack.id, {
      name: props.pack.name,
      identity: patch.identity ?? props.pack.identity,
      meta: patch.meta ?? (props.pack.meta as Record<string, unknown>),
    })
    emit('changed')
  } catch (e) { error.value = (e as Error).message }
}
function updateDisplayName(name: string) {
  void save({ identity: { ...props.pack.identity, display_name: name.trim() || null } })
}
function updateAvatar(avatar: string) {
  void save({ identity: { ...props.pack.identity, avatar: avatar || null } })
}
const packVars = computed<VarDecl[]>(
  () => ((props.pack.meta as Record<string, unknown>).variables as VarDecl[]) ?? [],
)
function saveVars(vars: VarDecl[]) {
  void save({ meta: { ...(props.pack.meta as Record<string, unknown>), variables: vars } })
}

// ── content tree (owner_kind = 'pack'), mirrors the template-tree wiring ──
function slugifyType(name: string) {
  const slug = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '')
  return slug || `type-${Date.now().toString(36)}`
}
async function addPrompt(definitionId: string) {
  try { await createNode('pack', props.pack.id, { parent_id: null, kind: 'ref', definition_id: definitionId }); await reload() } catch (e) { error.value = (e as Error).message }
}
async function addContainer(typeId: string) {
  try { await createNode('pack', props.pack.id, { parent_id: null, kind: 'folder', tag: typeId }); await reload() } catch (e) { error.value = (e as Error).message }
}
async function addRefToContainer(parentId: string, definitionId: string) {
  try { await createNode('pack', props.pack.id, { parent_id: parentId, kind: 'ref', definition_id: definitionId }); await reload() } catch (e) { error.value = (e as Error).message }
}
async function createNewPrompt(name: string) {
  try {
    const def = await createDefinition({ type: 'prompt', name: name?.trim() || 'New prompt', content: '', meta: {} })
    await library.loadDefinitions()
    await createNode('pack', props.pack.id, { parent_id: null, kind: 'ref', definition_id: def.id })
    await reload()
  } catch (e) { error.value = (e as Error).message }
}
async function createNewInContainer(parentId: string, typeId: string) {
  try {
    const def = await createDefinition({ type: typeId, name: `New ${typeId}`, content: '', meta: {} })
    await library.loadDefinitions()
    await createNode('pack', props.pack.id, { parent_id: parentId, kind: 'ref', definition_id: def.id })
    await reload()
  } catch (e) { error.value = (e as Error).message }
}
async function createType(name: string) {
  if (!name.trim()) return
  try { const created = await library.addType(slugifyType(name), name.trim()); await addContainer(created.id) } catch (e) { error.value = (e as Error).message }
}
async function toggleEnabled(nodeId: string) {
  const node = nodes.value.find((n) => n.id === nodeId)
  if (!node) return
  const enabling = !node.enabled
  try {
    await updateNode(nodeId, { enabled: enabling })
    if (enabling) {
      for (const sib of selectOneSiblingsToDisable(nodes.value, nodeId)) {
        await updateNode(sib, { enabled: false })
      }
    }
    await reload()
  } catch (e) { error.value = (e as Error).message }
}
async function updateNodeMeta(nodeId: string, meta: Record<string, unknown>) {
  try { await updateNode(nodeId, { meta }); await reload() } catch (e) { error.value = (e as Error).message }
}
async function handleDelete(nodeId: string) {
  try { await deleteNode(nodeId); await reload() } catch (e) { error.value = (e as Error).message }
}
async function reorder(orderedIds: string[]) {
  try { await reorderNodes('pack', props.pack.id, orderedIds); await reload() } catch (e) { error.value = (e as Error).message }
}
async function updateContent(definitionId: string, content: string) {
  try { await updateDefinition(definitionId, { content }); await library.loadDefinitions() } catch (e) { error.value = (e as Error).message }
}
async function updateTrigger(definitionId: string, trigger: Trigger) {
  const def = library.definitions.find((d) => d.id === definitionId)
  if (!def) return
  try { await updateDefinition(definitionId, { meta: { ...def.meta, trigger } }); await library.loadDefinitions() } catch (e) { error.value = (e as Error).message }
}
</script>

<template>
  <div data-test="pack-editor">
    <!-- identity -->
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2.5">{{ $t('pack.identity') }}</h3>
    <div class="flex items-start gap-4 mb-4">
      <AssetPicker
        shape="circle"
        kind="avatar"
        :model-value="pack.identity.avatar || ''"
        @update:model-value="updateAvatar"
      />
      <label class="flex-1">
        <span class="text-[12px] text-muted block mb-1.5">{{ $t('pack.displayName') }}</span>
        <input
          data-test="pack-display-name"
          :value="pack.identity.display_name || ''"
          type="text"
          class="field w-full"
          :placeholder="$t('pack.displayNamePlaceholder')"
          @change="updateDisplayName(($event.target as HTMLInputElement).value)"
        />
      </label>
    </div>

    <!-- content tree -->
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2">{{ $t('pack.contentTree') }}</h3>
    <PromptTree
      :nodes="nodes"
      :definitions="library.definitions"
      :types="library.containerTypes"
      @toggle-enabled="toggleEnabled"
      @add-prompt="addPrompt"
      @add-container="addContainer"
      @add-ref-to-container="addRefToContainer"
      @create-new-prompt="createNewPrompt"
      @create-new-in-container="createNewInContainer"
      @create-type="createType"
      @update-content="updateContent"
      @update-trigger="updateTrigger"
      @update-node-meta="updateNodeMeta"
      @delete-node="handleDelete"
      @reorder="reorder"
    />

    <!-- variables -->
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mt-4 mb-2">{{ $t('pack.variables') }}</h3>
    <VariablesEditor :model-value="packVars" @update:model-value="saveVars" />

    <p v-if="error" class="text-coral text-sm mt-3">{{ error }}</p>
  </div>
</template>
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm --prefix shirita-ui test -- PackEditor`
Expected: PASS — all 3 tests (mount loads the tree; display name shows + editing it calls `updatePack` and emits `changed`).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/PackEditor.vue shirita-ui/src/components/PackEditor.test.ts
git commit -m "feat(ui): PackEditor (identity + pack tree + variables)"
```

---

## Final Verification

- [ ] **Full UI test + typecheck sweep**

Run: `npm --prefix shirita-ui test 2>&1 | tail -8 && npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: all Vitest suites pass; build succeeds with no type errors.

---

## Self-Review

**Spec coverage (subset of spec §3.2 Pack editor + §3.4 behavior):**
- Pack identity editor (avatar + display name → `updatePack`) — Task 3.
- Pack content tree (`owner_kind=pack`, reuses PromptTree) — Task 3.
- Pack variables (`pack.meta.variables` via `updatePack`) — Task 3.
- `select=one` enable-time mutual exclusion — Task 2 (pure helper) + used in PackEditor's `toggleEnabled` (Task 3); reused by template/session handlers in Plan 8.
- i18n labels — Task 1.
- Deferred to **Plan 8**: mounting `<PackEditor>` in the Book's new PACK section, the Template/Pack `EntityPicker` pickers, Template-first reorder, section color-coding, and wiring `selectOneSiblingsToDisable` into BookView's template + session toggle handlers. Deferred to **Plan 9**: single-screen new-chat.

**Placeholder scan:** none — full code and exact commands throughout.

**Type consistency:** `PackEditor` props (`pack: Pack`) + emit (`changed`) match the test and (Plan 8) the BookView usage. `updatePack(id, { name, identity, meta })` body matches the Plan 6 client signature and the backend `PackBody`. `selectOneSiblingsToDisable(nodes, nodeId)` signature is identical in Task 2, its test, and PackEditor. All PromptTree event names wired in PackEditor (`toggle-enabled`, `add-prompt`, `add-ref-to-container`, `add-container`, `create-new-prompt`, `create-new-in-container`, `create-type`, `update-content`, `update-trigger`, `update-node-meta`, `delete-node`, `reorder`) match `PromptTree.vue`'s `defineEmits`.
