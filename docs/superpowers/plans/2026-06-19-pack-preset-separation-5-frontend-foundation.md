# Pack/Preset Separation — Plan 5: Frontend Foundation (content row, folder select, pack types) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Teach the shared prompt-tree UI the two new node concepts — the undeletable `content` mount row and the folder `select` (all/one) switch — and add the pack-owner TypeScript vocabulary that the later editor/new-chat plans build on.

**Architecture:** The tree row component `NodeRow.vue` is reused for template, session, and (next plans) pack trees, so both new behaviors live there. `content` renders like the existing `history` magic row (toggle-able, no delete, no add, no expand); the folder `select` switch writes `meta.select` via the existing `updateNodeMeta` emit (the backend already honors `meta.select === "one"` in `assembly.rs`). No `PromptTree.vue` change is needed — it already forwards `toggleEnabled`/`updateNodeMeta` for root nodes and skips child rendering for non-folders. Frontend tests are Vitest + `@vue/test-utils`; i18n keys go into all four locales so the parity test stays green.

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, vue-i18n, lucide-vue-next icons, Vitest, vue-tsc.

## Global Constraints

- Build UI in English copy; keep labels flexible-width (no fixed-width label layouts). Comments and commit messages in English.
- i18n: every new key MUST be added to all four locales (`en`, `zh-Hans`, `zh-Hant`, `ja`) or `src/locales/parity.test.ts` fails. `en.ts` is the source schema.
- The backend `select=one` semantics are already implemented in `shirita-core/src/assembly.rs` (`pack_pairs`): among a folder's enabled children in sort order, only the first that renders is emitted. This plan only adds the **switch UI** that sets `meta.select`; the enable-time "auto-disable siblings" UX is deferred to Plan 6 (needs the editor/store wiring).
- Run UI commands without `cd` (they trigger a prompt): use `npm --prefix shirita-ui …`.
- Vitest filter: `npm --prefix shirita-ui test -- <pattern>` runs only matching files. Typecheck/build gate: `npm --prefix shirita-ui run build`.

---

## File Structure

- `shirita-ui/src/api/types.ts` — extend `PromptNode.kind` (`+'content'`) and `owner_kind` (`+'pack'`); add `PackIdentity` + `Pack` interfaces (consumed by Plans 6/7). (Task 1)
- `shirita-ui/src/components/NodeRow.vue` — render the `content` row (Task 1); render the folder `select` switch (Task 2).
- `shirita-ui/src/components/NodeRow.test.ts` — content-row tests (Task 1); select-switch tests (Task 2).
- `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` — `prompt.contentMount` (Task 1); `prompt.selectAll`, `prompt.selectOne`, `prompt.selectModeHint` (Task 2).

---

### Task 1: `content` mount row + pack-owner types

**Files:**
- Modify: `shirita-ui/src/api/types.ts:66-78` (`PromptNode`), and add `Pack`/`PackIdentity` interfaces
- Modify: `shirita-ui/src/components/NodeRow.vue`
- Modify: `shirita-ui/src/locales/en.ts:77` area, `zh-Hans.ts:70` area, `zh-Hant.ts:70` area, `ja.ts:70` area
- Test: `shirita-ui/src/components/NodeRow.test.ts`

**Interfaces:**
- Consumes: existing `NodeRow` props `{ node: PromptNode; definitions: Record<string,Definition>; depth: number; isExpanded: boolean }` and emits (`toggleEnabled`, `delete`, `add`, `toggleExpand`, `updateNodeMeta`, …).
- Produces: `PromptNode.kind` now includes `'content'`; `PromptNode.owner_kind` now includes `'pack'`; `export interface PackIdentity { display_name: string | null; avatar: string | null }`; `export interface Pack { id: string; name: string; identity: PackIdentity; meta: Record<string, unknown>; created_at: string; updated_at: string }`. NodeRow renders a content row with `data-test="enable-checkbox"` present and `data-test="node-delete"` / `data-test="node-add"` absent.

- [ ] **Step 1: Write the failing test**

Add to `shirita-ui/src/components/NodeRow.test.ts`, inside the `describe('NodeRow', …)` block (after the `history row` test):

```ts
  it('content row shows the mounted-packs label, an enable toggle, and no delete/add', () => {
    const c = node({ kind: 'content', definition_id: null, tag: null })
    const w = mount(NodeRow, { props: { node: c, definitions: defs, depth: 0, isExpanded: false } })
    expect(w.text()).toContain('Mounted packs')
    expect(w.find('[data-test="enable-checkbox"]').exists()).toBe(true)
    expect(w.find('[data-test="node-delete"]').exists()).toBe(false)
    expect(w.find('[data-test="node-add"]').exists()).toBe(false)
  })
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm --prefix shirita-ui test -- NodeRow`
Expected: FAIL — the content node renders the fallback `(missing)` / a ref row (delete button present), so `toContain('Mounted packs')` and the delete-absent assertion fail. (A TS error on `kind: 'content'` is also acceptable as the failing state.)

- [ ] **Step 3: Extend the types**

In `shirita-ui/src/api/types.ts`, change the `PromptNode` interface (lines 66-78). Replace:

```ts
export interface PromptNode {
  id: string
  owner_kind: 'template' | 'session'
  owner_id: string
  parent_id: string | null
  sort_order: number
  kind: 'folder' | 'ref' | 'history'
  tag: string | null
  definition_id: string | null
  enabled: boolean
  created_at: string
  meta: Record<string, unknown>
}
```

with:

```ts
export interface PromptNode {
  id: string
  owner_kind: 'template' | 'session' | 'pack'
  owner_id: string
  parent_id: string | null
  sort_order: number
  kind: 'folder' | 'ref' | 'history' | 'content'
  tag: string | null
  definition_id: string | null
  enabled: boolean
  created_at: string
  meta: Record<string, unknown>
}

/** A pack's bound display identity (mirrors core PackIdentity; empty == unset). */
export interface PackIdentity {
  display_name: string | null
  avatar: string | null
}

/** A content bundle: its own node tree plus an optional bound identity. */
export interface Pack {
  id: string
  name: string
  identity: PackIdentity
  meta: Record<string, unknown>
  created_at: string
  updated_at: string
}
```

- [ ] **Step 4: Render the content row in NodeRow**

In `shirita-ui/src/components/NodeRow.vue`:

(a) Add the `Package` icon to the lucide import (line 4). Replace:

```ts
import { ChevronRight, Folder, FileText, History, Check, Maximize2, Trash2, Plus, GripVertical } from 'lucide-vue-next'
```

with:

```ts
import { ChevronRight, Folder, FileText, History, Package, Check, Maximize2, Trash2, Plus, GripVertical } from 'lucide-vue-next'
```

(b) Add an `isContent` computed beside `isHistory` (after line 30):

```ts
const isContent = computed(() => props.node.kind === 'content')
```

(c) Extend the `label` computed (lines 36-40) to handle content. Replace:

```ts
const label = computed(() => {
  if (isHistory.value) return t('prompt.chatHistory')
  if (isFolder.value) return props.node.tag || t('prompt.folderFallback')
  return def.value ? def.value.name : t('prompt.missing')
})
```

with:

```ts
const label = computed(() => {
  if (isHistory.value) return t('prompt.chatHistory')
  if (isContent.value) return t('prompt.contentMount')
  if (isFolder.value) return props.node.tag || t('prompt.folderFallback')
  return def.value ? def.value.name : t('prompt.missing')
})
```

(d) Add the Package icon to the icon block. Replace:

```html
      <!-- type icon -->
      <History v-if="isHistory" :size="16" class="text-primary shrink-0" :stroke-width="1.8" />
      <Folder v-else-if="isFolder" :size="17" :class="iconColor" class="shrink-0" :stroke-width="1.8" />
      <FileText v-else :size="16" :class="iconColor" class="shrink-0" :stroke-width="1.8" />
```

with:

```html
      <!-- type icon -->
      <History v-if="isHistory" :size="16" class="text-primary shrink-0" :stroke-width="1.8" />
      <Package v-else-if="isContent" :size="16" class="text-primary shrink-0" :stroke-width="1.8" />
      <Folder v-else-if="isFolder" :size="17" :class="iconColor" class="shrink-0" :stroke-width="1.8" />
      <FileText v-else :size="16" :class="iconColor" class="shrink-0" :stroke-width="1.8" />
```

(e) Hide delete for content (it is undeletable). Replace:

```html
      <!-- delete (history rows render none) -->
      <button
        v-if="!isHistory"
        data-test="node-delete"
```

with:

```html
      <!-- delete (history + content rows render none) -->
      <button
        v-if="!isHistory && !isContent"
        data-test="node-delete"
```

(f) Show the expand chevron only for rows that expand (folders/refs), not the magic rows. Replace:

```html
      <!-- trailing expand chevron: folders expand children, refs expand content -->
      <button data-test="expand-btn" class="text-muted/70 hover:text-ink shrink-0 p-0.5" @click="emit('toggleExpand')">
```

with:

```html
      <!-- trailing expand chevron: folders expand children, refs expand content -->
      <button v-if="!isHistory && !isContent" data-test="expand-btn" class="text-muted/70 hover:text-ink shrink-0 p-0.5" @click="emit('toggleExpand')">
```

(g) Defensively exclude content from the inline content-editor block. Replace:

```html
    <div v-if="!isFolder && !isHistory && isExpanded" :style="{ paddingLeft: `${8 + (depth + 1) * 26}px` }" class="pr-2 pb-2 pt-0.5">
```

with:

```html
    <div v-if="!isFolder && !isHistory && !isContent && isExpanded" :style="{ paddingLeft: `${8 + (depth + 1) * 26}px` }" class="pr-2 pb-2 pt-0.5">
```

- [ ] **Step 5: Add the `contentMount` i18n key to all four locales**

`shirita-ui/src/locales/en.ts` — after the `chatHistory: 'Chat history',` line (77), add:

```ts
    contentMount: 'Mounted packs',
```

`shirita-ui/src/locales/zh-Hans.ts` — after `chatHistory: '聊天记录',` (line 70), add:

```ts
    contentMount: '挂载的包',
```

`shirita-ui/src/locales/zh-Hant.ts` — after `chatHistory: '聊天記錄',` (line 70), add:

```ts
    contentMount: '掛載的包',
```

`shirita-ui/src/locales/ja.ts` — after `chatHistory: 'チャット履歴',` (line 70), add:

```ts
    contentMount: 'マウント済みパック',
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `npm --prefix shirita-ui test -- NodeRow locales`
Expected: PASS — the new content-row test plus all pre-existing `NodeRow` tests and the locale `parity` test (all four locales now share `prompt.contentMount`).

- [ ] **Step 7: Typecheck**

Run: `npm --prefix shirita-ui run build 2>&1 | tail -15`
Expected: build succeeds (vue-tsc reports no type errors from the new `'content'`/`'pack'` unions or the new interfaces).

- [ ] **Step 8: Commit**

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/components/NodeRow.vue shirita-ui/src/components/NodeRow.test.ts shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts
git commit -m "feat(ui): render content mount row + pack-owner types"
```

---

### Task 2: Folder `select` (all/one) switch

**Files:**
- Modify: `shirita-ui/src/components/NodeRow.vue`
- Modify: `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts`
- Test: `shirita-ui/src/components/NodeRow.test.ts`

**Interfaces:**
- Consumes: existing `updateNodeMeta: [meta: Record<string, unknown>]` emit (already wired through `PromptTree.vue:142` for root folders).
- Produces: a folder-only `data-test="select-mode"` control. Clicking it emits `updateNodeMeta` with `{ ...node.meta, select: <toggled> }` where the value flips between `'one'` and `'all'`. Label reads `prompt.selectAll` when `meta.select !== 'one'` (default), `prompt.selectOne` when `meta.select === 'one'`.

- [ ] **Step 1: Write the failing tests**

Add to `shirita-ui/src/components/NodeRow.test.ts`, inside `describe('NodeRow', …)`:

```ts
  it('ref rows have no select-mode switch', () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 0, isExpanded: false } })
    expect(w.find('[data-test="select-mode"]').exists()).toBe(false)
  })

  it('folder select-mode defaults to All and toggles to one via updateNodeMeta', async () => {
    const folder = node({ kind: 'folder', tag: 'style', definition_id: null, meta: {} })
    const w = mount(NodeRow, { props: { node: folder, definitions: defs, depth: 0, isExpanded: false } })
    const btn = w.find('[data-test="select-mode"]')
    expect(btn.exists()).toBe(true)
    expect(btn.text()).toBe('All')
    await btn.trigger('click')
    expect(w.emitted('updateNodeMeta')![0]).toEqual([{ select: 'one' }])
  })

  it('folder select-mode reads an existing meta.select=one as Single', () => {
    const folder = node({ kind: 'folder', tag: 'style', definition_id: null, meta: { select: 'one' } })
    const w = mount(NodeRow, { props: { node: folder, definitions: defs, depth: 0, isExpanded: false } })
    expect(w.find('[data-test="select-mode"]').text()).toBe('Single')
  })
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm --prefix shirita-ui test -- NodeRow`
Expected: FAIL — `data-test="select-mode"` does not exist yet (the folder tests fail on `btn.exists()` / `.text()`).

- [ ] **Step 3: Add the select-mode computed + handler**

In `shirita-ui/src/components/NodeRow.vue`, after the `isContent` computed added in Task 1, add:

```ts
// Folder selection policy: 'all' (default) renders every enabled child; 'one'
// renders only the first (a single-select hub). Stored in node.meta.select;
// the backend (assembly.rs pack_pairs) honors it.
const selectMode = computed(() =>
  ((props.node.meta as Record<string, unknown>).select === 'one' ? 'one' : 'all'),
)
function toggleSelectMode() {
  const next = selectMode.value === 'one' ? 'all' : 'one'
  emit('updateNodeMeta', { ...(props.node.meta as Record<string, unknown>), select: next })
}
```

- [ ] **Step 4: Render the switch on folder rows**

In `shirita-ui/src/components/NodeRow.vue`, add the switch immediately before the folder add button. Replace:

```html
      <!-- add-to-container: lives beside delete, no extra row (containers only) -->
      <button
        v-if="isFolder"
        data-test="node-add"
```

with:

```html
      <!-- folder selection policy: all vs single-select -->
      <button
        v-if="isFolder"
        data-test="select-mode"
        class="shrink-0 text-[11px] px-1.5 py-0.5 rounded-md border border-line text-muted hover:text-ink transition-colors"
        :title="$t('prompt.selectModeHint')"
        @click.stop="toggleSelectMode"
      >{{ selectMode === 'one' ? $t('prompt.selectOne') : $t('prompt.selectAll') }}</button>

      <!-- add-to-container: lives beside delete, no extra row (containers only) -->
      <button
        v-if="isFolder"
        data-test="node-add"
```

- [ ] **Step 5: Add the select i18n keys to all four locales**

`shirita-ui/src/locales/en.ts` — after the `contentMount` line added in Task 1, add:

```ts
    selectAll: 'All',
    selectOne: 'Single',
    selectModeHint: 'All: render every enabled item. Single: render only the first.',
```

`shirita-ui/src/locales/zh-Hans.ts` — after its `contentMount` line, add:

```ts
    selectAll: '全部',
    selectOne: '单选',
    selectModeHint: '全部：渲染所有启用项；单选：只渲染第一项。',
```

`shirita-ui/src/locales/zh-Hant.ts` — after its `contentMount` line, add:

```ts
    selectAll: '全部',
    selectOne: '單選',
    selectModeHint: '全部：渲染所有啟用項；單選：只渲染第一項。',
```

`shirita-ui/src/locales/ja.ts` — after its `contentMount` line, add:

```ts
    selectAll: 'すべて',
    selectOne: '単一',
    selectModeHint: 'すべて：有効な項目をすべて表示。単一：最初の1件のみ表示。',
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `npm --prefix shirita-ui test -- NodeRow locales`
Expected: PASS — the 3 new select-mode tests plus all pre-existing `NodeRow` tests and the locale `parity` test.

- [ ] **Step 7: Typecheck**

Run: `npm --prefix shirita-ui run build 2>&1 | tail -15`
Expected: build succeeds, no type errors.

- [ ] **Step 8: Commit**

```bash
git add shirita-ui/src/components/NodeRow.vue shirita-ui/src/components/NodeRow.test.ts shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts
git commit -m "feat(ui): folder select all/one switch on node rows"
```

---

## Final Verification

- [ ] **Full UI test + typecheck sweep**

Run: `npm --prefix shirita-ui test 2>&1 | tail -20 && npm --prefix shirita-ui run build 2>&1 | tail -10`
Expected: all Vitest suites pass; build succeeds with no type errors.

---

## Self-Review

**Spec coverage (Plan 5 scope — spec §12 "PromptTree 组件" + §13):**
- Render `content` row (专属图标、可停用、不可删，类 history 行) — Task 1 (Package icon, enable toggle kept, delete/add/expand hidden).
- Folder `select`（all/one 切换）— Task 2 (switch writing `meta.select`).
- wrap-in-tag toggle — already present in `NodeRow.vue` (`showWrapToggle`/`wrapValue`), no work needed.
- `owner_kind: 'pack'` + `Pack`/`PackIdentity` types — Task 1 (foundation consumed by Plans 6/7).
- Deferred (out of this plan, noted in Global Constraints): the select=one enable-time "auto-disable siblings" UX → Plan 6 (editor/store wiring); the Pack/Template editor split → Plan 6; the two-step new-chat flow → Plan 7.

**Placeholder scan:** No TBD/TODO; every code step shows full before/after; commands include expected output.

**Type consistency:** `meta.select` value `'one'`/`'all'` matches `assembly.rs` (`== Some("one")`). `PackIdentity { display_name, avatar }` mirrors core's `PackIdentity` (snake_case fields). `data-test="select-mode"` is identical between the Task 2 test and template. The `updateNodeMeta` payload shape `{ ...meta, select }` matches the existing `wrap_in_tag` precedent in the same component.
