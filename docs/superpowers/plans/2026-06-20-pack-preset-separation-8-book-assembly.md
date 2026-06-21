# Pack/Preset Separation — Plan 8: Book assembly (PACK section + search pickers + select=one wiring) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the Plan 6/7 building blocks into `BookView` so the Book becomes Template → Pack → Definitions (single column), with search-box pickers, a mauve/teal/neutral accent per section, and `select=one` single-select enforcement.

**Architecture:** `BookView` gains a PACK section between the existing Template tree and the Definitions editor: an `EntityPicker` to pick/create a pack, rename/duplicate/delete ops, and the `PackEditor` for the selected pack. The Template picker's `<select>` is replaced by an `EntityPicker` for consistency, and the `select=one` sibling-disable helper is wired into the template and session toggle handlers. No new components — everything was built in Plans 6/7.

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, Pinia, vue-i18n, lucide-vue-next, Vitest, `@vue/test-utils`.

## Global Constraints

- Single column, no tabs. Section order: **Template → Pack → Definitions**. Accent per section heading + left rule: **Template = `primary` (teal)**, **Pack = `mauve`**, **Definitions = neutral** (the existing `DefinitionEditor` heading is already `text-ink/65`, so no change there). Accent is heading chrome only.
- The Book is an **editor**: pickers are labeled "Edit template"/"Edit pack".
- Reuse `EntityPicker` (Plan 6) and `PackEditor` (Plan 7) and `selectOneSiblingsToDisable` (Plan 7).
- i18n keys added to all four locales (`en` source); `parity.test.ts` stays green. English copy; flexible-width. Comments/commits in English.
- Test commands run without `cd`: `npm --prefix shirita-ui test -- <pattern>`; build: `npm --prefix shirita-ui run build`.
- `library.packs` / `library.loadPacks` exist (Plan 6); `createPack/updatePack/deletePack/duplicatePack` exist (Plan 6).

---

## File Structure

- `shirita-ui/src/views/BookView.vue` — add PACK section + swap Template picker + wire `select=one`. (all tasks)
- `shirita-ui/src/views/BookView.test.ts` — extend mocks + assert the PACK section. (Task 1)
- `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` — pack-section labels (Task 1), template-picker labels (Task 2).

---

### Task 1: Add the PACK section to BookView

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue` (imports, `onMounted`, new pack state/handlers, new template block)
- Modify: `shirita-ui/src/views/BookView.test.ts`
- Modify: `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts`

**Interfaces:**
- Consumes: `EntityPicker` (Plan 6), `PackEditor` (Plan 7), `library.packs`/`loadPacks` + `createPack/updatePack/deletePack/duplicatePack` (Plan 6).
- Produces: a `data-test="book-pack"` section with `data-test="pack-picker"` (an EntityPicker) and `<PackEditor>` for the selected pack; `data-test="section-pack"` mauve heading.

- [ ] **Step 1: Add the pack-section i18n keys (all four locales)**

`shirita-ui/src/locales/en.ts` — inside the `book: { … }` block, after `deleteTemplateOrphans: …,` (before the block's closing `},`), add:

```ts
    packHeading: 'Pack',
    editPack: 'Edit pack…',
    createPack: 'New pack',
    packNamePlaceholder: 'Pack name',
    deletePackConfirm: 'Delete this pack?',
```

`zh-Hans.ts` (same spot in its `book` block):

```ts
    packHeading: '包',
    editPack: '编辑包…',
    createPack: '新建包',
    packNamePlaceholder: '包名称',
    deletePackConfirm: '删除这个包？',
```

`zh-Hant.ts`:

```ts
    packHeading: '包',
    editPack: '編輯包…',
    createPack: '新增包',
    packNamePlaceholder: '包名稱',
    deletePackConfirm: '刪除這個包？',
```

`ja.ts`:

```ts
    packHeading: 'パック',
    editPack: 'パックを編集…',
    createPack: 'パックを新規作成',
    packNamePlaceholder: 'パック名',
    deletePackConfirm: 'このパックを削除しますか？',
```

- [ ] **Step 2: Write the failing test**

In `shirita-ui/src/views/BookView.test.ts`, extend the `../api/client` mock (add the pack functions) and the `../stores/library` mock (add `packs`/`loadPacks`):

In the `vi.mock('../api/client', () => ({ … }))` object add:

```ts
  listPacks: vi.fn().mockResolvedValue([]),
  createPack: vi.fn().mockResolvedValue({ id: 'np' }),
  updatePack: vi.fn().mockResolvedValue({}),
  deletePack: vi.fn().mockResolvedValue(undefined),
  duplicatePack: vi.fn().mockResolvedValue({ id: 'dp' }),
```

In the `vi.mock('../stores/library', …)` `useLibraryStore` object add:

```ts
    packs: [], loadPacks: vi.fn(),
```

Then add a test:

```ts
  it('shows the Pack section (picker + heading) in the global view', async () => {
    const ui = useUiStore(); ui.setActiveChatId(null)
    const w = mount(BookView)
    await flushPromises()
    expect(w.find('[data-test="book-pack"]').exists()).toBe(true)
    expect(w.find('[data-test="section-pack"]').exists()).toBe(true)
    expect(w.find('[data-test="pack-picker"]').exists()).toBe(true)
  })
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `npm --prefix shirita-ui test -- BookView`
Expected: FAIL — `book-pack` / `section-pack` / `pack-picker` do not exist yet.

- [ ] **Step 4: Add imports + pack state/handlers to BookView script**

In `shirita-ui/src/views/BookView.vue`:

(a) Add the pack client functions to the existing `from "../api/client"` import list (it ends with `exportTemplatePath,`): add on their own lines inside that import block:

```ts
    createPack,
    updatePack,
    deletePack,
    duplicatePack,
```

(b) Add the two component imports after `import VariablesEditor from "../components/VariablesEditor.vue";`:

```ts
import EntityPicker from "../components/EntityPicker.vue";
import PackEditor from "../components/PackEditor.vue";
```

(c) Add `library.loadPacks()` to the `onMounted` `Promise.all` (the array currently has `loadTemplates`, `loadDefinitions`, `loadTypes`):

```ts
        await Promise.all([
            library.loadTemplates(),
            library.loadDefinitions(),
            library.loadTypes(),
            library.loadPacks(),
        ]);
```

(d) Add the pack state + handlers (place just before the `// ── definition editor ──` comment near the end of `<script setup>`):

```ts
// ── packs ───────────────────────────────────────────────────
const selectedPackId = ref<string | null>(null);
const selectedPack = computed(() => library.packs.find((p) => p.id === selectedPackId.value) ?? null);
const renamingPack = ref(false);
const packNameDraft = ref("");

function selectPack(id: string) { selectedPackId.value = id || null; }
async function createPackNamed(name: string) {
    try {
        const p = await createPack({ name: name?.trim() || "New pack" });
        await library.loadPacks();
        selectedPackId.value = p.id;
    } catch (e) { error.value = (e as Error).message; }
}
function startRenamePack() {
    if (!selectedPack.value) return;
    packNameDraft.value = selectedPack.value.name;
    renamingPack.value = true;
}
async function renamePack() {
    const p = selectedPack.value;
    const n = packNameDraft.value.trim();
    renamingPack.value = false;
    if (!p || !n || n === p.name) return;
    try {
        await updatePack(p.id, { name: n, identity: p.identity, meta: p.meta as Record<string, unknown> });
        await library.loadPacks();
    } catch (e) { error.value = (e as Error).message; }
}
async function dupPack() {
    if (!selectedPackId.value) return;
    try { const p = await duplicatePack(selectedPackId.value); await library.loadPacks(); selectedPackId.value = p.id; }
    catch (e) { error.value = (e as Error).message; }
}
async function delPack() {
    if (!selectedPackId.value) return;
    if (!confirm(tr("book.deletePackConfirm"))) return;
    try { await deletePack(selectedPackId.value); selectedPackId.value = null; await library.loadPacks(); }
    catch (e) { error.value = (e as Error).message; }
}
```

- [ ] **Step 5: Add the PACK section markup**

In `shirita-ui/src/views/BookView.vue`, find the template-variables block followed by the divider that precedes the `DefinitionEditor` (the `<div class="h-px bg-line my-6" />` right before `<DefinitionEditor :definition="editDef" …>`). **Insert the PACK section between that divider and the `<DefinitionEditor>`** so the order becomes Template → (divider) → Pack → (divider) → Definitions:

```html
            <!-- PACK section (mauve accent) -->
            <h2 data-test="section-pack" class="flex items-center text-[12px] font-semibold uppercase tracking-wide text-mauve border-l-2 border-mauve pl-2 mb-3">{{ $t('book.packHeading') }}</h2>
            <div data-test="book-pack" class="mb-2">
                <div class="flex items-center gap-2 mb-3">
                    <input
                        v-if="renamingPack"
                        v-model="packNameDraft"
                        type="text"
                        class="field flex-1"
                        :placeholder="$t('book.packNamePlaceholder')"
                        @keydown.enter="renamePack"
                        @blur="renamePack"
                    />
                    <EntityPicker
                        v-else
                        class="flex-1"
                        data-test="pack-picker"
                        :items="library.packs.map((p) => ({ id: p.id, name: p.name }))"
                        :placeholder="$t('book.editPack')"
                        :create-label="$t('book.createPack')"
                        @select="selectPack"
                        @create="createPackNamed"
                    />
                    <div v-if="selectedPack" class="flex items-center">
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" :title="$t('common.rename')" @click="startRenamePack"><Pencil :size="15" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" :title="$t('common.duplicate')" @click="dupPack"><Copy :size="16" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-coral rounded-lg" :title="$t('common.delete')" @click="delPack"><Trash2 :size="16" /></button>
                    </div>
                </div>
                <PackEditor v-if="selectedPack" :pack="selectedPack" @changed="library.loadPacks()" />
            </div>

            <div class="h-px bg-line my-6" />
```

(The `Pencil`/`Copy`/`Trash2` icons are already imported at the top of `BookView.vue`.)

- [ ] **Step 6: Run the test to verify it passes**

Run: `npm --prefix shirita-ui test -- BookView locales`
Expected: PASS — the new Pack-section test, the pre-existing BookView scope tests (the library mock now provides `packs`/`loadPacks`), and the locale parity test.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/views/BookView.vue shirita-ui/src/views/BookView.test.ts shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts
git commit -m "feat(ui): Book PACK section (picker + PackEditor)"
```

---

### Task 2: Swap the Template picker to EntityPicker + teal heading

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue`
- Modify: `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts`

**Interfaces:**
- Consumes: `EntityPicker` (imported in Task 1).
- Produces: `data-test="template-picker"` (an EntityPicker) replaces the `<select>`; `data-test="section-template"` teal heading; `selectTemplate(id)` no longer special-cases `"__new__"`; new `createTemplateNamed(name)`.

- [ ] **Step 1: Add the template-picker i18n keys (all four locales)**

`en.ts` `book` block:

```ts
    templateHeading: 'Template',
    editTemplate: 'Edit template…',
    createTemplate: 'New template',
```

`zh-Hans.ts`:

```ts
    templateHeading: '模板',
    editTemplate: '编辑模板…',
    createTemplate: '新建模板',
```

`zh-Hant.ts`:

```ts
    templateHeading: '範本',
    editTemplate: '編輯範本…',
    createTemplate: '新增範本',
```

`ja.ts`:

```ts
    templateHeading: 'テンプレート',
    editTemplate: 'テンプレートを編集…',
    createTemplate: 'テンプレートを新規作成',
```

- [ ] **Step 2: Replace the dead create-draft code with `createTemplateNamed`**

In `shirita-ui/src/views/BookView.vue`:

(a) Remove the now-unused `creatingTemplate` ref declaration (the line `const creatingTemplate = ref(false);`).

(b) In `selectTemplate`, remove the `__new__` special-case (the first three lines of the function):

```ts
async function selectTemplate(id: string) {
    if (id === "__new__") {
        startDraft();
        return;
    }
    selectedTemplateId.value = id || null;
```

becomes:

```ts
async function selectTemplate(id: string) {
    selectedTemplateId.value = id || null;
```

(c) Replace the three functions `startDraft`, `finishCreateTemplate`, `cancelCreateTemplate` with a single `createTemplateNamed`:

```ts
async function createTemplateNamed(name: string) {
    try {
        const t = await createTemplate(name.trim() || "New template");
        await library.loadTemplates();
        await selectTemplate(t.id);
    } catch (e) {
        error.value = (e as Error).message;
    }
}
```

- [ ] **Step 3: Replace the picker markup + add the teal heading**

In the template (around the `<section data-test="book-global">` open), the picker is a `<div class="flex items-center gap-2">` whose first child is `<template v-if="creatingTemplate"> … </template><template v-else><select …>…</select></template>`. Replace that `creatingTemplate`/`<select>` pair with an `EntityPicker`, and add a teal heading just above the wrapping div. The result:

```html
            <!-- TEMPLATE section (teal accent) -->
            <h2 data-test="section-template" class="flex items-center text-[12px] font-semibold uppercase tracking-wide text-primary border-l-2 border-primary pl-2 mb-3">{{ $t('book.templateHeading') }}</h2>
            <!-- template picker / ops -->
            <div class="flex items-center gap-2">
                <EntityPicker
                    class="flex-1"
                    data-test="template-picker"
                    :items="library.templates.map((t) => ({ id: t.id, name: t.name }))"
                    :placeholder="$t('book.editTemplate')"
                    :create-label="$t('book.createTemplate')"
                    @select="selectTemplate"
                    @create="createTemplateNamed"
                />
                <div class="flex items-center">
```

(Keep the existing ops `<div class="flex items-center">…</div>` — rename/import/export/duplicate/delete — and everything after it unchanged. Only the `creatingTemplate`/`<select>` pair at the top of the picker `div` is replaced, and the teal heading is added above the `div`.)

- [ ] **Step 4: Run the tests + typecheck**

Run: `npm --prefix shirita-ui test -- BookView locales 2>&1 | tail -12 && npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: PASS — BookView + parity tests; build clean (no unused-symbol errors — `creatingTemplate`/`startDraft`/`finishCreateTemplate`/`cancelCreateTemplate` are gone and `createTemplateNamed` is used by the picker).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/BookView.vue shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts
git commit -m "feat(ui): Template search picker + section headings"
```

---

### Task 3: Wire select=one mutual-exclusion into the Book toggle handlers

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue`

**Interfaces:**
- Consumes: `selectOneSiblingsToDisable` from `../utils/tree` (Plan 7).
- Produces: enabling a child in a `select=one` folder disables its enabled siblings, in both the template tree (`handleToggleEnabled`) and the session/local tree (`localToggleEnabled`).

- [ ] **Step 1: Import the helper**

In `shirita-ui/src/views/BookView.vue`, add after the `import type { … } from "../api/types";` line:

```ts
import { selectOneSiblingsToDisable } from "../utils/tree";
```

- [ ] **Step 2: Wire it into the template toggle handler**

Replace `handleToggleEnabled` (the template-tree handler that currently does an in-place splice) with a reload-based version that disables `select=one` siblings:

```ts
async function handleToggleEnabled(nodeId: string) {
    const node = nodes.value.find((n) => n.id === nodeId);
    if (!node) return;
    const enabling = !node.enabled;
    try {
        await updateNode(nodeId, { enabled: enabling });
        if (enabling) {
            for (const sib of selectOneSiblingsToDisable(nodes.value, nodeId)) {
                await updateNode(sib, { enabled: false });
            }
        }
        await reload();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
```

- [ ] **Step 3: Wire it into the session/local toggle handler**

Replace `localToggleEnabled` with:

```ts
async function localToggleEnabled(nodeId: string) {
    const sid = ui.activeChatId;
    if (!sid) return;
    const node = localNodes.value.find((n) => n.id === nodeId);
    if (!node) return;
    const enabling = !node.enabled;
    try {
        await ensureMaterialized();
        await updateNode(nodeId, { enabled: enabling });
        if (enabling) {
            for (const sib of selectOneSiblingsToDisable(localNodes.value, nodeId)) {
                await updateNode(sib, { enabled: false });
            }
        }
        await loadLocalNodes();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
```

- [ ] **Step 4: Run the full suite + typecheck**

Run: `npm --prefix shirita-ui test 2>&1 | tail -6 && npm --prefix shirita-ui run build 2>&1 | tail -4`
Expected: all Vitest suites pass; build clean. (The exclusion logic itself is unit-tested via `selectOneSiblingsToDisable` in Plan 7; this step confirms the two handlers compile and the existing BookView tests still pass.)

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/BookView.vue
git commit -m "feat(ui): enforce select=one in Book template + session trees"
```

---

## Final Verification

- [ ] **Full UI test + typecheck sweep**

Run: `npm --prefix shirita-ui test 2>&1 | tail -8 && npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: all Vitest suites pass; build succeeds with no type errors.

---

## Self-Review

**Spec coverage (spec §3.1–§3.4):**
- PACK section (identity + tree + variables via `PackEditor`) with pick/create/rename/duplicate/delete — Task 1.
- Template-first order (Template heading + section, then Pack, then Definitions) — Tasks 1 & 2 (Pack inserted after the Template tree, before Definitions).
- Search-box pickers for both Template and Pack — Task 1 (pack) + Task 2 (template), via `EntityPicker`.
- Section color-coding: Template teal, Pack mauve, Definitions neutral (DefinitionEditor's own heading) — Tasks 2 & 1.
- `select=one` mutual exclusion in the Book trees — Task 3 (template + session); the Pack tree already enforces it inside `PackEditor` (Plan 7).
- Definitions section — unchanged `DefinitionEditor`, stays at the bottom.

**Placeholder scan:** none — full code + exact commands throughout. (Task 3 has no new automated test by design: the exclusion logic is unit-tested in Plan 7's `selectOneSiblingsToDisable`, and the two handlers reuse that proven pattern exactly as `PackEditor` does; this step is guarded by typecheck + the existing BookView suite.)

**Type consistency:** `createPack/updatePack/deletePack/duplicatePack` and `library.packs/loadPacks` match Plan 6. `<PackEditor :pack @changed>` matches Plan 7's contract. `EntityPicker` props (`items`/`placeholder`/`createLabel`) + events (`select`/`create`) match Plan 6. `selectOneSiblingsToDisable(nodes, nodeId)` matches Plan 7. `selectPack`/`createPackNamed`/`createTemplateNamed` are defined in the same task that references them in markup.
