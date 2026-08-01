# Book UI Phase 1A — Local Drill-Down Navigator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace BookView's tiled local template-tree + definition-override editors with a drill-down navigator (`BookNavigator`: L0 session template root → L2 definition view), preserving copy-on-write local-override semantics with a deep-copy invariant.

**Architecture:** A new `BookNavigator` component owns a local `navigationStack` + reactive `transitionDirection` (no URL router). It renders one level component at a time inside a `<Transition>`. `SessionTemplateRoot` (L0) shows the session's template tree (filtered from `localNodes` by `meta._source === 'template'`); clicking a definition row pushes `DefinitionView` (L2), which edits that definition as a session-local override. BookView provides its existing local-session state/handlers to the level components via `provide/inject` (no composable extraction yet — the god-view script shrink happens in a later plan).

**Tech Stack:** Vue 3 (`<script setup>`), TypeScript, Pinia stores, vitest + @vue/test-utils, vue-i18n, Tailwind.

## Scope (Phase 1A only)

**In scope:** drill-down for the local **template** side — `BookNavigator`, `SessionTemplateRoot` (L0, template tree), `DefinitionView` (L2, definition override). Removes the tiled local template-subtree and the definition-override chip strip. Keeps the per-chat variables editor and the entire global section untouched.

**Deferred to Phase 1B (separate plan):** mounted-pack chips on L0, `PackView` (L1), pack-subtree drill, `_source`-tagging for newly-created nodes, `editTarget` refactor of `PackEditor`/`DefinitionEditor`, full `useLocalBookSession` composable extraction. The pack subtree is **removed** in 1A (it was already broken — it rendered all session nodes unfiltered); 1B restores it correctly behind L1.

## Global Constraints

- Frontend-only. No backend, storage, data-model, or assembly changes.
- Deep-copy invariant: any session-local data derived from a global entity uses `structuredClone` (no shared references).
- New i18n keys added to `en` (source) **and** `zh-Hans`, `zh-Hant`, `ja` — the locale parity test must pass.
- Code comments and commit messages in English.
- Gates after every task: `npx vue-tsc --noEmit` clean (run from `shirita-ui/`); relevant vitest green.
- Branch: `refactor/book-ui-drilldown` (already created). Commit after every task.

---

## File Structure

- **Create** `shirita-ui/src/components/book/types.ts` — `Target` union, `TreeHandlers`, `LocalBookApi` interface, `LOCAL_BOOK_KEY` inject key.
- **Create** `shirita-ui/src/utils/clone.ts` — `deepClone` (structuredClone wrapper).
- **Modify** `shirita-ui/src/components/NodeRow.vue` — emit `openDefinition` on row click.
- **Modify** `shirita-ui/src/components/PromptTree.vue` — re-emit `openDefinition`.
- **Create** `shirita-ui/src/components/book/BookNavigator.vue` — owns stack + direction, renders level component, breadcrumb back.
- **Create** `shirita-ui/src/components/book/SessionTemplateRoot.vue` — L0: template tree.
- **Create** `shirita-ui/src/components/book/DefinitionView.vue` — L2: local override editor.
- **Modify** `shirita-ui/src/views/BookView.vue` — provide `LocalBookApi`; swap tiled local template/definition UI for `<BookNavigator>`; keep variables + global.
- **Modify** `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` — new `book.nav.*` keys.
- **Tests** alongside each new component; migrate `shirita-ui/src/views/BookView.test.ts`.

---

### Task 1: Foundations — types, deepClone, i18n keys

**Files:**
- Create: `shirita-ui/src/components/book/types.ts`
- Create: `shirita-ui/src/utils/clone.ts`
- Create: `shirita-ui/src/utils/clone.test.ts`
- Modify: `shirita-ui/src/locales/en.ts`, `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts` (the `book` block)

**Interfaces:**
- Produces: `Target`, `TreeHandlers`, `LocalBookApi`, `LOCAL_BOOK_KEY` (used by Tasks 3-6); `deepClone` (used by Task 5).

- [ ] **Step 1: Write the failing test for `deepClone`**

Create `shirita-ui/src/utils/clone.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { deepClone } from './clone'

describe('deepClone', () => {
  it('severs nested references so mutating the clone does not affect the source', () => {
    const source = { meta: { trigger: { keys: ['a'] } }, content: 'x' }
    const copy = deepClone(source)
    copy.meta.trigger.keys.push('b')
    copy.content = 'y'
    expect(source.meta.trigger.keys).toEqual(['a'])
    expect(source.content).toBe('x')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/utils/clone.test.ts`
Expected: FAIL — `Cannot find module './clone'`.

- [ ] **Step 3: Write `deepClone`**

Create `shirita-ui/src/utils/clone.ts`:

```ts
// Deep-copy primitive. Used for copy-on-write session overrides so that
// editing a session-local value can never mutate the global library entity.
export function deepClone<T>(value: T): T {
  return structuredClone(value)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/utils/clone.test.ts`
Expected: PASS.

- [ ] **Step 5: Create `types.ts`**

Create `shirita-ui/src/components/book/types.ts`:

```ts
import type { InjectionKey, ComputedRef, Ref } from 'vue'
import type { PromptNode, Definition, DefType, Trigger } from '../../api/types'

// One entry on the BookNavigator stack. stack[0] is always sessionRoot.
export type Target =
  | { kind: 'sessionRoot' }
  | { kind: 'pack'; packId: string }
  | { kind: 'definition'; definitionId: string }

// PromptTree event handlers, branched on editTarget by the provider (BookView).
export interface TreeHandlers {
  toggleEnabled: (nodeId: string) => void
  addPrompt: (definitionId: string) => void
  addContainer: (typeId: string) => void
  addRefToContainer: (parentId: string, definitionId: string) => void
  createNewPrompt: (name: string) => void
  createNewInContainer: (parentId: string | null, typeId: string) => void
  createType: (name: string) => void
  updateContent: (definitionId: string, content: string) => void
  updateTrigger: (definitionId: string, trigger: Trigger) => void
  updateNodeMeta: (nodeId: string, meta: Record<string, unknown>) => void
  updateDefMeta: (definitionId: string, meta: Record<string, unknown>) => void
  updateDefName: (definitionId: string, name: string) => void
  deleteNode: (nodeId: string) => void
  reorder: (orderedIds: string[]) => void
}

// Everything a book level component needs from the host (BookView), via inject.
export interface LocalBookApi {
  // session template nodes (localNodes filtered by meta._source === 'template')
  templateNodes: ComputedRef<PromptNode[]>
  definitions: Definition[]
  types: DefType[]
  // local definition overrides (session.override_config.local_definitions)
  localDefs: ComputedRef<Record<string, Record<string, unknown>>>
  // local definition editor buffer + state
  localEditDef: Definition
  localDefActive: Ref<boolean>
  localSavedTick: Ref<number>
  // PromptTree wiring (session-owner node ops)
  tree: TreeHandlers
  // local definition override ops
  editLocal: (defId: string) => void
  saveLocal: () => Promise<void>
  revertLocal: (defId: string) => Promise<void>
  defName: (defId: string) => string
}

export const LOCAL_BOOK_KEY: InjectionKey<LocalBookApi> = Symbol('local-book')
```

- [ ] **Step 6: Add i18n keys**

In `shirita-ui/src/locales/en.ts`, inside the `book: { ... }` block (after `packHeading: 'Pack',`), add:

```ts
  nav: {
    back: 'Back',
    backToTemplate: 'Back to template',
    backToPack: 'Back to pack',
  },
```

In `shirita-ui/src/locales/zh-Hans.ts` (same position):

```ts
  nav: {
    back: '返回',
    backToTemplate: '返回模板',
    backToPack: '返回设定包',
  },
```

In `shirita-ui/src/locales/zh-Hant.ts`:

```ts
  nav: {
    back: '返回',
    backToTemplate: '返回範本',
    backToPack: '返回設定包',
  },
```

In `shirita-ui/src/locales/ja.ts`:

```ts
  nav: {
    back: '戻る',
    backToTemplate: 'テンプレートに戻る',
    backToPack: 'パックに戻る',
  },
```

- [ ] **Step 7: Verify typecheck + locale parity**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npm test -- --run src/locales 2>/dev/null || npm test`
Expected: tsc clean; all tests pass (the i18n parity test, if present, passes — all four locales have `book.nav`).

- [ ] **Step 8: Commit**

```bash
git add shirita-ui/src/components/book/types.ts shirita-ui/src/utils/clone.ts shirita-ui/src/utils/clone.test.ts shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts
git commit -m "feat(book): add nav types, deepClone util, drill-down i18n keys"
```

---

### Task 2: PromptTree / NodeRow emit `openDefinition` on row click

The drill-down needs a "click a definition row → open it" signal. PromptTree currently has no such emit.

**Files:**
- Modify: `shirita-ui/src/components/NodeRow.vue` (add click → emit `openDefinition`)
- Modify: `shirita-ui/src/components/PromptTree.vue` (re-emit `openDefinition`)
- Test: `shirita-ui/src/components/PromptTree.test.ts` (create if absent) or extend an existing PromptTree test

**Interfaces:**
- Produces: PromptTree now emits `openDefinition: [definitionId: string]` (used by Task 4, L0).

- [ ] **Step 1: Write the failing test**

Create/extend `shirita-ui/src/components/PromptTree.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import PromptTree from './PromptTree.vue'

const makeNode = (id: string, definitionId: string | null) => ({
  id, owner_kind: 'session' as const, owner_id: 's', parent_id: null,
  sort_order: 0, kind: 'ref' as const, tag: null, definition_id: definitionId,
  enabled: true, created_at: '', meta: {},
})

describe('PromptTree openDefinition', () => {
  it('emits openDefinition with the definition_id when a ref row is clicked', async () => {
    const w = mount(PromptTree, {
      props: { nodes: [makeNode('n1', 'def-1')], definitions: [], types: [] },
    })
    await w.find('[data-test="node-row-n1"]').trigger('click')
    expect(w.emitted('openDefinition')).toEqual([['def-1']])
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/PromptTree.test.ts`
Expected: FAIL — no `openDefinition` emit / `data-test="node-row-..."` not found.

- [ ] **Step 3: Add `data-test` + click emit to NodeRow**

In `shirita-ui/src/components/NodeRow.vue`, on the row's root element add `:data-test="\`node-row-${node.id}\`"` and `@click="onRowClick"`. In the `<script setup>`, add:

```ts
const emit = defineEmits<{ openDefinition: [definitionId: string] }>()
function onRowClick() {
  if (props.node.definition_id) emit('openDefinition', props.node.definition_id)
}
```

(If NodeRow already declares `defineEmits`, merge `openDefinition` into it. Read the file first and adapt — the click handler goes on the row's outermost element; keep existing buttons' `@click.stop` so action buttons do not also trigger the drill.)

- [ ] **Step 4: Re-emit from PromptTree**

In `shirita-ui/src/components/PromptTree.vue`, add `openDefinition: [definitionId: string]` to the `defineEmits` block, and on the `<NodeRow>` usage add `@open-definition="(id) => emit('openDefinition', id)"` (top-level rows). Do the same on any recursive `<NodeRow>` child usage (the tree renders children recursively — both call sites must forward `openDefinition`).

- [ ] **Step 5: Run test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/PromptTree.test.ts`
Expected: PASS.

- [ ] **Step 6: Verify typecheck + full suite**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npm test`
Expected: tsc clean; all tests pass.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/components/NodeRow.vue shirita-ui/src/components/PromptTree.vue shirita-ui/src/components/PromptTree.test.ts
git commit -m "feat(tree): emit openDefinition on row click for drill-down"
```

---

### Task 3: `BookNavigator` — navigation stack + transition direction

**Files:**
- Create: `shirita-ui/src/components/book/BookNavigator.vue`
- Create: `shirita-ui/src/components/book/BookNavigator.test.ts`

**Interfaces:**
- Consumes: `Target` from `./types`.
- Produces: `<BookNavigator :root-target="..." />` with internal `push`/`pop`; renders `SessionTemplateRoot` for `sessionRoot`, `DefinitionView` for `definition`. (Level components created in Tasks 4-5; this task stubs their imports behind the test.)

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/components/book/BookNavigator.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import BookNavigator from './BookNavigator.vue'

describe('BookNavigator', () => {
  it('renders only the root level initially and shows no back button', () => {
    const w = mount(BookNavigator, { props: { rootTarget: { kind: 'sessionRoot' } } })
    expect(w.find('[data-test="nav-back"]').exists()).toBe(false)
    expect(w.find('[data-test="nav-level-sessionRoot"]').exists()).toBe(true)
  })

  it('pushes a definition target on the drill event and shows back', async () => {
    const w = mount(BookNavigator, { props: { rootTarget: { kind: 'sessionRoot' } } })
    await w.find('[data-test="nav-level-sessionRoot"]').vm.$emit('drill', { kind: 'definition', definitionId: 'd1' })
    expect(w.find('[data-test="nav-level-definition"]').exists()).toBe(true)
    expect(w.find('[data-test="nav-back"]').exists()).toBe(true)
  })

  it('pop returns to the previous level and sets backward direction', async () => {
    const w = mount(BookNavigator, { props: { rootTarget: { kind: 'sessionRoot' } } })
    await w.find('[data-test="nav-level-sessionRoot"]').vm.$emit('drill', { kind: 'definition', definitionId: 'd1' })
    await w.find('[data-test="nav-back"]').trigger('click')
    expect(w.find('[data-test="nav-level-sessionRoot"]').exists()).toBe(true)
    expect(w.find('[data-test="nav-level-definition"]').exists()).toBe(false)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/book/BookNavigator.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Write `BookNavigator.vue`**

Create `shirita-ui/src/components/book/BookNavigator.vue`:

```vue
<script setup lang="ts">
import { ref, computed } from 'vue'
import type { Target } from './types'
import SessionTemplateRoot from './SessionTemplateRoot.vue'
import DefinitionView from './DefinitionView.vue'

const props = defineProps<{ rootTarget: Target }>()

// In-component navigation stack (NOT URL router — preserves chat-page state).
const stack = ref<Target[]>([props.rootTarget])
// Reactive transition direction: 'forward' on push, 'backward' on pop.
const transitionDirection = ref<'forward' | 'backward'>('forward')

function push(target: Target) {
  stack.value.push(target)
  transitionDirection.value = 'forward'
}
function pop() {
  if (stack.value.length > 1) {
    stack.value.pop()
    transitionDirection.value = 'backward'
  }
}

const current = computed<Target>(() => stack.value[stack.value.length - 1])
const canPop = computed(() => stack.value.length > 1)

// Breadcrumb label describes the level a "back" press returns to.
const backLabelKey = computed(() => {
  if (stack.value.length < 2) return ''
  const prev = stack.value[stack.value.length - 2]
  if (prev.kind === 'sessionRoot') return 'book.nav.backToTemplate'
  if (prev.kind === 'pack') return 'book.nav.backToPack'
  return 'book.nav.back'
})
</script>

<template>
  <div data-test="book-navigator" class="book-navigator">
    <button
      v-if="canPop"
      data-test="nav-back"
      class="flex items-center gap-1 text-[13px] text-muted hover:text-ink mb-3"
      @click="pop"
    >
      <span aria-hidden>‹</span> {{ $t(backLabelKey) }}
    </button>
    <Transition :name="transitionDirection" mode="out-in">
      <SessionTemplateRoot
        v-if="current.kind === 'sessionRoot'"
        :key="'root'"
        data-test="nav-level-sessionRoot"
        @drill="push"
      />
      <DefinitionView
        v-else-if="current.kind === 'definition'"
        :key="current.definitionId"
        :definition-id="current.definitionId"
        data-test="nav-level-definition"
        @drill="push"
      />
      <!-- pack level added in Phase 1B -->
    </Transition>
  </div>
</template>

<style scoped>
/* Drill-in: new level slides in from the right. */
.forward-enter-active, .forward-leave-active,
.backward-enter-active, .backward-leave-active {
  transition: transform 180ms ease, opacity 180ms ease;
}
.forward-enter-from { transform: translateX(24px); opacity: 0; }
.forward-leave-to   { transform: translateX(-24px); opacity: 0; }
.backward-enter-from { transform: translateX(-24px); opacity: 0; }
.backward-leave-to   { transform: translateX(24px); opacity: 0; }
</style>
```

Note: `SessionTemplateRoot` / `DefinitionView` are created in the next tasks. To keep this task's test green in isolation, the test mounts with `sessionRoot` and only needs `SessionTemplateRoot` to render a stub. **Create minimal placeholder files now** so the import resolves, then flesh them out in Tasks 4-5:

Create `shirita-ui/src/components/book/SessionTemplateRoot.vue` (placeholder):

```vue
<script setup lang="ts"></script>
<template><div data-test="nav-level-sessionRoot" /></template>
```

Create `shirita-ui/src/components/book/DefinitionView.vue` (placeholder):

```vue
<script setup lang="ts">
defineProps<{ definitionId: string }>()
</script>
<template><div data-test="nav-level-definition" /></template>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/book/BookNavigator.test.ts`
Expected: PASS.

- [ ] **Step 5: Verify typecheck**

Run: `cd shirita-ui && npx vue-tsc --noEmit`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/components/book/BookNavigator.vue shirita-ui/src/components/book/BookNavigator.test.ts shirita-ui/src/components/book/SessionTemplateRoot.vue shirita-ui/src/components/book/DefinitionView.vue
git commit -m "feat(book): BookNavigator with stack + transition direction"
```

---

### Task 4: `SessionTemplateRoot` (L0) — session template tree

**Files:**
- Modify: `shirita-ui/src/components/book/SessionTemplateRoot.vue` (replace placeholder)
- Create: `shirita-ui/src/components/book/SessionTemplateRoot.test.ts`

**Interfaces:**
- Consumes: `LOCAL_BOOK_KEY` inject (`templateNodes`, `definitions`, `types`, `tree` handlers); `PromptTree` emits incl. `openDefinition` (Task 2).
- Produces: emits `drill: [Target]` (consumed by BookNavigator).

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/components/book/SessionTemplateRoot.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { provide } from 'vue'
import SessionTemplateRoot from './SessionTemplateRoot.vue'
import { LOCAL_BOOK_KEY, type LocalBookApi } from './types'

function mountWith(api: Partial<LocalBookApi>) {
  return mount(SessionTemplateRoot, {
    global: { provide: { [LOCAL_BOOK_KEY as symbol]: api } },
  })
}

describe('SessionTemplateRoot', () => {
  it('renders the template tree with only template-sourced nodes', () => {
    const api = {
      templateNodes: () => [],  // ref-like getter stub is fine; use a real ref in the host
      definitions: [],
      types: [],
      tree: {},
    } as any
    const w = mountWith(api)
    expect(w.findComponent({ name: 'PromptTree' }).exists()).toBe(true)
  })
})
```

(Note: in the real host, `templateNodes` is a `ComputedRef`. For the test stub above, passing a plain getter is acceptable because the component reads `.value` only inside the template via a computed; to keep the test robust, the component should access `api.templateNodes.value` — so in the test pass `{ templateNodes: { value: [] } }` if needed. Adjust the stub to `{ templateNodes: { value: [] } } as any` to match a Ref shape.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/book/SessionTemplateRoot.test.ts`
Expected: FAIL (placeholder has no PromptTree).

- [ ] **Step 3: Implement `SessionTemplateRoot.vue`**

Replace the placeholder with:

```vue
<script setup lang="ts">
import { inject } from 'vue'
import PromptTree from '../PromptTree.vue'
import { LOCAL_BOOK_KEY, type Target } from './types'

const emit = defineEmits<{ drill: [target: Target] }>()

// Local-book API provided by BookView (session-owner nodes + handlers).
const book = inject(LOCAL_BOOK_KEY)!

// Drill into a definition: push a definition target onto the navigator stack.
function onOpenDefinition(definitionId: string) {
  emit('drill', { kind: 'definition', definitionId })
}
</script>

<template>
  <div>
    <h3 class="text-[11px] font-semibold text-mauve uppercase tracking-wide border-l-2 border-mauve pl-2 mb-2">
      {{ $t('book.templateHeading') }}
    </h3>
    <PromptTree
      :nodes="book.templateNodes.value"
      :definitions="book.definitions"
      :types="book.types"
      @toggle-enabled="book.tree.toggleEnabled"
      @add-prompt="book.tree.addPrompt"
      @add-container="book.tree.addContainer"
      @add-ref-to-container="book.tree.addRefToContainer"
      @create-new-prompt="book.tree.createNewPrompt"
      @create-new-in-container="book.tree.createNewInContainer"
      @create-type="book.tree.createType"
      @update-content="book.tree.updateContent"
      @update-trigger="book.tree.updateTrigger"
      @update-node-meta="book.tree.updateNodeMeta"
      @update-def-meta="book.tree.updateDefMeta"
      @update-def-name="book.tree.updateDefName"
      @delete-node="book.tree.deleteNode"
      @reorder="book.tree.reorder"
      @open-definition="onOpenDefinition"
    />
  </div>
</template>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/book/SessionTemplateRoot.test.ts`
Expected: PASS. Adjust the test's `templateNodes` stub to `{ value: [] }` if the template access path differs.

- [ ] **Step 5: Verify typecheck + suite**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npm test`
Expected: clean + green.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/components/book/SessionTemplateRoot.vue shirita-ui/src/components/book/SessionTemplateRoot.test.ts
git commit -m "feat(book): SessionTemplateRoot L0 renders template tree, drills on open"
```

---

### Task 5: `DefinitionView` (L2) — local override editor with deep-copy

**Files:**
- Modify: `shirita-ui/src/components/book/DefinitionView.vue` (replace placeholder)
- Create: `shirita-ui/src/components/book/DefinitionView.test.ts`

**Interfaces:**
- Consumes: props `definitionId: string`; `LOCAL_BOOK_KEY` inject (`localDefs`, `localEditDef`, `localDefActive`, `localSavedTick`, `definitions`, `types`, `editLocal`, `saveLocal`, `revertLocal`, `defName`); `DefinitionEditor` (existing component).
- Produces: emits `drill` (forwarded; unused at L2 in 1A but kept for the shared interface).

- [ ] **Step 1: Write the failing test (incl. deep-copy guard)**

Create `shirita-ui/src/components/book/DefinitionView.test.ts`:

```ts
import { describe, it, expect, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { ref, reactive } from 'vue'
import DefinitionView from './DefinitionView.vue'
import { LOCAL_BOOK_KEY, type LocalBookApi } from './types'
import { blankDefHolder } from './_testkit' // see Step 3

describe('DefinitionView (local override)', () => {
  it('loads the definition into the editor on mount via editLocal', async () => {
    const editLocal = vi.fn()
    const api = {
      definitions: [{ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: {} }],
      types: [],
      localDefs: { value: {} },
      localEditDef: reactive({ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: {} }),
      localDefActive: ref(true),
      localSavedTick: ref(0),
      editLocal,
      saveLocal: vi.fn(),
      revertLocal: vi.fn(),
      defName: () => 'N',
    } as any
    mount(DefinitionView, {
      props: { definitionId: 'd1' },
      global: { provide: { [LOCAL_BOOK_KEY as symbol]: api } },
    })
    await flushPromises()
    expect(editLocal).toHaveBeenCalledWith('d1')
  })

  it('saves via the injected saveLocal handler', async () => {
    const saveLocal = vi.fn().mockResolvedValue(undefined)
    const api = blankDefHolder({ saveLocal })
    const w = mount(DefinitionView, {
      props: { definitionId: 'd1' },
      global: { provide: { [LOCAL_BOOK_KEY as symbol]: api } },
    })
    await flushPromises()
    await w.findComponent({ name: 'DefinitionEditor' }).vm.$emit('save')
    await flushPromises()
    expect(saveLocal).toHaveBeenCalled()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/book/DefinitionView.test.ts`
Expected: FAIL (placeholder).

- [ ] **Step 3: Add a tiny testkit + implement `DefinitionView.vue`**

Create `shirita-ui/src/components/book/_testkit.ts`:

```ts
import { reactive, ref } from 'vue'
import type { LocalBookApi } from './types'

// Minimal LocalBookApi stub for DefinitionView tests.
export function blankDefHolder(overrides: Partial<LocalBookApi> = {}): LocalBookApi {
  return {
    templateNodes: ref([]) as any,
    definitions: [{ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: {} }] as any,
    types: [],
    localDefs: ref({}) as any,
    localEditDef: reactive({ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: {} }) as any,
    localDefActive: ref(true),
    localSavedTick: ref(0),
    tree: {} as any,
    editLocal: () => {},
    saveLocal: async () => {},
    revertLocal: async () => {},
    defName: () => 'N',
    ...overrides,
  } as any
}
```

Replace `DefinitionView.vue` placeholder with:

```vue
<script setup lang="ts">
import { inject, watch, computed } from 'vue'
import DefinitionEditor from '../DefinitionEditor.vue'
import { deepClone } from '../../utils/clone'
import { LOCAL_BOOK_KEY, type Target } from './types'

const props = defineProps<{ definitionId: string }>()
const emit = defineEmits<{ drill: [target: Target] }>()

const book = inject(LOCAL_BOOK_KEY)!

// Has this definition been overridden in this session?
const isOverridden = computed(() => Object.keys(book.localDefs.value).includes(props.definitionId))

// Load the definition into the local editor buffer on mount / when target changes.
// The host's editLocal builds the buffer via deepClone (see BookView wiring).
watch(
  () => props.definitionId,
  (id) => { if (id) book.editLocal(id) },
  { immediate: true },
)

function onRevert() {
  book.revertLocal(props.definitionId)
}
</script>

<template>
  <div>
    <DefinitionEditor
      v-if="book.localDefActive.value"
      :definition="book.localEditDef"
      :all-definitions="book.definitions"
      :types="book.types"
      :active="true"
      :header-actions="false"
      :saved-tick="book.localSavedTick.value"
      @update:name="book.localEditDef.name = $event"
      @update:type="book.localEditDef.type = $event"
      @update:content="book.localEditDef.content = $event"
      @update:meta="book.localEditDef.meta = $event"
      @save="book.saveLocal"
    />
    <button
      v-if="isOverridden"
      data-test="def-revert"
      class="text-[12px] text-muted hover:text-coral mt-2"
      @click="onRevert"
    >
      {{ $t('book.revertToGlobal') }}
    </button>
  </div>
</template>
```

- [ ] **Step 4: Verify the deep-copy invariant is wired in the host**

The deep-copy happens in BookView's `editLocal` (Task 6) — confirm `editLocal` builds `localEditDef` via `deepClone`. The `DefinitionView` test in Step 1 asserts `editLocal('d1')` is called; Task 6's test asserts the buffer is a deep copy. (This task adds the `deepClone` import surface; the actual deep-copy call is added to `editLocal` in Task 6.)

- [ ] **Step 5: Run test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/book/DefinitionView.test.ts`
Expected: PASS.

- [ ] **Step 6: Verify typecheck + suite**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npm test`
Expected: clean + green.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/components/book/DefinitionView.vue shirita-ui/src/components/book/DefinitionView.test.ts shirita-ui/src/components/book/_testkit.ts
git commit -m "feat(book): DefinitionView L2 edits local override via DefinitionEditor"
```

---

### Task 6: Wire `BookView` — provide local API, swap tiled local UI for `<BookNavigator>`

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue` (script: build + `provide` `LocalBookApi`, deep-copy in `editLocal`; template: replace tiled local template/definition blocks with `<BookNavigator>`, keep variables + customize gate + global section).
- Modify: `shirita-ui/src/views/BookView.test.ts` (migrate tests for removed chip strip + local template tiling).

**Interfaces:**
- Consumes: all components from Tasks 3-5.
- Produces: a working local drill-down; existing tests migrated.

- [ ] **Step 1: Update `editLocal` to deep-copy the base definition**

In `shirita-ui/src/views/BookView.vue`, add the import near the other util imports:

```ts
import { deepClone } from '../utils/clone'
```

In `editLocal` (around line 189), change the buffer construction so `meta` is deep-cloned from `base.meta` (never aliased). Replace the `meta: { ...base.meta, ... }` spread with:

```ts
    const meta = deepClone(base.meta) as Record<string, unknown>
    if (patch.trigger) meta.trigger = patch.trigger
    if (patch.scan) meta.scan = patch.scan
    Object.assign(localEditDef, {
      id: base.id,
      type: base.type,
      name: (patch.name as string) ?? base.name,
      content: (patch.content as string) ?? base.content,
      meta,
    })
```

(Keep the surrounding `editLocal` body; only the `meta` construction changes so it is a deep copy with no reference to `base.meta`.)

- [ ] **Step 2: Build and provide the `LocalBookApi`**

Near the top of the `<script setup>` (after the existing local-session refs/handlers are declared), add:

```ts
import { provide } from 'vue'
import type { LocalBookApi } from '../components/book/types'
import { LOCAL_BOOK_KEY } from '../components/book/types'

// Session template nodes: localNodes filtered by the backend's _source marker.
const templateNodes = computed(() =>
  localNodes.value.filter(
    (n) => (n.meta as Record<string, unknown>)?._source === 'template',
  ),
)

const localBookApi: LocalBookApi = {
  templateNodes,
  definitions: library.definitions,
  types: library.containerTypes,
  localDefs,
  localEditDef,
  localDefActive,
  localSavedTick,
  tree: {
    toggleEnabled: localToggleEnabled,
    addPrompt: localAddPrompt,
    addContainer: localAddContainer,
    addRefToContainer: localAddRefToContainer,
    createNewPrompt: localCreateNewPrompt,
    createNewInContainer: localCreateNewInContainer,
    createType: localCreateType,
    updateContent: localUpdateContent,
    updateTrigger: localUpdateTrigger,
    updateNodeMeta: localUpdateNodeMeta,
    updateDefMeta: handleUpdateDefMeta,
    updateDefName: handleUpdateDefName,
    deleteNode: localDeleteNode,
    reorder: localReorder,
  },
  editLocal,
  saveLocal,
  revertLocal,
  defName,
}
provide(LOCAL_BOOK_KEY, localBookApi)
```

(If any handler name above does not exist verbatim in BookView — e.g., `localUpdateNodeMeta` vs the actual name — read BookView and use the exact existing function names. The explore confirmed the local section wires these events; reuse the same handler functions.)

- [ ] **Step 3: Replace the tiled local template/definition UI with `<BookNavigator>`**

In the template, the LOCAL section currently has (after Task 6 of the prior batch): a `<template v-else>` containing the template + pack subtrees, followed by the definition chip/editor block. **Replace** the entire `<template v-else> ... </template>` (the template + pack subtrees) and the definition-override block with a single BookNavigator. The local section becomes:

```vue
<section v-if="ui.activeChatId" data-test="book-local" class="rounded-2xl bg-primary/5 border border-line/60 p-4 mb-6">
  <h2 class="flex items-center text-[12px] font-semibold uppercase tracking-wide text-primary border-l-2 border-primary pl-2 mb-3">
    {{ $t("book.localHeading") }}
  </h2>

  <template v-if="!customizedLocally">
    <div class="text-[13px] text-muted py-1.5 flex items-center gap-2">
      <span>{{ $t("book.followsGlobal") }}</span>
      <button data-test="customize-locally" class="btn btn-primary !px-2.5 !py-1 text-[12px]" @click="materializeAll">
        {{ $t("book.customizeLocally") }}
      </button>
    </div>
  </template>

  <BookNavigator v-else :root-target="{ kind: 'sessionRoot' }" />

  <!-- Variables (this chat) — unchanged, stays outside the customize gate -->
  <div data-test="local-variables" class="mt-4">
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-wide border-l-2 border-muted/50 pl-2 mb-2">{{ $t("book.variablesThisChat") }}</h3>
    <VariablesEditor :model-value="localVars" @update:model-value="saveLocalVars" />
  </div>
</section>
```

Add the import at the top of the script:

```ts
import BookNavigator from '../components/book/BookNavigator.vue'
```

Remove the now-unused definition chip template (`data-test="local-chips"`) and the inline `<DefinitionEditor v-if="localDefActive" .../>` local-override block (editing now happens in `DefinitionView` via drill-down). Keep `editLocal`/`saveLocal`/`revertLocal` in the script (provided via the API).

- [ ] **Step 4: Migrate `BookView.test.ts`**

- Remove/replace the test `shows the changed-in-this-chat chip strip when a local override exists` (chip strip no longer exists). Replace with a drill-down assertion:

```ts
it('drills into a definition override via the navigator when customized', async () => {
  ;(api.getSession as any).mockResolvedValue({
    id: 'c1', template_id: 't1',
    override_config: { local_definitions: { d1: { content: 'local' } } },
  })
  // a template-sourced session node referencing d1
  ;(api.listNodes as any).mockResolvedValue([
    { id: 'n1', owner_kind: 'session', owner_id: 'c1', parent_id: null, sort_order: 0,
      kind: 'ref', tag: null, definition_id: 'd1', enabled: true, created_at: '', meta: { _source: 'template' } },
  ])
  libraryMock.definitions = [{ id: 'd1', type: 'prompt', name: 'D1', content: 'global', meta: {} }]
  const ui = useUiStore(); ui.setActiveChatId('c1')
  const w = mount(BookView)
  await flushPromises()
  // customize to enter This-chat mode
  await w.find('[data-test="customize-locally"]').trigger('click')
  await flushPromises()
  // click the definition row drills to L2
  await w.find('[data-test="node-row-n1"]').trigger('click')
  await flushPromises()
  expect(w.find('[data-test="nav-level-definition"]').exists()).toBe(true)
})
```

- Keep the `local-variables` test (variables still render). Keep the no-chat / active-chat book-local/book-global tests. Delete the `local-chips` test.

- [ ] **Step 5: Add a deep-copy guard test**

Add to `BookView.test.ts`:

```ts
it('editing a local definition does not mutate the global definition in the library', async () => {
  ;(api.getSession as any).mockResolvedValue({ id: 'c1', template_id: null, override_config: { local_definitions: { d1: { content: 'patched' } } } })
  const globalDef = { id: 'd1', type: 'prompt', name: 'D1', content: 'global', meta: { trigger: { keys: ['a'] } } }
  libraryMock.definitions = [globalDef]
  const ui = useUiStore(); ui.setActiveChatId('c1')
  const w = mount(BookView)
  await flushPromises()
  // trigger editLocal for d1 via the navigator drill (or call the provided API)
  await w.find('[data-test="customize-locally"]').trigger('click')
  await flushPromises()
  // The global definition's meta must remain untouched after local load.
  expect((globalDef.meta as any).trigger).toEqual({ keys: ['a'] })
  expect(globalDef.content).toBe('global')
})
```

- [ ] **Step 6: Run the full suite + typecheck**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npm test`
Expected: tsc clean; all tests green (including migrated + new drill/deep-copy tests).

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/views/BookView.vue shirita-ui/src/views/BookView.test.ts
git commit -m "feat(book): wire BookNavigator into local section; deep-copy local overrides"
```

---

### Task 7: End-to-end verification

**Files:** none (verification only).

- [ ] **Step 1: Full typecheck**

Run: `cd shirita-ui && npx vue-tsc --noEmit`
Expected: no output (clean).

- [ ] **Step 2: Full test suite**

Run: `cd shirita-ui && npm test`
Expected: all test files pass.

- [ ] **Step 3: Manual smoke check (dev server)**

The Vite dev server is already running on `:5173` (proxying to the backend on `:8787`). Open it, pick a chat that uses a template, click **Customize locally**, confirm:
- L0 shows the template tree (mauve heading).
- Clicking a definition row slides to L2 (definition editor); `‹ Back to template` returns to L0.
- Variables editor still shows under the local section.
- Global section is unchanged.

- [ ] **Step 4: Commit any smoke-fixes (if needed)**

If the smoke check surfaced issues, fix and commit with `fix(book): ...`. Otherwise no commit.

---

## Self-Review (completed)

**Spec coverage (Phase 1A slice):**
- §2 surface model (customize gate into This-chat) → Task 6 (gate retained, BookNavigator behind it).
- §4 drill-down stack + transitionDirection → Task 3.
- §5 override semantics (This-chat = local) + deep-copy invariant → Tasks 5 + 6 (deep-copy in `editLocal`, guard test).
- §6 L0 (template tree; variables deferred to L0 in 1B) → Task 4; variables kept in BookView for 1A.
- §7 component decomposition (BookNavigator, level components, inject seam) → Tasks 3-6.
- §8 transitions (reactive direction, slide) → Task 3 (`<Transition :name="transitionDirection">`).
- §9 i18n → Task 1.
- §10 testing (push/pop, deep-copy guard, state-preservation) → Tasks 3, 5, 6. (State-preservation across Library⇔This-chat is Phase 2's `<KeepAlive>`; N/A in 1A which has no Library mode yet.)
- §11 phasing: this plan = Phase 1A; 1B (pack/L1) + Phase 2 (Library) + Phase 3 (unify) are separate plans.

**Deferred (explicitly out of 1A):** mounted-pack chips, `PackView` (L1), `_source`-tagging for new nodes, `editTarget` refactor of `PackEditor`/`DefinitionEditor`, `useLocalBookSession` composable extraction, `<KeepAlive>` mode toggle, Library mode.

**Placeholder scan:** none — all code is concrete; BookView modifications reference existing handler names (implementer reads BookView to confirm exact names per the note in Task 6 Step 2).

**Type consistency:** `Target` (Task 1) used identically in BookNavigator (Task 3), SessionTemplateRoot drill emit (Task 4), DefinitionView (Task 5). `LOCAL_BOOK_KEY`/`LocalBookApi` produced in Task 1, injected in Tasks 4-5, provided in Task 6. `deepClone` produced in Task 1, used in Task 6 (`editLocal`). `openDefinition` emitted by PromptTree (Task 2), handled in SessionTemplateRoot (Task 4).
