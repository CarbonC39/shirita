# M5 Plan 1b — Dynamic Variables (Frontend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface variables in the UI — a read-only Variables panel (System / Custom) bound to the active branch, `$avatar`/`$background` driving the chat view, displayed message text that hides `<state_update>` tags, and a Book declaration UI for template-level and per-chat variables.

**Architecture:** A new `VariablesPanel` component renders the schema + values returned by `GET …/state`. `ChatView` fetches that state on load / after a send / after a swipe and binds `$avatar`/`$background`. `MessageItem` switches its body to `display_content ?? raw_content` so control tags never render. A reusable `VariablesEditor` in Book edits `template.meta.variables` (global) and `override_config.local_variables` (per-chat) via the M5 1a endpoints.

**Tech Stack:** Vue 3 `<script setup>`, Pinia, Vitest + @vue/test-utils, lucide-vue-next, Tailwind v4. Depends on M5 Plan 1a endpoints (`GET …/state`, `PUT …/local-variables`) and `PUT /api/templates/{id}` accepting `meta`.

**Upstream spec:** `docs/superpowers/specs/2026-06-15-m5-variables-state-design.md`.

---

## File Structure

- `shirita-ui/src/api/types.ts` — **modify**: `VarType`, `VarDecl`, `SessionState`.
- `shirita-ui/src/api/client.ts` — **modify**: `getSessionState`, `setLocalVariables`; extend `updateTemplate` to send `meta`.
- `shirita-ui/src/components/VariablesPanel.vue` — **create**: read-only panel (System/Custom).
- `shirita-ui/src/components/VariablesEditor.vue` — **create**: declare name/type/initial rows.
- `shirita-ui/src/components/MessageItem.vue` — **modify**: render `display_content ?? raw_content`.
- `shirita-ui/src/views/ChatView.vue` — **modify**: fetch state, wire panel, bind `$avatar`/`$background`.
- `shirita-ui/src/views/BookView.vue` — **modify**: Variables declaration (global template + local).
- Tests: `VariablesPanel.test.ts`, `VariablesEditor.test.ts`, `MessageItem.test.ts` (extend), `ChatView.test.ts` (extend).

---

## Task 1: TS types + client functions

**Files:**
- Modify: `shirita-ui/src/api/types.ts`, `shirita-ui/src/api/client.ts`

- [ ] **Step 1: Add the types**

In `shirita-ui/src/api/types.ts`, add:

```ts
export type VarType = 'number' | 'bool' | 'string' | 'list'

export interface VarDecl {
  name: string
  type: VarType
  initial: unknown
  /** 'system' | 'template' | 'local' — for UI grouping. */
  scope?: string
}

export interface SessionState {
  schema: VarDecl[]
  values: Record<string, unknown>
}
```

- [ ] **Step 2: Add the client functions**

In `shirita-ui/src/api/client.ts`, add near the other session functions:

```ts
export function getSessionState(id: string): Promise<SessionState> {
  return apiGet<SessionState>(`/sessions/${id}/state`)
}

export async function setLocalVariables(sessionId: string, variables: VarDecl[]): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/local-variables`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ variables }),
  })
  if (!res.ok) throw new Error(`Set local variables failed: ${res.status}`)
}
```

Extend `updateTemplate` to optionally carry `meta` (it currently sends only `name`). Replace its body with:

```ts
export async function updateTemplate(id: string, name: string, meta?: Record<string, unknown>): Promise<Template> {
  const res = await fetch(`${BASE}/api/templates/${id}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(meta === undefined ? { name } : { name, meta }),
  })
  if (!res.ok) throw new Error(`Update template failed: ${res.status}`)
  return res.json()
}
```

Add `SessionState`, `VarDecl` to the type import at the top of `client.ts` (the `import type { … } from './types'` line).

- [ ] **Step 3: Typecheck + commit**

Run: `cd shirita-ui && npx vue-tsc --noEmit`
Expected: clean.

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/api/client.ts
git commit -m "feat(ui): variable types + getSessionState/setLocalVariables client fns"
```

---

## Task 2: `VariablesPanel` — read-only System/Custom panel

**Files:**
- Create: `shirita-ui/src/components/VariablesPanel.vue`
- Test: `shirita-ui/src/components/VariablesPanel.test.ts`

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/components/VariablesPanel.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import VariablesPanel from './VariablesPanel.vue'
import type { VarDecl } from '../api/types'

const schema: VarDecl[] = [
  { name: '$avatar', type: 'string', initial: '', scope: 'system' },
  { name: 'hp', type: 'number', initial: 100, scope: 'template' },
  { name: 'alarmed', type: 'bool', initial: false, scope: 'template' },
]

describe('VariablesPanel', () => {
  it('renders nothing when the schema is empty', () => {
    const w = mount(VariablesPanel, { props: { schema: [], values: {} } })
    expect(w.find('[data-test="variables-panel"]').exists()).toBe(false)
  })

  it('reveals System and Custom groups on toggle with formatted values', async () => {
    const w = mount(VariablesPanel, { props: { schema, values: { '$avatar': '', hp: 95, alarmed: true } } })
    expect(w.find('[data-test="variables-panel"]').exists()).toBe(true)
    // collapsed by default
    expect(w.find('[data-test="var-system"]').exists()).toBe(false)
    await w.find('[data-test="variables-toggle"]').trigger('click')
    expect(w.find('[data-test="var-system"]').text()).toContain('$avatar')
    const custom = w.find('[data-test="var-custom"]')
    expect(custom.text()).toContain('hp')
    expect(custom.text()).toContain('95')
    expect(custom.text()).toContain('✓') // alarmed=true
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/VariablesPanel.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the component**

Create `shirita-ui/src/components/VariablesPanel.vue`:

```vue
<script setup lang="ts">
import { ref, computed } from 'vue'
import { ChevronDown, ChevronRight } from 'lucide-vue-next'
import type { VarDecl } from '../api/types'

const props = defineProps<{ schema: VarDecl[]; values: Record<string, unknown> }>()
const open = ref(false)

const system = computed(() => props.schema.filter((d) => d.scope === 'system'))
const custom = computed(() => props.schema.filter((d) => d.scope !== 'system'))

function fmt(v: unknown): string {
  if (typeof v === 'boolean') return v ? '✓' : '✗'
  if (Array.isArray(v)) return v.length ? v.join(', ') : '—'
  if (v === undefined || v === null || v === '') return '—'
  return String(v)
}
</script>

<template>
  <div v-if="schema.length" data-test="variables-panel" class="border-t border-line/70 px-5 py-2 text-[13px]">
    <button data-test="variables-toggle" class="flex items-center gap-1 text-muted hover:text-ink" @click="open = !open">
      <component :is="open ? ChevronDown : ChevronRight" :size="14" />
      <span>Variables</span>
    </button>
    <div v-if="open" class="mt-2 space-y-2">
      <div v-if="system.length" data-test="var-system">
        <span class="text-[11px] uppercase tracking-[0.06em] text-muted">System</span>
        <div class="flex flex-wrap gap-x-4 gap-y-1 mt-1">
          <span v-for="d in system" :key="d.name" data-test="var-row" class="tabular-nums">
            <span class="text-muted">{{ d.name }}</span> {{ fmt(values[d.name]) }}
          </span>
        </div>
      </div>
      <div v-if="custom.length" data-test="var-custom">
        <span class="text-[11px] uppercase tracking-[0.06em] text-muted">Custom</span>
        <div class="flex flex-wrap gap-x-4 gap-y-1 mt-1">
          <span v-for="d in custom" :key="d.name" data-test="var-row" class="tabular-nums">
            <span class="text-muted">{{ d.name }}</span> {{ fmt(values[d.name]) }}
          </span>
        </div>
      </div>
    </div>
  </div>
</template>
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/VariablesPanel.test.ts`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/VariablesPanel.vue shirita-ui/src/components/VariablesPanel.test.ts
git commit -m "feat(ui): VariablesPanel — read-only System/Custom variable view"
```

---

## Task 3: Display text hides `<state_update>` tags

**Files:**
- Modify: `shirita-ui/src/components/MessageItem.vue`
- Test: `shirita-ui/src/components/MessageItem.test.ts` (extend)

- [ ] **Step 1: Write the failing test**

Add to `shirita-ui/src/components/MessageItem.test.ts`:

```ts
  it('renders display_content when present (hiding control tags)', () => {
    const wrapper = mount(MessageItem, {
      props: {
        message: makeMsg({ role: 'assistant', raw_content: 'Hit. <state_update action="SUB" key="hp" value="5"/>', display_content: 'Hit.' }),
        style: 'bubble',
      },
    })
    expect(wrapper.text()).toContain('Hit.')
    expect(wrapper.text()).not.toContain('state_update')
  })
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/MessageItem.test.ts -t "display_content"`
Expected: FAIL — the raw text (with the tag) renders.

- [ ] **Step 3: Add a `displayText` computed and use it**

In `shirita-ui/src/components/MessageItem.vue` `<script setup>`, add after the existing computeds:

```ts
const displayText = computed(() => props.message.display_content ?? props.message.raw_content)
```

In the template, replace **both** occurrences of `{{ message.raw_content }}` (bubble and flat bodies) with `{{ displayText }}`. Leave the copy/edit buttons using `message.raw_content` (copying/editing operates on the source).

- [ ] **Step 4: Run to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/MessageItem.test.ts`
Expected: PASS (existing + new).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/MessageItem.vue shirita-ui/src/components/MessageItem.test.ts
git commit -m "feat(ui): render display_content so control tags never show"
```

---

## Task 4: Wire the panel + `$avatar`/`$background` into ChatView

**Files:**
- Modify: `shirita-ui/src/views/ChatView.vue`
- Test: `shirita-ui/src/views/ChatView.test.ts` (extend)

- [ ] **Step 1: Write the failing test**

In `shirita-ui/src/views/ChatView.test.ts`, the `beforeEach` already stubs `getSession`. Add a stub for `getSessionState` there:

```ts
    vi.spyOn(client, 'getSessionState').mockResolvedValue({
      schema: [{ name: 'hp', type: 'number', initial: 100, scope: 'template' }],
      values: { hp: 100 },
    } as never)
```

Then add a test:

```ts
  it('shows the variables panel from session state', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(wrapper.find('[data-test="variables-panel"]').exists()).toBe(true)
  })
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/views/ChatView.test.ts -t "variables panel"`
Expected: FAIL — `getSessionState` is not a mockable export yet / panel not rendered.

- [ ] **Step 3: Fetch state + render the panel + bind system vars**

In `shirita-ui/src/views/ChatView.vue` `<script setup>`, add to the imports:

```ts
import { ref } from 'vue'
import { getSessionState } from '../api/client'
import type { SessionState } from '../api/types'
import VariablesPanel from '../components/VariablesPanel.vue'
```

Add state + loader (place near the other `const`s):

```ts
const sessionState = ref<SessionState>({ schema: [], values: {} })
async function loadState() {
  try {
    sessionState.value = await getSessionState(sessionId)
  } catch {
    sessionState.value = { schema: [], values: {} }
  }
}
const bg = computed(() => {
  const v = sessionState.value.values['$background']
  return typeof v === 'string' && v ? `/assets/${v}` : ''
})
```

Call `loadState()` in `onMounted` (alongside `chat.loadMessages`) and after a send completes. Update `handleSend`, `handleRegenerate`, and `handleSwipe` to refresh state after the store action resolves — change them to:

```ts
async function handleSend(text: string) {
  await chat.send(sessionId, text)
  await loadState()
}
```
```ts
async function handleRegenerate(id: string) {
  await chat.regenerate(sessionId, id)
  await loadState()
}
```
```ts
async function handleSwipe(id: string, delta: -1 | 1) {
  const cur = chat.messages.find((m) => m.id === id)
  if (!cur) return
  const sibs = siblings(chat.messages, cur)
  const i = sibs.findIndex((s) => s.id === id)
  const target = sibs[i + delta]
  if (target) { await chat.switchLeaf(target.id); await loadState() }
}
```

And in `onMounted`:

```ts
onMounted(() => {
  chat.loadMessages(sessionId)
  loadState()
})
```

In the template: add a `:style` background to the outer wrapper and render the panel above the composer. Change the root `<div>` opening to bind the background, and insert the panel before `<Composer …/>`:

```html
  <div
    class="flex flex-col h-full max-w-[600px] mx-auto bg-cover bg-center"
    :style="bg ? { backgroundImage: `url(${bg})` } : {}"
  >
```
```html
    <VariablesPanel :schema="sessionState.schema" :values="sessionState.values" />
    <Composer :disabled="chat.isStreaming" @send="handleSend" />
```

> `$avatar` binding: surface it in the header next to the title. Replace the title span with an avatar + title when set:
> ```html
> <img v-if="sessionState.values['$avatar']" :src="`/assets/${sessionState.values['$avatar']}`" class="w-6 h-6 rounded-full object-cover shrink-0" alt="" />
> <span class="font-semibold text-ink truncate">Chat</span>
> ```

- [ ] **Step 4: Run to verify it passes**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npx vitest run src/views/ChatView.test.ts`
Expected: PASS (existing + new).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/ChatView.vue shirita-ui/src/views/ChatView.test.ts
git commit -m "feat(ui): chat shows variables panel + binds \$avatar/\$background"
```

---

## Task 5: Book — declare variables (template global + per-chat local)

**Files:**
- Create: `shirita-ui/src/components/VariablesEditor.vue`
- Modify: `shirita-ui/src/views/BookView.vue`
- Test: `shirita-ui/src/components/VariablesEditor.test.ts`

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/components/VariablesEditor.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import VariablesEditor from './VariablesEditor.vue'
import type { VarDecl } from '../api/types'

describe('VariablesEditor', () => {
  it('emits update when a row is added', async () => {
    const w = mount(VariablesEditor, { props: { modelValue: [] as VarDecl[] } })
    await w.find('[data-test="add-var"]').trigger('click')
    const ev = w.emitted('update:modelValue')!.at(-1)![0] as VarDecl[]
    expect(ev).toHaveLength(1)
    expect(ev[0].type).toBe('number')
  })

  it('emits update when a row is removed', async () => {
    const w = mount(VariablesEditor, {
      props: { modelValue: [{ name: 'hp', type: 'number', initial: 100 }] as VarDecl[] },
    })
    await w.find('[data-test="remove-var"]').trigger('click')
    const ev = w.emitted('update:modelValue')!.at(-1)![0] as VarDecl[]
    expect(ev).toHaveLength(0)
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/VariablesEditor.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `VariablesEditor`**

Create `shirita-ui/src/components/VariablesEditor.vue`:

```vue
<script setup lang="ts">
import { X, Plus } from 'lucide-vue-next'
import type { VarDecl, VarType } from '../api/types'

const props = defineProps<{ modelValue: VarDecl[] }>()
const emit = defineEmits<{ 'update:modelValue': [v: VarDecl[]] }>()

const types: VarType[] = ['number', 'bool', 'string', 'list']

function emitWith(next: VarDecl[]) { emit('update:modelValue', next) }
function addRow() { emitWith([...props.modelValue, { name: '', type: 'number', initial: 0 }]) }
function removeRow(i: number) { emitWith(props.modelValue.filter((_, idx) => idx !== i)) }
function patch(i: number, p: Partial<VarDecl>) {
  emitWith(props.modelValue.map((d, idx) => (idx === i ? { ...d, ...p } : d)))
}
function defaultInitial(t: VarType): unknown {
  return t === 'number' ? 0 : t === 'bool' ? false : t === 'list' ? [] : ''
}
</script>

<template>
  <div class="space-y-2">
    <div v-for="(d, i) in modelValue" :key="i" class="flex items-center gap-2">
      <input
        :value="d.name" placeholder="name" class="field flex-1 text-[13px]"
        @input="patch(i, { name: ($event.target as HTMLInputElement).value })"
      />
      <select
        :value="d.type" class="field text-[13px]"
        @change="patch(i, { type: ($event.target as HTMLSelectElement).value as VarType, initial: defaultInitial(($event.target as HTMLSelectElement).value as VarType) })"
      >
        <option v-for="t in types" :key="t" :value="t">{{ t }}</option>
      </select>
      <input
        :value="String(d.initial ?? '')" placeholder="initial" class="field w-20 text-[13px]"
        @input="patch(i, { initial: d.type === 'number' ? Number(($event.target as HTMLInputElement).value) || 0 : d.type === 'bool' ? ($event.target as HTMLInputElement).value === 'true' : ($event.target as HTMLInputElement).value })"
      />
      <button data-test="remove-var" class="text-muted hover:text-coral" @click="removeRow(i)"><X :size="14" /></button>
    </div>
    <button data-test="add-var" class="flex items-center gap-1 text-[12px] text-primary hover:text-primary-strong" @click="addRow">
      <Plus :size="13" /> Add variable
    </button>
  </div>
</template>
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/VariablesEditor.test.ts`
Expected: PASS (2 tests).

- [ ] **Step 5: Wire it into BookView (global = template, local = this chat)**

In `shirita-ui/src/views/BookView.vue` `<script setup>`, add imports + handlers:

```ts
import VariablesEditor from '../components/VariablesEditor.vue'
import { setLocalVariables } from '../api/client'
import type { VarDecl } from '../api/types'

// global (template) variables — read from the selected template's meta
const templateVars = computed<VarDecl[]>(() => {
  const t = library.templates.find((x) => x.id === selectedTemplateId.value)
  return ((t?.meta as Record<string, unknown> | undefined)?.variables as VarDecl[]) ?? []
})
async function saveTemplateVars(vars: VarDecl[]) {
  if (!selectedTemplateId.value) return
  const t = library.templates.find((x) => x.id === selectedTemplateId.value)
  const meta = { ...(t?.meta as Record<string, unknown> ?? {}), variables: vars }
  try {
    await updateTemplate(selectedTemplateId.value, templateName.value.trim() || 'Template', meta)
    await library.loadTemplates()
  } catch (e) { error.value = (e as Error).message }
}

// local (this conversation) variables — from override_config.local_variables
const localVars = computed<VarDecl[]>(
  () => ((localSession.value?.override_config as Record<string, unknown> | undefined)?.local_variables as VarDecl[]) ?? [],
)
async function saveLocalVars(vars: VarDecl[]) {
  if (!ui.activeChatId) return
  try {
    await setLocalVariables(ui.activeChatId, vars)
    await loadLocal()
  } catch (e) { error.value = (e as Error).message }
}
```

In the template, add a Variables block to the **global** section (inside `<section data-test="book-global">`, after the `PromptTree`/before the divider) — shown when a template is selected:

```html
            <div v-if="selectedTemplateId" class="mt-4">
              <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2">Variables</h3>
              <VariablesEditor :model-value="templateVars" @update:model-value="saveTemplateVars" />
            </div>
```

And add a Variables block to the **local** section (inside `<section data-test="book-local">`, after the local `PromptTree` block / before the local `DefinitionEditor`):

```html
                <div class="mb-4">
                  <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2">Variables (this chat)</h3>
                  <VariablesEditor :model-value="localVars" @update:model-value="saveLocalVars" />
                </div>
```

- [ ] **Step 6: Typecheck + run the Book tests + commit**

Run: `cd shirita-ui && npx vue-tsc --noEmit && npx vitest run src/views/BookView.test.ts`
Expected: PASS (the existing BookView tests still pass — `templateVars`/`localVars` default to `[]` under the mocked store).

```bash
git add shirita-ui/src/components/VariablesEditor.vue shirita-ui/src/components/VariablesEditor.test.ts shirita-ui/src/views/BookView.vue
git commit -m "feat(ui): Book variable declaration (template global + per-chat local)"
```

---

## Task 6: Manual verification (browser)

- [ ] **Step 1:** Start the stack (`cargo run -p shirita-web` with `TOKEN_SECRET`/`DATABASE_PATH`/`BIND_ADDR`; `cd shirita-ui && npm run dev`). In Book, select a template and declare `hp number 100`.
- [ ] **Step 2:** New chat from that template → open it → the **Variables** panel shows `Custom hp 100` and `System $avatar/$background`.
- [ ] **Step 3:** Drive a reply containing `<state_update action="SUB" key="hp" value="5"/>` (an offline `EchoProvider` build can be primed via a definition/prompt that echoes the tag, or use a real provider). The displayed message hides the tag; the panel shows `hp 95`; regenerate makes a sibling with its own value; swiping between siblings updates the panel.
- [ ] **Step 4:** A reply with `<state_update action="SET" key="$avatar" value="…png"/>` swaps the header avatar; `$background` changes the chat background — per branch.
- [ ] **Step 5:** In Book's 局部 section, add a per-chat variable → it appears in the panel (backfilled to its initial); the template/global library is unchanged.
- [ ] **Step 6:** Run the full suites once more: `cargo test --workspace` and `cd shirita-ui && npx vitest run && npm run build`.

---

## Self-Review Checklist

- **Spec coverage:** read-only panel with System/Custom groups bound to active branch (T2/T4) ✓; `display_content` hides tags (T3) ✓; `$avatar`/`$background` drive the view per branch (T4) ✓; Book declaration for template (global) + per-chat (local) (T5) ✓; refresh on load/send/swipe (T4) ✓; client fns for state/local-vars/template-meta (T1) ✓. Deferred per spec: manual value editing from the panel (not built).
- **Placeholders:** none — every step has concrete code/commands.
- **Type consistency:** `VarDecl`/`VarType`/`SessionState` defined in T1 and used identically in T2/T4/T5; `getSessionState(id) -> Promise<SessionState>`, `setLocalVariables(id, VarDecl[])`, `updateTemplate(id, name, meta?)` match between T1 defs and T4/T5 uses; `VariablesPanel` props `{schema, values}` and `VariablesEditor` `v-model` (`modelValue: VarDecl[]` + `update:modelValue`) match between component defs (T2/T5) and call sites (T4/T5).
- **Open verification points for the implementer:** `ChatView.test.ts` must stub `getSessionState` in `beforeEach` (added in T4) or the existing tests will hit the network; confirm `BookView.test.ts`'s `../api/client` mock includes `setLocalVariables`/`updateTemplate` (the file already mocks the module — add any missing names returning resolved values); the `.field`/`.btn` utility classes used here already exist in the project's CSS (used throughout BookView).
