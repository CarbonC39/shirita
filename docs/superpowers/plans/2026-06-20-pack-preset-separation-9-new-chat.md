# Pack/Preset Separation — Plan 9: Single-screen new chat Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse the two-step new-chat wizard (`NewChatView` + `NewChatPromptView`) into one single-column screen: name + template search-pick + mount-pack chips (removable + reorderable) + optional avatar override → `createSession(name, templateId, avatar, packIds)` → `/chat/:id`.

**Architecture:** Repurpose `NewChatView.vue` into the single screen and delete `NewChatPromptView.vue` (and its `/new/prompt` route). The screen reuses the Plan-6 building blocks — `EntityPicker` for the template and the mount-packs add control — plus the existing `AvatarPicker` for the optional per-chat override. Mounted packs are an ordered id list rendered as removable chips; reorder uses the same native HTML5 drag pattern as `PromptTree` (a `dragId` ref gated by a `[data-test="drag-handle"]` grab — no `dataTransfer`, so it is jsdom-testable). No tree editing on this screen (authoring lives in the Book).

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, Pinia, vue-i18n, vue-router, lucide-vue-next, Vitest, `@vue/test-utils`.

## Global Constraints

- Single column, no view-switching. Section order: **name → Template → Mount packs → avatar → Create**.
- Pickers are the search-box construction (`EntityPicker`), not `<select>`.
- **Mount order is meaningful** (drives identity precedence + assembly order), so chips are both **removable** (× per chip) and **reorderable** (drag handle, `PromptTree`'s native pattern).
- "+ New template" / "+ New pack" in these pickers **route to the Book** (`/book`) — no inline authoring on this screen.
- Optional avatar override → session avatar (Plan-4 fallback). The resolved assistant face comes from the first mounted character pack at chat-render time via Plan-4 `resolve_identity_with_packs`; this screen does not re-derive it.
- Create posts `createSession(name, templateId, avatar, packIds)` with **`packIds` in chip order**.
- i18n keys added to all four locales (`en` source); `parity.test.ts` stays green. English copy; flexible-width. Comments/commits in English.
- Test command (no `cd`): `npm --prefix shirita-ui test -- <pattern>`; build: `npm --prefix shirita-ui run build`.
- Available now: `createSession(name, templateId?, avatar?, packIds=[])` (Plan 6); `library.templates` / `library.packs` / `library.loadAll()` (Plan 6, `loadAll` already loads packs); `EntityPicker` (Plan 6, emits `select[id]` / `create[name]`); `AvatarPicker` (emits `select[path|null]`).

---

## File Structure

- `shirita-ui/src/views/NewChatView.vue` — **rewritten** into the single screen. (Tasks 2, 3)
- `shirita-ui/src/views/NewChatPromptView.vue` — **deleted**. (Task 2)
- `shirita-ui/src/router/index.ts` — drop the `/new/prompt` route. (Task 2)
- `shirita-ui/src/views/NewChatView.test.ts` — **new** test file. (Tasks 2, 3)
- `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` — new-chat labels. (Task 1)

---

### Task 1: i18n for the single new-chat screen

**Files:**
- Modify: `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts`

**Interfaces:**
- Produces: `newChat.{title,namePlaceholder,template,templatePlaceholder,newTemplate,mountPacks,mountPlaceholder,newPack,avatar,create,creating,removePack,reorderPack}`. (`namePlaceholder` already exists; `next`/`skip` are removed — they were only used by the wizard being replaced.)

- [ ] **Step 1: Replace the `newChat` block in `en.ts` (source)**

In `shirita-ui/src/locales/en.ts`, replace the whole existing block

```ts
  newChat: {
    namePlaceholder: 'Name',
    next: 'Next',
    skip: 'Skip',
  },
```

with:

```ts
  newChat: {
    title: 'New chat',
    namePlaceholder: 'Name (optional)',
    template: 'Template',
    templatePlaceholder: 'Pick a template…',
    newTemplate: 'New template (opens Book)',
    mountPacks: 'Mount packs',
    mountPlaceholder: 'Add a pack…',
    newPack: 'New pack (opens Book)',
    avatar: 'Avatar (optional)',
    create: 'Create chat',
    creating: 'Creating…',
    removePack: 'Remove',
    reorderPack: 'Drag to reorder',
  },
```

- [ ] **Step 2: Mirror the block in the three other locales**

`zh-Hans.ts` — replace its `newChat` block with:

```ts
  newChat: {
    title: '新建对话',
    namePlaceholder: '名称（可选）',
    template: '模板',
    templatePlaceholder: '选择模板…',
    newTemplate: '新建模板（前往书）',
    mountPacks: '挂载包',
    mountPlaceholder: '添加包…',
    newPack: '新建包（前往书）',
    avatar: '头像（可选）',
    create: '创建对话',
    creating: '创建中…',
    removePack: '移除',
    reorderPack: '拖动以排序',
  },
```

`zh-Hant.ts`:

```ts
  newChat: {
    title: '新增對話',
    namePlaceholder: '名稱（選填）',
    template: '範本',
    templatePlaceholder: '選擇範本…',
    newTemplate: '新增範本（前往書）',
    mountPacks: '掛載包',
    mountPlaceholder: '新增包…',
    newPack: '新增包（前往書）',
    avatar: '頭像（選填）',
    create: '建立對話',
    creating: '建立中…',
    removePack: '移除',
    reorderPack: '拖曳以排序',
  },
```

`ja.ts`:

```ts
  newChat: {
    title: '新しいチャット',
    namePlaceholder: '名前（任意）',
    template: 'テンプレート',
    templatePlaceholder: 'テンプレートを選択…',
    newTemplate: 'テンプレートを新規作成（ブックを開く）',
    mountPacks: 'パックをマウント',
    mountPlaceholder: 'パックを追加…',
    newPack: 'パックを新規作成（ブックを開く）',
    avatar: 'アバター（任意）',
    create: 'チャットを作成',
    creating: '作成中…',
    removePack: '削除',
    reorderPack: 'ドラッグして並べ替え',
  },
```

- [ ] **Step 3: Run the parity test**

Run: `npm --prefix shirita-ui test -- locales`
Expected: PASS — all four locales share the same key set (the `newChat` block is identical-shaped across locales; `next`/`skip` removed from all four).

- [ ] **Step 4: Commit**

```bash
git add shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts
git commit -m "feat(ui): i18n for single-screen new chat"
```

---

### Task 2: Rewrite NewChatView as the single screen (template + name + avatar + Create), delete the wizard's second view

**Files:**
- Modify: `shirita-ui/src/views/NewChatView.vue` (full rewrite)
- Delete: `shirita-ui/src/views/NewChatPromptView.vue`
- Modify: `shirita-ui/src/router/index.ts`
- Create: `shirita-ui/src/views/NewChatView.test.ts`

**Interfaces:**
- Consumes: `createSession` (Plan 6), `library.templates`/`loadAll` (Plan 6), `EntityPicker` (Plan 6), `AvatarPicker`, `useRouter`.
- Produces: `data-test="new-chat"` root, `data-test="template-picker"` (EntityPicker), `data-test="create-chat"` button. `selectedTemplateId` defaults to the first template on mount. `mountedPackIds` (ordered) is introduced here (empty in this task; populated in Task 3) so the `createSession` call already passes it.

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/views/NewChatView.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'

const { push } = vi.hoisted(() => ({ push: vi.fn() }))
vi.mock('vue-router', () => ({ useRouter: () => ({ push }) }))

vi.mock('../api/client', () => ({
  createSession: vi.fn().mockResolvedValue({ id: 'c9' }),
}))

const templates = [
  { id: 't1', name: 'Default' },
  { id: 't2', name: 'Other' },
]
const packs = [
  { id: 'p1', name: 'Alice', identity: { avatar: '', display_name: '' }, meta: {} },
  { id: 'p2', name: 'Lorebook', identity: { avatar: '', display_name: '' }, meta: {} },
]
vi.mock('../stores/library', () => ({
  useLibraryStore: () => ({ templates, packs, loadAll: vi.fn() }),
}))

import NewChatView from './NewChatView.vue'
import * as api from '../api/client'

describe('NewChatView (single screen)', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    ;(api.createSession as any).mockClear()
    push.mockClear()
  })

  it('renders the screen with a template picker and a create button', async () => {
    const w = mount(NewChatView)
    await flushPromises()
    expect(w.find('[data-test="new-chat"]').exists()).toBe(true)
    expect(w.find('[data-test="template-picker"]').exists()).toBe(true)
    expect(w.find('[data-test="create-chat"]').exists()).toBe(true)
  })

  it('defaults the template to the first one and creates a chat with no packs', async () => {
    const w = mount(NewChatView)
    await flushPromises()
    await w.find('[data-test="chat-name"]').setValue('My chat')
    await w.find('[data-test="create-chat"]').trigger('click')
    await flushPromises()
    expect(api.createSession).toHaveBeenCalledWith('My chat', 't1', null, [])
    expect(push).toHaveBeenCalledWith('/chat/c9')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm --prefix shirita-ui test -- NewChatView`
Expected: FAIL — the old `NewChatView` has no `data-test="new-chat"`/`template-picker`/`create-chat`/`chat-name`, and still pushes to `/new/prompt`.

- [ ] **Step 3: Rewrite `NewChatView.vue`**

Replace the entire contents of `shirita-ui/src/views/NewChatView.vue` with:

```vue
<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { GripVertical, X } from 'lucide-vue-next'
import { useLibraryStore } from '../stores/library'
import { createSession } from '../api/client'
import EntityPicker from '../components/EntityPicker.vue'
import AvatarPicker from '../components/AvatarPicker.vue'

const router = useRouter()
const library = useLibraryStore()

const name = ref('')
const avatar = ref<string | null>(null)
const selectedTemplateId = ref<string | null>(null)
const mountedPackIds = ref<string[]>([])
const creating = ref(false)
const error = ref<string | null>(null)

onMounted(async () => {
  await library.loadAll()
  if (!selectedTemplateId.value) selectedTemplateId.value = library.templates[0]?.id ?? null
})

const selectedTemplateName = computed(
  () => library.templates.find((t) => t.id === selectedTemplateId.value)?.name ?? '',
)
const mountedPacks = computed(() =>
  mountedPackIds.value
    .map((id) => library.packs.find((p) => p.id === id))
    .filter((p): p is NonNullable<typeof p> => !!p),
)

function selectTemplate(id: string) { selectedTemplateId.value = id || null }
function goAuthor() { router.push('/book') }

function addPack(id: string) {
  if (id && !mountedPackIds.value.includes(id)) mountedPackIds.value = [...mountedPackIds.value, id]
}
function removePack(id: string) {
  mountedPackIds.value = mountedPackIds.value.filter((p) => p !== id)
}

// native HTML5 drag-reorder of the chip list, same pattern as PromptTree:
// a drag only counts if it began on a [data-test="drag-handle"] grip.
const dragId = ref<string | null>(null)
const grabbedHandle = ref(false)
function onMouseDown(e: MouseEvent) {
  grabbedHandle.value = !!(e.target as HTMLElement).closest('[data-test="drag-handle"]')
}
function onDragStart(id: string, e: DragEvent) {
  if (!grabbedHandle.value) { e.preventDefault(); return }
  dragId.value = id
}
function onDrop(targetId: string) {
  const src = dragId.value
  dragId.value = null
  grabbedHandle.value = false
  if (!src || src === targetId) return
  const ids = [...mountedPackIds.value]
  const from = ids.indexOf(src)
  const to = ids.indexOf(targetId)
  if (from === -1 || to === -1) return
  ids.splice(to, 0, ids.splice(from, 1)[0])
  mountedPackIds.value = ids
}

async function createChat() {
  creating.value = true
  error.value = null
  // name falls back to the first mounted pack's name, then "Untitled".
  const finalName = name.value.trim() || mountedPacks.value[0]?.name || ''
  try {
    const session = await createSession(
      finalName || 'Untitled',
      selectedTemplateId.value,
      avatar.value,
      mountedPackIds.value,
    )
    router.push(`/chat/${session.id}`)
  } catch (e) {
    error.value = (e as Error).message
  } finally {
    creating.value = false
  }
}
</script>

<template>
  <div data-test="new-chat" class="pt-6 pb-12 flex flex-col gap-5">
    <h2 class="text-lg font-semibold">{{ $t('newChat.title') }}</h2>

    <!-- name -->
    <input
      v-model="name"
      data-test="chat-name"
      type="text"
      :placeholder="$t('newChat.namePlaceholder')"
      class="field w-full"
    />

    <!-- template -->
    <div>
      <label class="text-[13px] text-muted mb-1.5 block">{{ $t('newChat.template') }}</label>
      <EntityPicker
        data-test="template-picker"
        :items="library.templates.map((t) => ({ id: t.id, name: t.name }))"
        :placeholder="$t('newChat.templatePlaceholder')"
        :create-label="$t('newChat.newTemplate')"
        @select="selectTemplate"
        @create="goAuthor"
      />
      <p v-if="selectedTemplateName" class="text-[12.5px] text-muted mt-1.5">{{ selectedTemplateName }}</p>
    </div>

    <!-- mount packs (chips added in Task 3 markup) -->
    <div>
      <label class="text-[13px] text-muted mb-1.5 block">{{ $t('newChat.mountPacks') }}</label>
      <EntityPicker
        data-test="pack-picker"
        :items="library.packs.map((p) => ({ id: p.id, name: p.name }))"
        :placeholder="$t('newChat.mountPlaceholder')"
        :create-label="$t('newChat.newPack')"
        @select="addPack"
        @create="goAuthor"
      />
      <div
        v-if="mountedPacks.length"
        data-test="pack-chips"
        class="flex flex-wrap gap-2 mt-2.5"
        @mousedown="onMouseDown"
      >
        <span
          v-for="p in mountedPacks"
          :key="p.id"
          data-test="pack-chip"
          draggable="true"
          class="inline-flex items-center gap-1.5 pl-1.5 pr-2 py-1 rounded-full bg-card border border-line text-[13px]"
          @dragstart="onDragStart(p.id, $event)"
          @dragover.prevent
          @drop="onDrop(p.id)"
        >
          <span
            data-test="drag-handle"
            class="cursor-grab active:cursor-grabbing text-muted/40 hover:text-muted/70"
            :title="$t('newChat.reorderPack')"
          ><GripVertical :size="13" /></span>
          <span class="text-ink">{{ p.name }}</span>
          <button
            data-test="pack-chip-remove"
            class="text-muted hover:text-coral"
            :title="$t('newChat.removePack')"
            @click="removePack(p.id)"
          ><X :size="13" /></button>
        </span>
      </div>
    </div>

    <!-- avatar override -->
    <div>
      <label class="text-[13px] text-muted mb-2 block">{{ $t('newChat.avatar') }}</label>
      <AvatarPicker @select="avatar = $event" />
    </div>

    <p v-if="error" class="text-coral text-sm">{{ error }}</p>

    <button
      data-test="create-chat"
      :disabled="creating"
      class="w-full py-2.5 rounded-full font-medium bg-primary text-white hover:bg-primary-strong transition-colors disabled:opacity-50"
      @click="createChat"
    >{{ creating ? $t('newChat.creating') : $t('newChat.create') }}</button>
  </div>
</template>
```

(The mount-packs chip markup is included now so the file is written once; Task 3 only adds the chip *behavior* tests. The chip handlers `addPack`/`removePack`/`onMouseDown`/`onDragStart`/`onDrop` are all defined above.)

- [ ] **Step 4: Delete the second wizard view + its route**

Delete the file:

```bash
git rm shirita-ui/src/views/NewChatPromptView.vue
```

In `shirita-ui/src/router/index.ts`, remove the `/new/prompt` route line (the one with `name: 'newPrompt'` / `NewChatPromptView`). Leave the `/new` route (now the single screen) and its crumbs unchanged.

- [ ] **Step 5: Run the test to verify it passes**

Run: `npm --prefix shirita-ui test -- NewChatView`
Expected: PASS — both tests in the new spec (render + default-template-create-no-packs).

- [ ] **Step 6: Typecheck/build (catches the deleted view + dropped route)**

Run: `npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: build clean — no dangling import of `NewChatPromptView`, no unused i18n usage errors.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/views/NewChatView.vue shirita-ui/src/views/NewChatView.test.ts shirita-ui/src/router/index.ts
git rm shirita-ui/src/views/NewChatPromptView.vue
git commit -m "feat(ui): single-screen new chat (template pick + avatar override)"
```

---

### Task 3: Mount-pack chips — add / remove / reorder, posted in chip order

**Files:**
- Modify: `shirita-ui/src/views/NewChatView.test.ts` (add chip tests)

**Interfaces:**
- Consumes: the chip markup + `addPack`/`removePack`/`onDrop` already written in Task 2 (`data-test="pack-picker"`, `pack-chip`, `pack-chip-remove`, `drag-handle`).
- Produces: no new source — this task proves the chip behavior and the chip-order `createSession` payload.

- [ ] **Step 1: Write the failing chip tests**

In `shirita-ui/src/views/NewChatView.test.ts`, add (inside the existing `describe`, after the last test) — note `EntityPicker` is imported to address the two pickers by their `data-test`:

```ts
  it('adds, removes, and reorders mount-pack chips and posts them in order', async () => {
    const EntityPicker = (await import('../components/EntityPicker.vue')).default
    const w = mount(NewChatView)
    await flushPromises()

    const packPicker = w
      .findAllComponents(EntityPicker)
      .find((p) => p.attributes('data-test') === 'pack-picker')!

    // add Alice then Lorebook
    packPicker.vm.$emit('select', 'p1')
    packPicker.vm.$emit('select', 'p2')
    await flushPromises()
    let chips = w.findAll('[data-test="pack-chip"]')
    expect(chips.map((c) => c.text())).toEqual(['Alice', 'Lorebook'])

    // adding a duplicate is a no-op
    packPicker.vm.$emit('select', 'p1')
    await flushPromises()
    expect(w.findAll('[data-test="pack-chip"]').length).toBe(2)

    // reorder: drag Alice (first) onto Lorebook (second) → Lorebook, Alice
    chips = w.findAll('[data-test="pack-chip"]')
    await chips[0].find('[data-test="drag-handle"]').trigger('mousedown')
    await chips[0].trigger('dragstart')
    await chips[1].trigger('drop')
    await flushPromises()
    expect(w.findAll('[data-test="pack-chip"]').map((c) => c.text())).toEqual(['Lorebook', 'Alice'])

    // remove the first chip (now Lorebook)
    await w.findAll('[data-test="pack-chip-remove"]')[0].trigger('click')
    await flushPromises()
    expect(w.findAll('[data-test="pack-chip"]').map((c) => c.text())).toEqual(['Alice'])

    // create posts the surviving pack id
    await w.find('[data-test="create-chat"]').trigger('click')
    await flushPromises()
    expect(api.createSession).toHaveBeenCalledWith('Alice', 't1', null, ['p1'])
  })
```

(The final `createSession` name arg is `'Alice'`: the name input is left blank, so the screen falls back to the first mounted pack's name — `p1` = Alice — proving the blank-name fallback too.)

- [ ] **Step 2: Run the test to verify it passes**

Run: `npm --prefix shirita-ui test -- NewChatView`
Expected: PASS — the chip add/remove/reorder + chip-order payload all hold against the markup written in Task 2.

> If the reorder assertion fails because the `mousedown`→`dragstart`→`drop` sequence didn't register, confirm the chips container carries `@mousedown="onMouseDown"` and each chip carries `draggable="true"` + `@dragstart`/`@drop` (Task 2 markup). Do not "fix" by reaching into component internals — the public DOM events are the contract.

- [ ] **Step 3: Commit**

```bash
git add shirita-ui/src/views/NewChatView.test.ts
git commit -m "test(ui): new-chat mount-pack chips (add/remove/reorder + order)"
```

---

## Final Verification

- [ ] **Full UI test + typecheck sweep**

Run: `npm --prefix shirita-ui test 2>&1 | tail -8 && npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: all Vitest suites pass; build succeeds with no type errors and no reference to the deleted `NewChatPromptView`.

---

## Self-Review

**Spec coverage (spec §4):**
- Single screen replacing the two-view wizard — Task 2 (rewrite `NewChatView`, delete `NewChatPromptView` + `/new/prompt` route).
- Template search-box picker, pre-selected to the first template — Task 2 (`EntityPicker` + `onMounted` default to `library.templates[0]`).
- Mount-packs search-add → ordered, **removable** + **reorderable** chips — Task 2 markup + Task 3 tests (drag uses PromptTree's `dragId`/`grabbedHandle` pattern).
- "+ New pack"/"+ New template" route to the Book — Task 2 (`@create="goAuthor"` → `router.push('/book')`).
- Optional avatar override → session avatar — Task 2 (`AvatarPicker` → `avatar`, passed to `createSession`). Resolved-from-pack face is deferred to Plan-4 backend identity resolution per the Global Constraints note (AvatarPicker has no external value input; a second avatar widget would be redundant).
- Name optional, defaults to the first mounted pack's name — Task 2 (`finalName` fallback) + asserted in Task 3.
- Create → `createSession(name, templateId, avatar, pack_ids)` in chip order → `/chat/:id` — Task 2 (no-packs payload) + Task 3 (chip-order payload).
- No inline tree editing — Task 2 (no `PromptTree`; authoring routes to `/book`).

**Placeholder scan:** none — full file rewrite + complete test code + exact commands. The Task-2 markup deliberately includes the chip block so the view is authored once; Task 3 adds only behavioral tests against it (no source left "TBD").

**Type consistency:** `createSession(name, templateId, avatar, packIds)` matches Plan 6's 4-arg signature (`packIds` defaulted, here always passed). `EntityPicker` `items`/`placeholder`/`createLabel` props + `select[id]`/`create[name]` events match Plan 6. `AvatarPicker` `select[path|null]` matches its contract. `library.templates`/`library.packs`/`loadAll` match Plan 6. The drag handlers (`dragId`, `grabbedHandle`, `onMouseDown`/`onDragStart`/`onDrop`) mirror `PromptTree.vue` exactly. `mountedPackIds` is the single source of chip order, read directly by `createChat`.

**Decision note (deviation from spec, deliberate):** the spec's "resolved avatar preview from the first mounted character pack" is not rendered as a separate widget — the override `AvatarPicker` is the only avatar control, and the assistant face resolves server-side at render time (Plan 4). Rationale: `AvatarPicker` exposes no external value input, so seeding it from a pack would require modifying it or stacking a second avatar circle; the backend already owns identity precedence. Flagged here for the reviewer; reinstating the live preview is a small follow-up if wanted.
