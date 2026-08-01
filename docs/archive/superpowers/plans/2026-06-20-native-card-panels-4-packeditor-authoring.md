# Native Card Panels — Plan 4: PackEditor authoring + live preview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a pack author write its panel — add a **Panel** section to `PackEditor`: an HTML editor, a CSS editor, capability toggles (write / insert / send), and a **live `<PanelView>` preview** bound to the pack's declared variables — persisted to `pack.meta.panel` via the existing `updatePack`.

**Architecture:** The Panel section mirrors how `PackEditor` already handles variables: local editable refs (`panelHtml` / `panelCss` / `panelCaps`) seeded from `pack.meta.panel`, the live preview reuses the real `<PanelView>` (Plan 2/3) so it's WYSIWYG, and any edit calls the existing `save({ meta })` helper which round-trips through `updatePack` + `emit('changed')`. This is the final v1 slice — it closes the loop: author here, render in chat (Plan 3) over the M5 state (Plan 1).

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, vue-i18n, Vitest + `@vue/test-utils`, the Plan-2 `<PanelView>`.

## Global Constraints

- Panel persists to `pack.meta.panel = { html, css, caps }` via the existing `PackEditor.save({ meta })` (keeps `name` + `identity` + the rest of `meta`).
- v1 is self-authored, so the caps toggles are plain on/off (declared == granted) — no consent flow.
- The preview reuses the **same `<PanelView>`** the chat uses (no separate renderer), bound to the pack's variable **initials** as sample values.
- i18n keys in all four locales (`en` source); `parity.test.ts` stays green. English copy; flexible-width.
- Comments/commits in English. Tests: `npm --prefix shirita-ui test -- <pattern>`; build: `npm --prefix shirita-ui run build`.

---

## File Structure

- `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` — `pack.panel…` labels. (Task 1)
- `shirita-ui/src/components/PackEditor.vue` — the Panel section (script + template). (Task 2)
- `shirita-ui/src/components/PackEditor.test.ts` — authoring tests. (Task 2)

---

### Task 1: i18n for the Panel section

**Files:**
- Modify: `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts`

**Interfaces:**
- Produces: `pack.{panel,panelHtml,panelHtmlPlaceholder,panelCss,panelCssPlaceholder,panelCaps,capWrite,capInsert,capSend,panelPreview}`.

- [ ] **Step 1: Add the keys to `en.ts` (source)**

In `shirita-ui/src/locales/en.ts`, inside the `pack: { … }` block, after `variables: 'Variables',`, add:

```ts
    panel: 'Panel',
    panelHtml: 'HTML',
    panelHtmlPlaceholder: '<div>…{{hp}}…</div>',
    panelCss: 'CSS',
    panelCssPlaceholder: '.box { … }',
    panelCaps: 'Allow:',
    capWrite: 'write vars',
    capInsert: 'fill input',
    capSend: 'send',
    panelPreview: 'Preview',
```

- [ ] **Step 2: Mirror in the three other locales**

`zh-Hans.ts` (`pack` block, after `variables: …,`):

```ts
    panel: '面板',
    panelHtml: 'HTML',
    panelHtmlPlaceholder: '<div>…{{hp}}…</div>',
    panelCss: 'CSS',
    panelCssPlaceholder: '.box { … }',
    panelCaps: '允许：',
    capWrite: '改变量',
    capInsert: '填输入框',
    capSend: '发送',
    panelPreview: '预览',
```

`zh-Hant.ts`:

```ts
    panel: '面板',
    panelHtml: 'HTML',
    panelHtmlPlaceholder: '<div>…{{hp}}…</div>',
    panelCss: 'CSS',
    panelCssPlaceholder: '.box { … }',
    panelCaps: '允許：',
    capWrite: '改變數',
    capInsert: '填輸入框',
    capSend: '傳送',
    panelPreview: '預覽',
```

`ja.ts`:

```ts
    panel: 'パネル',
    panelHtml: 'HTML',
    panelHtmlPlaceholder: '<div>…{{hp}}…</div>',
    panelCss: 'CSS',
    panelCssPlaceholder: '.box { … }',
    panelCaps: '許可：',
    capWrite: '変数を変更',
    capInsert: '入力欄に挿入',
    capSend: '送信',
    panelPreview: 'プレビュー',
```

- [ ] **Step 3: Run the parity test**

Run: `npm --prefix shirita-ui test -- locales`
Expected: PASS — the four locales share the same key set.

- [ ] **Step 4: Commit**

```bash
git add shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts
git commit -m "feat(ui): i18n for pack panel authoring section"
```

---

### Task 2: PackEditor Panel section + live preview

**Files:**
- Modify: `shirita-ui/src/components/PackEditor.vue`
- Test: `shirita-ui/src/components/PackEditor.test.ts`

**Interfaces:**
- Consumes: `PanelView` (Plan 2/3), types `Panel` / `PanelCaps` (Plan 2), the existing `save({ meta })` + `packVars`.
- Produces: a `data-test="pack-panel"` section with `data-test="panel-html"` / `panel-css` textareas, `cap-write` / `cap-insert` / `cap-send` checkboxes, and a `<PanelView>` preview; edits persist `pack.meta.panel`.

- [ ] **Step 1: Write the failing tests**

In `shirita-ui/src/components/PackEditor.test.ts`, add inside the `describe('PackEditor', …)` block:

```ts
  it('renders the panel editor seeded from meta.panel with a live preview', async () => {
    const withPanel = { ...pack, meta: { panel: { html: '<span data-bind="hp">x</span>', css: '', caps: {} } } }
    const w = mount(PackEditor, { props: { pack: withPanel }, global: { stubs } })
    await flushPromises()
    expect(w.find('[data-test="pack-panel"]').exists()).toBe(true)
    expect((w.find('[data-test="panel-html"]').element as HTMLTextAreaElement).value).toBe('<span data-bind="hp">x</span>')
    expect(w.find('[data-test="panel-host"]').exists()).toBe(true) // PanelView preview rendered
  })

  it('editing the panel HTML saves meta.panel', async () => {
    const w = mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    const ta = w.find('[data-test="panel-html"]')
    await ta.setValue('<b>{{hp}}</b>')
    await ta.trigger('change')
    await flushPromises()
    expect(api.updatePack).toHaveBeenCalledWith('p1', expect.objectContaining({
      meta: expect.objectContaining({ panel: { html: '<b>{{hp}}</b>', css: '', caps: {} } }),
    }))
  })

  it('toggling a capability saves it', async () => {
    const w = mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    await w.find('[data-test="cap-write"]').setValue(true)
    await flushPromises()
    expect(api.updatePack).toHaveBeenCalledWith('p1', expect.objectContaining({
      meta: expect.objectContaining({ panel: expect.objectContaining({ caps: { write: true } }) }),
    }))
  })
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm --prefix shirita-ui test -- PackEditor`
Expected: FAIL — no `pack-panel` / `panel-html` / `cap-write` in `PackEditor` yet.

- [ ] **Step 3: Add the panel state to `PackEditor.vue` script**

In `shirita-ui/src/components/PackEditor.vue`:

(a) Add the imports — extend the types import and add the component:

```ts
import type { Pack, PromptNode, VarDecl, Trigger, Panel, PanelCaps } from '../api/types'
import PanelView from './PanelView.vue'
```

(b) After `saveVars` (the variables block, ends near line 48), add:

```ts
// ── panel: local editable copy seeded from meta.panel, persisted via save() ──
const panelHtml = ref('')
const panelCss = ref('')
const panelCaps = ref<PanelCaps>({})
watch(
  () => props.pack.id,
  () => {
    const p = (props.pack.meta as { panel?: Panel }).panel
    panelHtml.value = p?.html ?? ''
    panelCss.value = p?.css ?? ''
    panelCaps.value = { ...(p?.caps ?? {}) }
  },
  { immediate: true },
)
function savePanel() {
  void save({
    meta: {
      ...(props.pack.meta as Record<string, unknown>),
      panel: { html: panelHtml.value, css: panelCss.value, caps: panelCaps.value },
    },
  })
}
function toggleCap(cap: 'write' | 'insert' | 'send') {
  panelCaps.value = { ...panelCaps.value, [cap]: !panelCaps.value[cap] }
  savePanel()
}
// Preview binds the pack's declared variables at their initial values.
const previewValues = computed<Record<string, unknown>>(
  () => Object.fromEntries(packVars.value.map((v) => [v.name, v.initial])),
)
```

- [ ] **Step 4: Add the Panel section to the `PackEditor.vue` template**

In `shirita-ui/src/components/PackEditor.vue`, between the variables block (`<VariablesEditor … />`) and the trailing `<p v-if="error" …>`, insert:

```html
    <!-- panel -->
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mt-4 mb-2">{{ $t('pack.panel') }}</h3>
    <div data-test="pack-panel" class="space-y-2">
      <label class="block">
        <span class="text-[12px] text-muted block mb-1">{{ $t('pack.panelHtml') }}</span>
        <textarea
          data-test="panel-html"
          v-model="panelHtml"
          rows="6"
          class="field w-full font-mono text-[12px]"
          :placeholder="$t('pack.panelHtmlPlaceholder')"
          @change="savePanel"
        />
      </label>
      <label class="block">
        <span class="text-[12px] text-muted block mb-1">{{ $t('pack.panelCss') }}</span>
        <textarea
          data-test="panel-css"
          v-model="panelCss"
          rows="5"
          class="field w-full font-mono text-[12px]"
          :placeholder="$t('pack.panelCssPlaceholder')"
          @change="savePanel"
        />
      </label>
      <div class="flex items-center flex-wrap gap-x-4 gap-y-1.5 text-[12px]">
        <span class="text-muted">{{ $t('pack.panelCaps') }}</span>
        <label class="flex items-center gap-1.5"><input type="checkbox" data-test="cap-write" :checked="panelCaps.write" @change="toggleCap('write')" />{{ $t('pack.capWrite') }}</label>
        <label class="flex items-center gap-1.5"><input type="checkbox" data-test="cap-insert" :checked="panelCaps.insert" @change="toggleCap('insert')" />{{ $t('pack.capInsert') }}</label>
        <label class="flex items-center gap-1.5"><input type="checkbox" data-test="cap-send" :checked="panelCaps.send" @change="toggleCap('send')" />{{ $t('pack.capSend') }}</label>
      </div>
      <div>
        <span class="text-[12px] text-muted block mb-1">{{ $t('pack.panelPreview') }}</span>
        <PanelView :html="panelHtml" :css="panelCss" :values="previewValues" />
      </div>
    </div>
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `npm --prefix shirita-ui test -- PackEditor`
Expected: PASS — the three new panel tests plus the pre-existing PackEditor tests (mounting now also renders `<PanelView>`, which doesn't call `updatePack`, so the display-name/identity tests are unaffected).

- [ ] **Step 6: Typecheck/build**

Run: `npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: clean — `Panel`/`PanelCaps`/`PanelView` resolve, no unused symbols.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/components/PackEditor.vue shirita-ui/src/components/PackEditor.test.ts
git commit -m "feat(ui): PackEditor panel authoring section + live preview"
```

---

## Final Verification

- [ ] **Full UI test + build sweep**

Run: `npm --prefix shirita-ui test 2>&1 | tail -8 && npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: all Vitest suites pass; build clean.

- [ ] **Manual smoke (optional, in the running dev app)**

In the Book → a Pack → Panel section, author `<div data-show="poisoned">poisoned</div> HP: {{hp}} <button data-diff-key="hp" data-diff-op="sub" data-diff-value="1">hit</button>` with a declared `hp` number variable and `caps.write` on; mount the pack in a new chat; confirm the panel renders the value and clicking `hit` decrements `hp` (and an opened `<details>` would stay open across the change).

---

## Self-Review

**Spec coverage (spec §7):**
- HTML editor + CSS editor — Task 2 (`panel-html` / `panel-css` textareas).
- Capability toggle row (write / insert / send → `meta.panel.caps`) — Task 2 (`cap-*` checkboxes + `toggleCap`).
- Live `<PanelView>` preview bound to the pack's variables — Task 2 (`<PanelView :values="previewValues">`, initials from `packVars`).
- Saved via `updatePack(id, { meta })` — Task 2 (reuses `save({ meta })`).
- i18n in four locales — Task 1.

**Placeholder scan:** none — full script + template block, complete test code, exact commands.

**Type consistency:** `pack.meta.panel` shape `{ html, css, caps }` matches `Panel` (Plan 2) and what `ChatView` reads (Plan 3). `panelCaps` is `PanelCaps`; `toggleCap` keys (`write`/`insert`/`send`) match `PanelCaps`'s optional fields. `<PanelView>` props `{ html, css, values }` match Plan 2. `save({ meta })` is the existing PackEditor helper. `previewValues` is `Record<string, unknown>` matching `PanelView.values`.

---

## Feature complete (v1)

With Plans 1–4 landed, native card panels v1 is end-to-end: author a Pack panel (Plan 4) → it renders in chat per mounted pack (Plan 3) inside a sanitized, scoped, morphdom Shadow DOM (Plan 2) → declarative `data-diff` actions fold typed diffs into a hidden state-carrier node on the branch (Plan 1). Out of scope and tracked for later: **v2** ST-card → native conversion, **v3** sandboxed JS bridge — plus the separate point 1 (zip export/import) and point 3 (cinema mode).
