# Prompt Tree v2 — Plan 5: Quick visual fixes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the batch of small visual corrections the user called out: nav icons as a uniform grayscale set (no bolding), more prominent section subheadings, auto-fetching the model list (drop the button), and palette-tinted definition type chips.

**Architecture:** Pure frontend polish. Tasks 1–3 are independent of the other plans and can ship first; Task 4 (dynamic tinted type chips) depends on Plan 2's `/api/types` + Plan 3's `library.containerTypes` cache.

**Tech Stack:** Vue 3 `<script setup>` + TS, Tailwind v4 tokens, Vitest + `@vue/test-utils`.

**Spec:** `docs/superpowers/specs/2026-06-13-prompt-tree-worldbook-design.md` §10.

---

## File structure

- `shirita-ui/src/components/AppShell.vue` — grayscale nav icons.
- `shirita-ui/src/components/AppShell.test.ts` — update active-state assertion.
- `shirita-ui/src/views/SettingsView.vue` — prominent subheadings + auto-fetch models.
- `shirita-ui/src/components/DefinitionEditor.vue` — prominent subheading + dynamic tinted type chips.

---

## Task 1: Grayscale nav icons

Currently active icons get `[&_svg]:stroke-[2.5]` + `text-ink` and inactive use `text-mauve/30`. The user wants a **grayscale** set: active darker, inactive lighter, **uniform stroke weight, no bolding**.

**Files:**
- Modify: `shirita-ui/src/components/AppShell.vue`, `shirita-ui/src/components/AppShell.test.ts`

- [ ] **Step 1: Update the test.** In `shirita-ui/src/components/AppShell.test.ts`, the "marks the book section active" test should assert active = `text-ink` and additionally that the inactive links use the muted grayscale class. Replace that test body with:

```ts
  it('marks the book section active in grayscale and others muted', async () => {
    const router = makeRouter()
    router.push('/book')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: [router] } })
    const links = wrapper.findAll('nav a')
    expect(links[1].classes()).toContain('text-ink')        // active book
    expect(links[0].classes()).toContain('text-muted/40')   // inactive chat
    expect(links[2].classes()).toContain('text-muted/40')   // inactive settings
  })
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/AppShell.test.ts`
Expected: FAIL (inactive links currently use `text-mauve/30`).

- [ ] **Step 3: Implement.** In `shirita-ui/src/components/AppShell.vue`, change each of the three nav `<router-link>`s to a uniform grayscale treatment with a constant stroke width. For the chat link:

```html
          <router-link to="/" :class="['transition-colors', section === 'chat' ? 'text-ink' : 'text-muted/40 hover:text-muted/70']">
            <MessageCircle :size="22" :stroke-width="1.8" />
          </router-link>
```
Apply the same pattern to `/book` (`section === 'book'`, `BookOpen`) and `/settings` (`section === 'settings'`, `Settings`): active `text-ink`, inactive `text-muted/40 hover:text-muted/70`, **constant `:stroke-width="1.8"`**, and remove the `[&_svg]:stroke-[2.5]` utility entirely.

- [ ] **Step 4: Run tests, verify pass**

Run: `cd shirita-ui && npx vitest run src/components/AppShell.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/AppShell.vue shirita-ui/src/components/AppShell.test.ts
git commit -m "fix(ui): grayscale nav icons with uniform stroke weight"
```

---

## Task 2: More prominent section subheadings

Subheadings currently render `text-muted` (too faint). Make them darker/heavier across the app.

**Files:**
- Modify: `shirita-ui/src/views/SettingsView.vue`, `shirita-ui/src/components/DefinitionEditor.vue`

- [ ] **Step 1: SettingsView headings.** In `shirita-ui/src/views/SettingsView.vue`, every section heading uses `class="text-[13px] font-semibold text-muted uppercase tracking-wide mb-4"`. Replace `text-muted` with `text-ink/65` in all of them (Provider, Generation, World Info, Appearance, Regex, Language, About). Use a single find-and-replace of `font-semibold text-muted uppercase tracking-wide` → `font-semibold text-ink/65 uppercase tracking-wide` (this string is unique to the section `<h3>`s).

- [ ] **Step 2: DefinitionEditor heading.** In `shirita-ui/src/components/DefinitionEditor.vue`, change the `<h3>` from `text-[11px] font-semibold text-muted uppercase tracking-[0.06em]` to `text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em]`.

- [ ] **Step 3: Visual check (no test).** Run: `cd shirita-ui && npx vue-tsc -b && npx vitest run` → still green (no test asserts the old `text-muted` heading color).

- [ ] **Step 4: Commit**

```bash
git add shirita-ui/src/views/SettingsView.vue shirita-ui/src/components/DefinitionEditor.vue
git commit -m "fix(ui): darker, heavier section subheadings"
```

---

## Task 3: Auto-fetch models (drop the button)

When provider source + base URL + key are present, fetch `/models` automatically and populate the dropdown — no manual button.

**Files:**
- Modify: `shirita-ui/src/views/SettingsView.vue`

- [ ] **Step 1: Add a debounced auto-fetch.** In `SettingsView.vue` `<script setup>`, import `watch`:

```ts
import { ref, computed, onMounted, watch } from 'vue'
```
and after the provider computeds add:

```ts
let modelsTimer: ReturnType<typeof setTimeout> | undefined
watch(
  () => [providerSource.value, providerBaseUrl.value, providerApiKey.value],
  () => {
    clearTimeout(modelsTimer)
    if (!providerBaseUrl.value || !providerApiKey.value) return
    modelsTimer = setTimeout(async () => {
      // persist creds so the server's /models uses them, then fetch.
      await settings.save({
        provider_source: providerSource.value,
        provider_base_url: providerBaseUrl.value,
        provider_api_key: providerApiKey.value,
      })
      await settings.fetchModels()
    }, 800)
  },
)
```

- [ ] **Step 2: Remove the button, keep the dropdown.** In the Model field markup, delete the `<button … @click="settings.fetchModels()">{{ … 'Fetch models' }}</button>` and collapse the `<div class="flex gap-2">` so the input is full-width. Keep the `<p v-if="settings.modelsError">` and the `<select v-if="settings.models.length > 0">`. Add a tiny inline status:

```html
            <div class="flex items-center gap-2">
              <input :value="providerModel" type="text" placeholder="gpt-4o" class="flex-1 border border-line rounded-lg px-3 py-2 text-[14px] outline-none focus:border-primary/50" @input="providerModel = ($event.target as HTMLInputElement).value" />
              <span v-if="settings.modelsLoading" class="text-[12px] text-muted">Fetching…</span>
            </div>
```

- [ ] **Step 3: Build + test.**

Run: `cd shirita-ui && npx vue-tsc -b && npx vitest run`
Expected: green. (No existing test clicks "Fetch models"; if one does, update it to assert the auto-fetch select instead.)

- [ ] **Step 4: Commit**

```bash
git add shirita-ui/src/views/SettingsView.vue
git commit -m "feat(ui): auto-fetch provider models (remove manual button)"
```

---

## Task 4: Dynamic, palette-tinted type chips

> **Depends on:** Plan 2 (`/api/types`) + Plan 3 (`library.containerTypes`). Run this task after those land.

`DefinitionEditor`'s `typeChips` is hard-coded `['char','persona','world','item','prompt']` (includes the removed `item`). Make it the registered container types + `prompt`, each tinted per the palette.

**Files:**
- Modify: `shirita-ui/src/components/DefinitionEditor.vue`, `shirita-ui/src/views/BookView.vue`

- [ ] **Step 1: Write the failing test.** Add to `shirita-ui/src/components/DefinitionEditor.test.ts`:

```ts
it('renders type chips from the provided types plus prompt', () => {
  const types = [
    { id: 'char', label: 'Character', sort: 0, builtin: true, created_at: '' },
    { id: 'world', label: 'World', sort: 1, builtin: true, created_at: '' },
  ]
  const d = { id: 'd', type: 'char', name: 'Neo', content: '', meta: {} }
  const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], types } })
  const chips = w.findAll('[data-test="type-chip"]').map((b) => b.text())
  expect(chips).toEqual(['Character', 'World', 'Prompt'])
})
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/DefinitionEditor.test.ts`
Expected: FAIL (no `types` prop; chips hard-coded + no `data-test`).

- [ ] **Step 3: Implement.** In `shirita-ui/src/components/DefinitionEditor.vue`:
  - Add `types: DefType[]` to props (import `DefType` from `../api/types`).
  - Replace `const typeChips = [...]` with a computed that maps registered types + a synthetic `prompt`:

```ts
const typeChips = computed(() => [
  ...props.types.map((t) => ({ id: t.id, label: t.label })),
  { id: 'prompt', label: 'Prompt' },
])
const chipTint: Record<string, string> = {
  char: 'bg-sky/30 border-sky/40', persona: 'bg-coral/30 border-coral/40',
  world: 'bg-mauve/25 border-mauve/40', prompt: 'bg-line/60 border-line',
}
```
  - Update the chip loop to use the computed + tint + `data-test`:

```html
      <button
        v-for="t in typeChips"
        :key="t.id"
        data-test="type-chip"
        :class="['text-[12px] rounded-full px-3 py-1 border transition-colors',
                 definition.type === t.id ? (chipTint[t.id] || 'bg-line/60 border-line') + ' text-ink'
                                          : 'text-muted border-line hover:text-ink']"
        @click="emit('update:type', t.id)"
      >{{ t.label }}</button>
```

- [ ] **Step 4: Pass types from BookView.** In `shirita-ui/src/views/BookView.vue`, add `:types="library.containerTypes"` to the `<DefinitionEditor>`. (`library.loadTypes()` already runs on mount per Plan 3 Task 6.)

- [ ] **Step 5: Run tests, verify pass**

Run: `cd shirita-ui && npx vitest run src/components/DefinitionEditor.test.ts`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/components/DefinitionEditor.vue shirita-ui/src/views/BookView.vue
git commit -m "feat(ui): dynamic, palette-tinted definition type chips"
```

---

## Task 5: Full verification

- [ ] **Step 1:** `cd shirita-ui && npx vue-tsc -b && npx vitest run && npx vite build` → all green.
- [ ] **Step 2: Manual smoke.** Nav icons read as a calm grayscale set (active dark, others faint, same weight). Subheadings are clearly readable. In Settings, entering a base URL + key auto-populates the model dropdown within ~1s (no button). In Book, the type chips reflect the registered container types + Prompt, tinted by palette.

---

## Self-review checklist

- **Spec coverage (§10):** nav grayscale + uniform stroke + no bold (T1) ✓ · prominent subheadings (T2) ✓ · auto-fetch models, drop button (T3) ✓ · palette-tinted type chips, dynamic from registry (T4; node-icon palette already landed in Plan 3 NodeRow) ✓.
- **Placeholder scan:** concrete class strings + code throughout.
- **Type consistency:** `DefType` prop on DefinitionEditor matches Plan 3's `library.containerTypes`; `chipTint`/`typeTint` keys align with the palette tokens (`sky/coral/mauve`).
- **Dependency:** T1–T3 independent (ship anytime); T4 after Plans 2+3.
- **`v-html`:** none.
