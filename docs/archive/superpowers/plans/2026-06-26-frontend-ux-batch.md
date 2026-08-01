# Frontend UX Batch Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 11 frontend bugs/enhancements found during manual testing of the Shirita UI.

**Architecture:** Almost all changes live in the Vue 3 `shirita-ui` app (`<script setup>` + Tailwind v4 + Pinia). One item adds a tiny Rust-core change: treat a `provider_max_tokens` of `0` as "no limit" so the UI can expose an "Unlimited" toggle.

**Tech Stack:** Vue 3, Pinia, vue-i18n v10, Tailwind v4, Vitest + @vue/test-utils (jsdom), lucide-vue-next; Rust core (sqlx/sqlite), `cargo test`.

## Global Constraints

- New i18n keys MUST be added to ALL FOUR locale files: `src/locales/en.ts` (source), `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts`. `src/locales/parity.test.ts` fails if any locale's leaf-key set differs from en.
- Code comments and git commit messages in English.
- Frontend tests run from `shirita-ui/`: `npx vitest run <path>`. The suite auto-installs i18n via `src/test/setup.ts`.
- Rust tests run from repo root: `cargo test -p shirita-core <name>`.
- Follow existing patterns: `field` utility class for inputs, `data-test` hooks for testable elements, Tailwind arbitrary values like `tracking-[0.25em]`.
- These default-value changes affect fresh setups only; existing persisted settings are untouched.

---

### Task 1: Default message style → flat

**Files:**
- Modify: `shirita-ui/src/stores/ui.ts:10-11`
- Test: `shirita-ui/src/stores/ui.test.ts:11-15`

- [ ] **Step 1: Update the failing test**

In `ui.test.ts`, change the existing default test to expect `flat`:

```ts
  it('defaults to flat style and system theme', () => {
    const ui = useUiStore()
    expect(ui.messageStyle).toBe('flat')
    expect(ui.theme).toBe('system')
  })
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/stores/ui.test.ts`
Expected: FAIL — `expected 'bubble' to be 'flat'`.

- [ ] **Step 3: Change the store default**

In `ui.ts`, change the `messageStyle` fallback:

```ts
    messageStyle:
      (localStorage.getItem('ui.messageStyle') as MessageStyle) || 'flat',
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/stores/ui.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/stores/ui.ts shirita-ui/src/stores/ui.test.ts
git commit -m "fix(ui): default message style to flat"
```

---

### Task 2: Generation defaults — temperature 1, max output 8192

**Files:**
- Modify: `shirita-ui/src/views/SettingsView.vue:142` and `:160`
- Test: `shirita-ui/src/views/SettingsView.fixes.test.ts` (create)

**Interfaces:**
- Produces: a shared SettingsView test file later tasks (4, 5, 6) append `describe` blocks to.

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/views/SettingsView.fixes.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import * as client from '../api/client'
import { i18n } from '../i18n'
import SettingsView from './SettingsView.vue'
import SliderControl from '../components/SliderControl.vue'

function mockEmptySettings() {
  vi.spyOn(client, 'getSettings').mockResolvedValue({})
  vi.spyOn(client, 'listDefinitions').mockResolvedValue([])
  vi.spyOn(client, 'getRegexScopes').mockResolvedValue([])
}

describe('SettingsView generation defaults', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
    i18n.global.locale.value = 'en'
  })

  it('defaults temperature to 1 and max response tokens to 8192', async () => {
    mockEmptySettings()
    const w = mount(SettingsView)
    await flushPromises()
    const sliders = w.findAllComponents(SliderControl)
    const temp = sliders.find((s) => s.props('label') === 'Temperature')!
    expect(temp.props('modelValue')).toBe(1)
    const max = sliders.find((s) => s.props('label') === 'Max response tokens')!
    expect(max.props('modelValue')).toBe(8192)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/views/SettingsView.fixes.test.ts`
Expected: FAIL — temp is `0.7`, and there is no "Max response tokens" SliderControl yet (max is currently a plain number input). The temp assertion fails first.

- [ ] **Step 3: Change the temperature default**

In `SettingsView.vue`, line ~142:

```ts
const genTemp = computed({
    get: () => (get("gen_temperature") as number) ?? 1,
    set: (v: number) => set("gen_temperature", v),
});
```

- [ ] **Step 4: Change the max-tokens default**

In `SettingsView.vue`, line ~160:

```ts
const genMaxTokens = computed({
    get: () => (get("provider_max_tokens") as number) ?? 8192,
    set: (v: number) => set("provider_max_tokens", v),
});
```

(The "Max response tokens" slider itself is added in Task 4; this task only fixes the defaults. The max assertion in the test passes once Task 4 lands — for now, keep only the temperature assertion green by splitting the test.)

Replace the test body so this task is independently green — assert temperature here, and leave max for Task 4:

```ts
  it('defaults temperature to 1', async () => {
    mockEmptySettings()
    const w = mount(SettingsView)
    await flushPromises()
    const temp = w.findAllComponents(SliderControl).find((s) => s.props('label') === 'Temperature')!
    expect(temp.props('modelValue')).toBe(1)
  })
```

- [ ] **Step 5: Run test to verify it passes**

Run: `npx vitest run src/views/SettingsView.fixes.test.ts`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/views/SettingsView.vue shirita-ui/src/views/SettingsView.fixes.test.ts
git commit -m "fix(settings): default temperature to 1 and max output to 8192"
```

---

### Task 3: Backend — treat provider_max_tokens 0 as unlimited

**Files:**
- Modify: `shirita-core/src/conversation.rs:70-78`
- Modify: `shirita-core/src/summarize.rs:113-119`
- Test: `shirita-core/src/conversation.rs` (add a `#[tokio::test]` in the existing `mod tests`)

**Interfaces:**
- Produces: settings value `provider_max_tokens == 0` → request `max_tokens: None` (provider default / omitted), consumed by Task 4's "Unlimited" toggle.

- [ ] **Step 1: Write the failing test**

In `conversation.rs`, inside `mod tests`, add (uses the existing `temp_storage()` helper and `Arc<dyn Storage>` pattern):

```rust
    #[tokio::test]
    async fn provider_max_tokens_zero_means_unlimited() {
        let storage: Arc<dyn Storage> = Arc::new(temp_storage().await);
        storage.set_setting("provider_max_tokens", &serde_json::json!(0)).await.unwrap();
        assert_eq!(super::provider_max_tokens(storage.as_ref()).await, None);
        storage.set_setting("provider_max_tokens", &serde_json::json!(4096)).await.unwrap();
        assert_eq!(super::provider_max_tokens(storage.as_ref()).await, Some(4096));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core provider_max_tokens_zero_means_unlimited`
Expected: FAIL — returns `Some(0)`, not `None`.

- [ ] **Step 3: Filter out 0 in conversation.rs**

In `conversation.rs::provider_max_tokens`:

```rust
async fn provider_max_tokens(storage: &dyn Storage) -> Option<u32> {
    storage
        .get_setting("provider_max_tokens")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        .filter(|&n| n > 0)
}
```

- [ ] **Step 4: Filter out 0 in summarize.rs**

In `summarize.rs`, the `max_tokens` let-binding:

```rust
    let max_tokens = storage
        .get_setting("provider_max_tokens")
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        .filter(|&n| n > 0);
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p shirita-core provider_max_tokens_zero_means_unlimited`
Expected: PASS. Then `cargo test -p shirita-core` to confirm no regressions.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/conversation.rs shirita-core/src/summarize.rs
git commit -m "feat(core): treat provider_max_tokens 0 as no output limit"
```

---

### Task 4: Max-output slider + number + Unlimited toggle

**Files:**
- Modify: `shirita-ui/src/views/SettingsView.vue` (script: add `maxTokensUnlimited`; template: replace the max-tokens number row ~648-664)
- Modify: `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` (add `settings.maxTokensUnlimited`)
- Test: `shirita-ui/src/views/SettingsView.fixes.test.ts` (append)

**Interfaces:**
- Consumes: Task 3's `0 → None` backend behaviour.

- [ ] **Step 1: Write the failing test**

Append to `SettingsView.fixes.test.ts`:

```ts
describe('SettingsView max output', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
    i18n.global.locale.value = 'en'
  })

  it('shows a max-tokens slider defaulting to 8192 with Unlimited off', async () => {
    mockEmptySettings()
    const w = mount(SettingsView)
    await flushPromises()
    const max = w.findAllComponents(SliderControl).find((s) => s.props('label') === 'Max response tokens')
    expect(max).toBeTruthy()
    expect(max!.props('modelValue')).toBe(8192)
    expect(w.get('[data-test="max-unlimited"] [data-test="toggle"]').attributes('aria-checked')).toBe('false')
  })

  it('treats stored 0 as Unlimited and hides the slider', async () => {
    vi.spyOn(client, 'getSettings').mockResolvedValue({ provider_max_tokens: 0 })
    vi.spyOn(client, 'listDefinitions').mockResolvedValue([])
    vi.spyOn(client, 'getRegexScopes').mockResolvedValue([])
    const w = mount(SettingsView)
    await flushPromises()
    expect(w.get('[data-test="max-unlimited"] [data-test="toggle"]').attributes('aria-checked')).toBe('true')
    expect(w.findAllComponents(SliderControl).some((s) => s.props('label') === 'Max response tokens')).toBe(false)
  })

  it('toggling Unlimited on stores 0 and hides the slider', async () => {
    mockEmptySettings()
    const w = mount(SettingsView)
    await flushPromises()
    await w.get('[data-test="max-unlimited"] [data-test="toggle"]').trigger('click')
    await flushPromises()
    expect(w.get('[data-test="max-unlimited"] [data-test="toggle"]').attributes('aria-checked')).toBe('true')
    expect(w.findAllComponents(SliderControl).some((s) => s.props('label') === 'Max response tokens')).toBe(false)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/views/SettingsView.fixes.test.ts`
Expected: FAIL — no `[data-test="max-unlimited"]` and no max SliderControl.

- [ ] **Step 3: Add the script logic**

In `SettingsView.vue` `<script setup>`, after `genMaxTokens` (~line 162) add:

```ts
// "Unlimited" output is the sentinel provider_max_tokens === 0 (backend maps it
// to None / no max_tokens). The slider/number are hidden while it's on.
const maxTokensUnlimited = computed(() => (get("provider_max_tokens") as number) === 0);
function setMaxTokensUnlimited(on: boolean) {
    set("provider_max_tokens", on ? 0 : 8192);
}
```

- [ ] **Step 4: Replace the template max-tokens row**

In `SettingsView.vue`, replace the existing max-tokens block (the `<div class="flex items-center justify-between">` with the `Max response tokens` label + number input, ~648-664) with:

```html
                <div data-test="max-unlimited" class="flex items-center justify-between">
                    <span class="text-[14px] text-ink">{{ $t("settings.maxTokensUnlimited") }}</span>
                    <ToggleSwitch
                        :model-value="maxTokensUnlimited"
                        @update:model-value="setMaxTokensUnlimited($event)"
                    />
                </div>
                <SliderControl
                    v-if="!maxTokensUnlimited"
                    v-model="genMaxTokens"
                    :label="$t('settings.maxResponseTokens')"
                    :min="256"
                    :max="32768"
                    :step="256"
                />
```

- [ ] **Step 5: Add the i18n key to all four locales**

In each of `en.ts`, `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts`, inside the `settings` object near `maxResponseTokens`, add:
- en: `maxTokensUnlimited: 'Unlimited output',`
- zh-Hans: `maxTokensUnlimited: '不限制输出',`
- zh-Hant: `maxTokensUnlimited: '不限制輸出',`
- ja: `maxTokensUnlimited: '出力を無制限',`

- [ ] **Step 6: Run tests to verify they pass**

Run: `npx vitest run src/views/SettingsView.fixes.test.ts src/locales/parity.test.ts`
Expected: PASS (both the max-output tests and locale parity).

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/views/SettingsView.vue shirita-ui/src/views/SettingsView.fixes.test.ts shirita-ui/src/locales
git commit -m "feat(settings): max output as slider with Unlimited toggle"
```

---

### Task 5: Remove the Custom CSS hook hint text

**Files:**
- Modify: `shirita-ui/src/views/SettingsView.vue:809` (the custom-css textarea placeholder)
- Test: `shirita-ui/src/views/SettingsView.fixes.test.ts` (append)

- [ ] **Step 1: Write the failing test**

Append to `SettingsView.fixes.test.ts`:

```ts
describe('SettingsView custom css', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
    i18n.global.locale.value = 'en'
  })

  it('no textarea exposes the internal hook selectors as a placeholder', async () => {
    mockEmptySettings()
    const w = mount(SettingsView)
    await flushPromises()
    const leaky = w.findAll('textarea').some((t) => (t.attributes('placeholder') ?? '').includes('hooks:'))
    expect(leaky).toBe(false)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/views/SettingsView.fixes.test.ts`
Expected: FAIL — the placeholder still contains `hooks: .app-...`.

- [ ] **Step 3: Replace the placeholder**

In `SettingsView.vue`, the custom-css `<textarea>` placeholder (~809):

```html
                            placeholder="/* Custom CSS */"
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/views/SettingsView.fixes.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/SettingsView.vue shirita-ui/src/views/SettingsView.fixes.test.ts
git commit -m "fix(settings): drop internal hook selectors from custom CSS hint"
```

---

### Task 6: API-key dots spacing in password mode

**Files:**
- Modify: `shirita-ui/src/views/SettingsView.vue:482-492` (api-key input)
- Test: `shirita-ui/src/views/SettingsView.fixes.test.ts` (append)

- [ ] **Step 1: Write the failing test**

Append to `SettingsView.fixes.test.ts`:

```ts
describe('SettingsView api key field', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
    i18n.global.locale.value = 'en'
  })

  it('widens letter-spacing while the key is masked, not when revealed', async () => {
    mockEmptySettings()
    const w = mount(SettingsView)
    await flushPromises()
    const key = w.get('[data-test="api-key"]')
    expect(key.classes()).toContain('tracking-[0.25em]')
    await w.get('[data-test="api-key-reveal"]').trigger('click')
    expect(w.get('[data-test="api-key"]').classes()).not.toContain('tracking-[0.25em]')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/views/SettingsView.fixes.test.ts`
Expected: FAIL — no `data-test="api-key"` element / no tracking class.

- [ ] **Step 3: Add data-test + conditional spacing**

In `SettingsView.vue`, the api-key `<input>` (~482) — add `data-test`, and a class bound to mask state:

```html
                            <input
                                :value="providerApiKey"
                                :type="showApiKey ? 'text' : 'password'"
                                data-test="api-key"
                                :placeholder="apiKeyOptional ? $t('settings.apiKeyOptional') : ''"
                                class="field w-full pr-9 font-mono"
                                :class="{ 'tracking-[0.25em]': !showApiKey }"
                                @input="
                                    providerApiKey = (
                                        $event.target as HTMLInputElement
                                    ).value
                                "
                            />
```

And add `data-test="api-key-reveal"` to the eye toggle button (~493):

```html
                            <button
                                data-test="api-key-reveal"
                                class="absolute right-2.5 top-2.5 text-muted hover:text-ink"
                                @click="showApiKey = !showApiKey"
                            >
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/views/SettingsView.fixes.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/SettingsView.vue shirita-ui/src/views/SettingsView.fixes.test.ts
git commit -m "fix(settings): space out masked API-key bullets"
```

---

### Task 7: Header brand image

**Files:**
- Modify: `shirita-ui/src/components/AppShell.vue` (script import + brand link)
- Test: `shirita-ui/src/components/AppShell.test.ts` (append)

- [ ] **Step 1: Write the failing test**

Append a test in `AppShell.test.ts` (reuse its `makeRouter`/`plugins` helpers):

```ts
  it('renders the brand mark as an image, not a letter', async () => {
    const router = makeRouter()
    router.push('/')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    const img = wrapper.find('[data-test="brand"] img')
    expect(img.exists()).toBe(true)
    expect(img.attributes('alt')).toBe('Shirita')
  })
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/AppShell.test.ts`
Expected: FAIL — no `[data-test="brand"] img`.

- [ ] **Step 3: Import the logo and swap the brand link**

In `AppShell.vue` `<script setup>`, add an import (top, with other imports):

```ts
import logoUrl from '../assets/favicon.svg'
```

Replace the brand `<router-link>` (the one showing `S`) with:

```html
            <router-link
              to="/"
              data-test="brand"
              class="w-7 h-7 rounded-lg overflow-hidden grid place-items-center shrink-0"
            >
              <img :src="logoUrl" alt="Shirita" class="w-7 h-7 object-cover" />
            </router-link>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/AppShell.test.ts`
Expected: PASS (existing `findAll('nav a')` length-3 test still passes — the brand link is outside `<nav>`).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/AppShell.vue shirita-ui/src/components/AppShell.test.ts
git commit -m "fix(shell): use the logo image as the header brand mark"
```

---

### Task 8: /new button glow

**Files:**
- Modify: `shirita-ui/src/views/HomeView.vue` (new-chat `<router-link>` SVG + a scoped glow style)

This is purely visual; verify by build + the Playwright pass in Task 15.

- [ ] **Step 1: Add a glowing, gently pulsing filter to the SVG**

In `HomeView.vue`, give the new-chat `<svg>` a class and add a scoped style. Replace the inline `style` `filter` with a class:

```html
                <svg
                    class="new-glow"
                    width="54"
                    height="54"
                    viewBox="0 0 24 24"
                    style="transform: scaleX(-1)"
                >
```

Add a `<style scoped>` block at the end of the file:

```html
<style scoped>
.new-glow {
  filter: drop-shadow(0 7px 16px rgba(0, 0, 0, 0.18))
    drop-shadow(0 0 6px color-mix(in srgb, var(--color-primary) 70%, transparent));
  animation: new-pulse 2.4s ease-in-out infinite;
}
@keyframes new-pulse {
  0%, 100% {
    filter: drop-shadow(0 7px 16px rgba(0, 0, 0, 0.18))
      drop-shadow(0 0 5px color-mix(in srgb, var(--color-primary) 55%, transparent));
  }
  50% {
    filter: drop-shadow(0 7px 18px rgba(0, 0, 0, 0.2))
      drop-shadow(0 0 12px color-mix(in srgb, var(--color-primary) 85%, transparent));
  }
}
@media (prefers-reduced-motion: reduce) {
  .new-glow { animation: none; }
}
</style>
```

- [ ] **Step 2: Verify the build compiles**

Run: `npx vue-tsc -b --noEmit` (or `npm run build`) from `shirita-ui/`
Expected: no type/template errors.

- [ ] **Step 3: Commit**

```bash
git add shirita-ui/src/views/HomeView.vue
git commit -m "feat(home): add a primary-color glow to the new-chat button"
```

---

### Task 9: Mobile — Book node delete button tappable on touch

**Files:**
- Modify: `shirita-ui/src/components/NodeRow.vue:171-177` (delete button classes)
- Test: `shirita-ui/src/components/NodeRow.test.ts` (append)

- [ ] **Step 1: Write the failing test**

Append to `NodeRow.test.ts`:

```ts
  it('keeps the delete button visible (not opacity-0) for touch devices', () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 0, isExpanded: false } })
    const del = w.get('[data-test="node-delete"]')
    expect(del.classes()).not.toContain('text-muted/0')
    expect(del.classes()).toContain('text-muted/40')
  })
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/NodeRow.test.ts`
Expected: FAIL — class is `text-muted/0`.

- [ ] **Step 3: Make the delete button faintly visible by default**

In `NodeRow.vue`, the delete button class:

```html
        class="text-muted/40 group-hover:text-muted/70 hover:!text-coral shrink-0 p-0.5 transition-colors"
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/NodeRow.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/NodeRow.vue shirita-ui/src/components/NodeRow.test.ts
git commit -m "fix(book): keep the node delete button tappable on touch devices"
```

---

### Task 10: Mobile — Book toolbars wrap instead of overflowing

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue` (template toolbar ~918, pack toolbar ~1055)

Visual; the actual overflowing rows are confirmed at 375px in Task 15. The change is to let the icon-button group wrap below the picker.

- [ ] **Step 1: Allow the template toolbar to wrap**

In `BookView.vue`, the template toolbar wrapper (`<div class="flex items-center gap-2">` directly under `template-picker`, ~918) becomes:

```html
            <div class="flex items-center gap-2 flex-wrap">
                <EntityPicker
                    class="flex-1 min-w-[180px]"
                    data-test="template-picker"
```

(Keep the rest of the EntityPicker props unchanged; only the wrapper gains `flex-wrap` and the picker gains `min-w-[180px]` so the 5-button group drops to the next line on narrow screens.)

- [ ] **Step 2: Allow the pack toolbar to wrap**

In `BookView.vue`, the pack toolbar wrapper (`<div class="flex items-center gap-2 mb-3">`, ~1055) becomes:

```html
                <div class="flex items-center gap-2 mb-3 flex-wrap">
```

And the pack `EntityPicker` / rename input `class="flex-1"` becomes `class="flex-1 min-w-[180px]"` (both the `v-if="renamingPack"` input and the `v-else` EntityPicker).

- [ ] **Step 3: Verify the build compiles**

Run: `npx vue-tsc -b --noEmit` from `shirita-ui/`
Expected: no errors. Run `npx vitest run src/views/BookView.test.ts` to confirm no regressions.

- [ ] **Step 4: Commit**

```bash
git add shirita-ui/src/views/BookView.vue
git commit -m "fix(book): wrap toolbar action buttons on narrow viewports"
```

---

### Task 11: Scan-depth row only for keyword triggers

**Files:**
- Modify: `shirita-ui/src/components/DefinitionEditor.vue:39-45` (add a trigger-mode computed) and `:268-282` (gate scan depth + recursive)
- Test: `shirita-ui/src/components/DefinitionEditor.test.ts` (append)

- [ ] **Step 1: Write the failing test**

Append to `DefinitionEditor.test.ts`:

```ts
describe('DefinitionEditor scan settings gating', () => {
  it('shows scan depth only for keyword triggers', () => {
    const kw = { id: 'd', type: 'world', name: 'Z', content: '', meta: { trigger: { mode: 'keyword', keys: ['z'], probability: 100 } } }
    const w = mount(DefinitionEditor, { props: { definition: kw, allDefinitions: [kw], active: true }, ...plugins })
    expect(w.find('[data-test="scan-depth"]').exists()).toBe(true)
  })

  it('hides scan depth for constant (always-on) triggers', () => {
    const constant = { id: 'd', type: 'world', name: 'Z', content: '', meta: { trigger: { mode: 'constant', keys: [], probability: 100 } } }
    const w = mount(DefinitionEditor, { props: { definition: constant, allDefinitions: [constant], active: true }, ...plugins })
    expect(w.find('[data-test="scan-depth"]').exists()).toBe(false)
    expect(w.find('[data-test="trigger-editor"]').exists()).toBe(true)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/DefinitionEditor.test.ts`
Expected: FAIL — `scan-depth` renders for the constant trigger too.

- [ ] **Step 3: Add a trigger-mode computed**

In `DefinitionEditor.vue` `<script setup>`, after the `scan` computed (~45), add:

```ts
// Scan depth/recursion only matter for keyword (world-info scan) triggers; a
// constant ("always on") or random insert never scans, so hide those controls.
const triggerMode = computed(() => triggerFromMeta(props.definition.meta).mode)
```

(`triggerFromMeta` is already imported at the top of the file.)

- [ ] **Step 4: Gate the scan depth + recursive controls**

In `DefinitionEditor.vue`, wrap the scan-depth + recursive labels (the two `<label>`s for `scanDepth` and `recursive` inside the `isContainerType` block, ~269-282) so they only render for keyword mode, leaving `wrap_in_tag` always visible:

```html
      <div class="flex items-center gap-4 flex-wrap">
        <template v-if="triggerMode === 'keyword'">
          <label class="flex items-center gap-2 text-[13px] text-ink">
            {{ $t('definition.scanDepth') }}
            <input
              data-test="scan-depth"
              :value="scan.depth"
              type="number" min="1" max="20"
              class="field !py-1 w-[64px] text-right tabular-nums"
              @input="updateScan({ depth: parseInt(($event.target as HTMLInputElement).value) || 1 })"
            />
          </label>
          <label class="flex items-center gap-2 text-[13px] text-ink">
            {{ $t('definition.recursive') }}
            <ToggleSwitch :model-value="scan.recursive" @update:model-value="updateScan({ recursive: $event })" />
          </label>
        </template>
        <label v-if="showWrapInTag" class="flex items-center gap-2 text-[13px] text-ink" :title="$t('definition.wrapInTagHint')">
          {{ $t('definition.wrapInTag') }}
          <ToggleSwitch
            data-test="wrap-in-tag"
            :model-value="(definition.meta as Record<string, unknown>).wrap_in_tag === true"
            @update:model-value="emit('update:meta', { ...definition.meta, wrap_in_tag: $event })"
          />
        </label>
      </div>
```

- [ ] **Step 5: Run test to verify it passes**

Run: `npx vitest run src/components/DefinitionEditor.test.ts`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/components/DefinitionEditor.vue shirita-ui/src/components/DefinitionEditor.test.ts
git commit -m "fix(definition): show scan depth only for keyword triggers"
```

---

### Task 12: Book remembers last-edited template/pack/definition

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue` (persist in `selectTemplate`, `selectPack`, `selectDefinition`; restore in `onMounted`)
- Test: `shirita-ui/src/views/BookView.test.ts` (append)

**Interfaces:**
- Produces: localStorage keys `book.templateId`, `book.packId`, `book.defId`.

- [ ] **Step 1: Write the failing test**

Append to `BookView.test.ts` (this file already mocks `../api/client` and `../stores/library` — see its top). Add a test that a remembered template id is reselected on mount:

```ts
describe('BookView remembers selection', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    libraryMock.templates = [{ id: 't1', name: 'One', meta: {} }, { id: 't2', name: 'Two', meta: {} }]
    ;(api.getSession as any).mockResolvedValue({ id: 'c1', template_id: null, override_config: {} })
    ;(api.listNodes as any).mockClear()
  })

  it('restores the last-edited template from localStorage', async () => {
    localStorage.setItem('book.templateId', 't2')
    const ui = useUiStore(); ui.setActiveChatId(null)
    mount(BookView)
    await flushPromises()
    expect(api.listNodes).toHaveBeenCalledWith('template', 't2')
  })

  it('falls back to the first template when the saved id is gone', async () => {
    localStorage.setItem('book.templateId', 'deleted')
    const ui = useUiStore(); ui.setActiveChatId(null)
    mount(BookView)
    await flushPromises()
    expect(api.listNodes).toHaveBeenCalledWith('template', 't1')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/views/BookView.test.ts`
Expected: FAIL — on mount it always selects `templates[0]` (`t1`), ignoring the saved id.

- [ ] **Step 3: Persist selections**

In `BookView.vue`, add persistence to the three select functions.

`selectTemplate` (~444) — after setting `selectedTemplateId.value`:

```ts
async function selectTemplate(id: string) {
    selectedTemplateId.value = id || null;
    try { localStorage.setItem("book.templateId", id || "") } catch { /* ignore */ }
    templateName.value = library.templates.find((t) => t.id === id)?.name ?? "";
    if (id) {
        try { nodes.value = await listNodes("template", id); } catch { nodes.value = []; }
    } else {
        nodes.value = [];
    }
}
```

`selectPack` (~714):

```ts
function selectPack(id: string) {
    selectedPackId.value = id || null;
    try { localStorage.setItem("book.packId", id || "") } catch { /* ignore */ }
}
```

`selectDefinition` (~762) — persist after a definition is chosen (record `id` for both new and existing):

```ts
function selectDefinition(id: string) {
    try { localStorage.setItem("book.defId", id || "") } catch { /* ignore */ }
    if (!id) {
        loadDef(blankDef());
        defActive.value = true;
        return;
    }
    const found = library.definitions.find((d) => d.id === id);
    if (found) {
        loadDef(found);
        defActive.value = true;
    }
}
```

- [ ] **Step 4: Restore on mount**

In `BookView.vue` `onMounted` (~432), replace the "Auto-select the first template" block with restore-then-fallback logic, and also restore pack + definition:

```ts
        // Restore the last-edited template/pack/definition (Book remounts on
        // navigation). Fall back to the default template, then the first.
        const savedTemplate = localStorage.getItem("book.templateId");
        const defaultTemplate = library.templates.find((t) => (t.meta as Record<string, unknown>)?.default)?.id;
        const templateToSelect =
            (savedTemplate && library.templates.some((t) => t.id === savedTemplate) ? savedTemplate : null) ??
            defaultTemplate ??
            library.templates[0]?.id ??
            null;
        if (templateToSelect) await selectTemplate(templateToSelect);

        const savedPack = localStorage.getItem("book.packId");
        if (savedPack && library.packs.some((p) => p.id === savedPack)) selectedPackId.value = savedPack;

        const savedDef = localStorage.getItem("book.defId");
        if (savedDef && library.definitions.some((d) => d.id === savedDef)) selectDefinition(savedDef);
```

(Remove the old `if (!selectedTemplateId.value && library.templates.length > 0) { await selectTemplate(library.templates[0].id); }` block.)

- [ ] **Step 5: Run test to verify it passes**

Run: `npx vitest run src/views/BookView.test.ts`
Expected: PASS (both new tests and the existing BookView tests).

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/views/BookView.vue shirita-ui/src/views/BookView.test.ts
git commit -m "feat(book): remember last-edited template/pack/definition across navigation"
```

---

### Task 13: Template default + auto-select on new chat

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue` (default star button + `setDefaultTemplate`)
- Modify: `shirita-ui/src/views/NewChatView.vue:20-23` (prefer default template)
- Modify: `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` (add `book.defaultTemplate`)
- Test: `shirita-ui/src/views/NewChatView.test.ts` (append)

**Interfaces:**
- Consumes: `updateTemplate(id, name, meta)` (already supports meta).
- Produces: `template.meta.default === true` marks the default; at most one.

- [ ] **Step 1: Write the failing test**

Append to `NewChatView.test.ts`. First, give one template a default flag in that file's `templates` mock — add a new test that uses its own library mock override is not possible (the mock is module-level), so instead extend the existing `templates` array with meta and assert. Replace the module-level `templates` with:

```ts
const templates = [
  { id: 't1', name: 'Default', meta: {} },
  { id: 't2', name: 'Other', meta: { default: true } },
]
```

Then append:

```ts
  it('auto-selects the template flagged default', async () => {
    const w = mount(NewChatView)
    await flushPromises()
    await w.find('[data-test="chat-name"]').setValue('Hi')
    await w.find('[data-test="create-chat"]').trigger('click')
    await flushPromises()
    expect(api.createSession).toHaveBeenCalledWith('Hi', 't2', null, [])
  })
```

Note: the existing tests assert `createSession` with `'t1'`. Because `t2` is now default, update those two existing assertions (`'My chat', 't1'` → `'t2'`, and `'Alice', 't1'` → `'t2'`) to keep them green, OR set `t1` as the default instead. To minimize churn, set the FIRST template default instead:

```ts
const templates = [
  { id: 't1', name: 'Default', meta: { default: true } },
  { id: 't2', name: 'Other', meta: {} },
]
```

and assert the new test expects `'t1'` (existing tests already expect `t1`, so they stay green):

```ts
  it('auto-selects the template flagged default', async () => {
    const w = mount(NewChatView)
    await flushPromises()
    await w.find('[data-test="chat-name"]').setValue('Hi')
    await w.find('[data-test="create-chat"]').trigger('click')
    await flushPromises()
    expect(api.createSession).toHaveBeenCalledWith('Hi', 't1', null, [])
  })
```

- [ ] **Step 2: Run test to verify it fails (or proves the path)**

Run: `npx vitest run src/views/NewChatView.test.ts`
Expected: With current code (`templates[0]`), the new test passes by coincidence (t1 is first). To make the test meaningful, temporarily reorder the mock to put the default second and expect `t2` — confirm it FAILS, then restore to default-first. (This proves the selection follows `meta.default`, not array order.)

- [ ] **Step 3: Prefer the default template in NewChatView**

In `NewChatView.vue` `onMounted` (~20):

```ts
onMounted(async () => {
  await library.loadAll()
  if (!selectedTemplateId.value) {
    const def = library.templates.find((t) => (t.meta as Record<string, unknown>)?.default)
    selectedTemplateId.value = def?.id ?? library.templates[0]?.id ?? null
  }
})
```

- [ ] **Step 4: Add the default-star button + handler in BookView**

In `BookView.vue`, import `Star` from lucide (extend the existing import on line 4):

```ts
import { Check, Pencil, Upload, Download, Copy, Trash2, Star } from "lucide-vue-next";
```

Add a computed + handler (near the template functions, ~456):

```ts
const isDefaultTemplate = computed(() => {
    const t = library.templates.find((x) => x.id === selectedTemplateId.value);
    return (t?.meta as Record<string, unknown> | undefined)?.default === true;
});
async function toggleDefaultTemplate() {
    const id = selectedTemplateId.value;
    if (!id) return;
    const turningOn = !isDefaultTemplate.value;
    try {
        // Clear any other default first so at most one template is default.
        for (const t of library.templates) {
            if ((t.meta as Record<string, unknown>)?.default && t.id !== id) {
                await updateTemplate(t.id, t.name, { ...t.meta, default: false });
            }
        }
        const cur = library.templates.find((t) => t.id === id);
        await updateTemplate(id, cur?.name ?? templateName.value, { ...(cur?.meta ?? {}), default: turningOn });
        await library.loadTemplates();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
```

In the template toolbar button group (~928, alongside rename/import/export/dup/delete), add a star button as the first button:

```html
                    <button
                        data-test="template-default"
                        class="w-[33px] h-[33px] grid place-items-center rounded-lg disabled:opacity-40"
                        :class="isDefaultTemplate ? 'text-amber-500' : 'text-muted hover:text-ink'"
                        :title="$t('book.defaultTemplate')"
                        :disabled="!selectedTemplateId"
                        @click="toggleDefaultTemplate"
                    >
                        <Star :size="15" :fill="isDefaultTemplate ? 'currentColor' : 'none'" />
                    </button>
```

- [ ] **Step 5: Add the i18n key to all four locales**

In each of `en.ts`, `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts`, inside the `book` object, add:
- en: `defaultTemplate: 'Default template',`
- zh-Hans: `defaultTemplate: '默认模板',`
- zh-Hant: `defaultTemplate: '預設範本',`
- ja: `defaultTemplate: 'デフォルトのテンプレート',`

- [ ] **Step 6: Run tests to verify they pass**

Run: `npx vitest run src/views/NewChatView.test.ts src/views/BookView.test.ts src/locales/parity.test.ts`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/views/BookView.vue shirita-ui/src/views/NewChatView.vue shirita-ui/src/views/NewChatView.test.ts shirita-ui/src/locales
git commit -m "feat(template): set a default template and auto-select it on new chat"
```

---

### Task 14: Template can carry regex rules

**Files:**
- Modify: `shirita-ui/src/components/PromptTree.vue:65-77` (add Regex brick to the omnibox)
- Modify: `shirita-ui/src/components/NodeRow.vue` (regex inline editor + new emits)
- Modify: `shirita-ui/src/components/PromptTree.vue` (forward new NodeRow emits, both root and child rows)
- Modify: `shirita-ui/src/views/BookView.vue` (wire `update-def-meta` / `update-def-name`; seed regex meta on create)
- Test: `shirita-ui/src/components/PromptTree.test.ts` and `NodeRow.test.ts` (append)

**Interfaces:**
- NodeRow new emits: `updateDefMeta: [meta: Record<string, unknown>]`, `updateDefName: [name: string]`.
- PromptTree new emits: `updateDefMeta: [definitionId: string, meta: Record<string, unknown>]`, `updateDefName: [definitionId: string, name: string]`.
- BookView handlers: `handleUpdateDefMeta(defId, meta)`, `handleUpdateDefName(defId, name)` → `updateDefinition` + `library.loadDefinitions()`.

- [ ] **Step 1: Write the failing PromptTree test**

Append to `PromptTree.test.ts`:

```ts
describe('PromptTree regex brick', () => {
  it('offers a Regex brick that creates a regex_rule at root', async () => {
    const w = mount(PromptTree, { props: { nodes: [], definitions: [], types: [] } })
    await w.find('[data-test="root-add"]').trigger('click')
    const btn = w.find('[data-test="create-regex_rule"]')
    expect(btn.exists()).toBe(true)
    await btn.trigger('click')
    expect(w.emitted('createNewInContainer')![0]).toEqual([null, 'regex_rule'])
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/PromptTree.test.ts`
Expected: FAIL — no `create-regex_rule` button.

- [ ] **Step 3: Add the Regex brick to the omnibox**

In `PromptTree.vue`, extend the `bricks` array (~67):

```ts
  const bricks: OmniItem[] = [
    { kind: 'brick', id: 'variables', name: 'Variables' },
    { kind: 'brick', id: 'regex_rule', name: 'Regex' },
  ]
```

- [ ] **Step 4: Run the PromptTree test to verify it passes**

Run: `npx vitest run src/components/PromptTree.test.ts`
Expected: PASS (`pickOmni` already routes `kind: 'brick'` to `createNewInContainer(null, id)`; the `data-test` is `create-regex_rule`).

- [ ] **Step 5: Write the failing NodeRow regex-editor test**

Append to `NodeRow.test.ts`:

```ts
describe('NodeRow regex editing', () => {
  const rxDefs: Record<string, Definition> = {
    r1: { id: 'r1', type: 'regex_rule', name: 'Clean', content: '', meta: { pattern: 'a', replacement: 'b' } },
  }
  function rxNode(): PromptNode {
    return { id: 'n1', owner_kind: 'template', owner_id: 't', parent_id: null, sort_order: 0,
      kind: 'ref', tag: null, definition_id: 'r1', enabled: true, created_at: '', meta: {} }
  }

  it('shows find/replace inputs and no content textarea for a regex ref', () => {
    const w = mount(NodeRow, { props: { node: rxNode(), definitions: rxDefs, depth: 0, isExpanded: true } })
    expect(w.find('[data-test="regex-find"]').exists()).toBe(true)
    expect(w.find('[data-test="node-content"]').exists()).toBe(false)
  })

  it('emits updateDefMeta when the pattern changes', async () => {
    const w = mount(NodeRow, { props: { node: rxNode(), definitions: rxDefs, depth: 0, isExpanded: true } })
    await w.find('[data-test="regex-find"]').setValue('xyz')
    const ev = w.emitted('updateDefMeta')
    expect(ev).toBeTruthy()
    expect((ev![ev!.length - 1][0] as Record<string, unknown>).pattern).toBe('xyz')
  })
})
```

- [ ] **Step 6: Run test to verify it fails**

Run: `npx vitest run src/components/NodeRow.test.ts`
Expected: FAIL — no `regex-find` element; content textarea still shows.

- [ ] **Step 7: Add the regex inline editor to NodeRow**

In `NodeRow.vue` `<script setup>`:

Extend the emits (`~19-27`):

```ts
const emit = defineEmits<{
  toggleEnabled: []
  toggleExpand: []
  updateContent: [content: string]
  delete: []
  updateTrigger: [trigger: Trigger]
  updateNodeMeta: [meta: Record<string, unknown>]
  updateDefMeta: [meta: Record<string, unknown>]
  updateDefName: [name: string]
  add: []
}>()
```

Add the regex bridge import (top, with other imports):

```ts
import { metaToRule, scopeFlagsToMeta } from '../utils/regexRule'
```

Add computeds + handlers (after `def`, ~37):

```ts
const isRegex = computed(() => def.value?.type === 'regex_rule')
const regexRule = computed(() => (def.value ? metaToRule(def.value) : null))
function updateRegexMeta(patch: Record<string, unknown>) {
  if (def.value) emit('updateDefMeta', { ...def.value.meta, ...patch })
}
function updateRegexScope(scope: { ai_output: boolean; user_input: boolean; phase: 'display' | 'both' | 'prompt' }) {
  updateRegexMeta(scopeFlagsToMeta(scope))
}
```

In the template, inside the inline content-editor block (`v-if="!isFolder && !isHistory && !isContent && isExpanded"`, ~209), wrap the existing content textarea + trigger + wrap in `v-if="!isRegex"`, and add a regex panel for `v-else`. Replace the inner `<div class="relative"> … </div>` content textarea and following blocks so the structure is:

```html
    <div v-if="!isFolder && !isHistory && !isContent && isExpanded" :style="{ paddingLeft: `${8 + (depth + 1) * 26}px` }" class="pr-2 pb-2 pt-0.5">
      <!-- regex ref: find / replace / apply-to (edits the definition meta) -->
      <div v-if="isRegex && regexRule" data-test="regex-inline" class="space-y-2.5">
        <label class="block">
          <span class="text-[11px] text-muted uppercase tracking-wide block mb-1">{{ $t('definition.heading') }}</span>
          <input
            data-test="regex-name"
            :value="def!.name"
            type="text"
            class="field w-full !py-1.5 text-[13px]"
            @input="emit('updateDefName', ($event.target as HTMLInputElement).value)"
          />
        </label>
        <label class="block">
          <span class="text-[11px] text-muted uppercase tracking-wide block mb-1">{{ $t('settings.regexFind') }}</span>
          <input
            data-test="regex-find"
            :value="regexRule.pattern"
            type="text"
            class="field w-full !py-1.5 text-[13px] font-mono"
            @input="updateRegexMeta({ pattern: ($event.target as HTMLInputElement).value })"
          />
        </label>
        <label class="block">
          <span class="text-[11px] text-muted uppercase tracking-wide block mb-1">{{ $t('settings.regexReplace') }}</span>
          <input
            data-test="regex-replace"
            :value="regexRule.replacement"
            type="text"
            class="field w-full !py-1.5 text-[13px] font-mono"
            @input="updateRegexMeta({ replacement: ($event.target as HTMLInputElement).value })"
          />
        </label>
        <div>
          <span class="text-[11px] text-muted uppercase tracking-wide block mb-1.5">{{ $t('settings.regexApplyTo') }}</span>
          <div class="flex flex-wrap gap-3 items-center">
            <label class="flex items-center gap-1 text-[13px]"><input type="checkbox" :checked="regexRule.scope.ai_output" class="w-3 h-3 rounded accent-primary" @change="updateRegexScope({ ...regexRule.scope, ai_output: ($event.target as HTMLInputElement).checked })" /> {{ $t('settings.regexAiOutput') }}</label>
            <label class="flex items-center gap-1 text-[13px]"><input type="checkbox" :checked="regexRule.scope.user_input" class="w-3 h-3 rounded accent-primary" @change="updateRegexScope({ ...regexRule.scope, user_input: ($event.target as HTMLInputElement).checked })" /> {{ $t('settings.regexUserInput') }}</label>
            <select :value="regexRule.scope.phase" class="field !py-1 text-[13px]" @change="updateRegexScope({ ...regexRule.scope, phase: ($event.target as HTMLSelectElement).value as 'display'|'both'|'prompt' })">
              <option value="display">{{ $t('settings.regexPhaseDisplay') }}</option>
              <option value="both">{{ $t('settings.regexPhaseBoth') }}</option>
              <option value="prompt">{{ $t('settings.regexPhasePrompt') }}</option>
            </select>
          </div>
        </div>
      </div>

      <!-- non-regex ref: content + trigger + wrap (unchanged) -->
      <template v-else>
        <div class="relative">
          <textarea
            v-model="draft"
            rows="3"
            data-test="node-content"
            class="w-full resize-y rounded-[9px] border border-line bg-card px-3 py-2.5 pr-8 text-[13px] leading-relaxed text-ink/75 outline-none focus:border-primary/50"
            :placeholder="$t('definition.contentPlaceholder')"
            @blur="commit"
          />
          <button
            data-test="node-fullscreen"
            class="absolute right-2 top-2 text-muted/70 hover:text-ink"
            :title="$t('settings.fullscreen')"
            @click="fullscreenOpen = true"
          >
            <Maximize2 :size="15" />
          </button>
        </div>

        <div v-if="def && !['prompt','regex_rule','tool'].includes(def.type)" class="mt-2.5">
          <TriggerEditor
            :model-value="triggerFromMeta(def.meta)"
            @update:model-value="emit('updateTrigger', $event)"
          />
        </div>

        <label
          v-if="showWrapToggle"
          data-test="node-wrap-in-tag"
          class="flex items-center gap-2 mt-2.5 text-[13px] text-ink"
          :title="$t('definition.wrapInTagHint')"
        >
          {{ $t('definition.wrapInTag') }}
          <ToggleSwitch :model-value="wrapValue" @update:model-value="updateWrap" />
        </label>
      </template>
    </div>
```

- [ ] **Step 8: Forward the new emits in PromptTree**

In `PromptTree.vue`, extend the emits block (~9-23):

```ts
  updateDefMeta: [definitionId: string, meta: Record<string, unknown>]
  updateDefName: [definitionId: string, name: string]
```

On BOTH `<NodeRow>` usages (root row ~153 and child row ~179), add:

```html
        @update-def-meta="(m) => node.definition_id && emit('updateDefMeta', node.definition_id, m)"
        @update-def-name="(n) => node.definition_id && emit('updateDefName', node.definition_id, n)"
```

(For the child row use `child.definition_id` and `child` accordingly.)

- [ ] **Step 9: Wire BookView handlers + seed regex meta**

In `BookView.vue`, add two handlers (near `handleUpdateTrigger`, ~695):

```ts
async function handleUpdateDefMeta(definitionId: string, meta: Record<string, unknown>) {
    try {
        await updateDefinition(definitionId, { meta });
        await library.loadDefinitions();
    } catch (e) { error.value = (e as Error).message; }
}
async function handleUpdateDefName(definitionId: string, name: string) {
    try {
        await updateDefinition(definitionId, { name });
        await library.loadDefinitions();
    } catch (e) { error.value = (e as Error).message; }
}
```

Wire them on BOTH `<PromptTree>` usages (the local tree ~871 and the template tree ~1031):

```html
        @update-def-meta="handleUpdateDefMeta"
        @update-def-name="handleUpdateDefName"
```

Seed sensible regex defaults so a new regex brick is a valid no-op rule. In `createNewInContainer` (~611) and `localCreateNewInContainer` (~337) and `createNewPrompt`'s sibling path, change the `createDefinition` call to special-case regex:

```ts
        const isRx = typeId === "regex_rule";
        const def = await createDefinition({
            type: typeId,
            name: isRx ? "New rule" : `New ${typeId}`,
            content: "",
            meta: isRx ? { pattern: "", replacement: "", disabled: false, scope: "display", targets: ["ai_output"] } : {},
        });
```

(Apply the same `isRx` seed in both `createNewInContainer` and `localCreateNewInContainer`.)

- [ ] **Step 10: Run tests to verify they pass**

Run: `npx vitest run src/components/NodeRow.test.ts src/components/PromptTree.test.ts src/views/BookView.test.ts`
Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add shirita-ui/src/components/NodeRow.vue shirita-ui/src/components/PromptTree.vue shirita-ui/src/views/BookView.vue shirita-ui/src/components/NodeRow.test.ts shirita-ui/src/components/PromptTree.test.ts
git commit -m "feat(template): attach and edit regex rules in the prompt tree"
```

---

### Task 15: Full verification — suite, build, and visual (desktop + mobile)

**Files:** none (verification only).

- [ ] **Step 1: Run the full frontend suite**

Run: `npx vitest run` from `shirita-ui/`
Expected: all tests pass, including `src/locales/parity.test.ts`.

- [ ] **Step 2: Type-check + build**

Run: `npm run build` from `shirita-ui/`
Expected: `vue-tsc` clean, Vite build succeeds.

- [ ] **Step 3: Run the Rust core suite**

Run: `cargo test -p shirita-core` from repo root
Expected: all pass.

- [ ] **Step 4: Visual check (desktop + 375px mobile)**

Serve the app (e.g. `npm run dev` in `shirita-ui/` with the web backend running, per the project's run skill) and use Playwright to confirm:
- Header shows the logo image (item 2).
- Home `/new` button glows (item 7).
- Book template + pack toolbars do NOT overflow at 375px width — the icon buttons wrap to a new row (items 1 & 10). Fix any other row that actually overflows at 375px.
- Book node rows: the delete trash icon is visible/tappable without hover (item 8).
- Settings: API-key bullets are spaced (item 3); max-output is a slider with an Unlimited toggle (item 4); custom-CSS placeholder no longer lists hook selectors (item 5).

- [ ] **Step 5: Final commit (only if Step 4 required a tweak)**

```bash
git add -A
git commit -m "fix(book): wrap remaining overflowing toolbar at mobile width"
```

---

## Self-Review

**Spec coverage:**
- Item 1 (mobile overflow) → Task 10 + verified in Task 15.
- Item 2 (header image) → Task 7.
- Item 3 (api-key dots) → Task 6.
- Item 4 (defaults + slider/unlimited) → Tasks 1 (flat), 2 (temp/max defaults), 3 (backend 0→None), 4 (slider+toggle).
- Item 5 (custom CSS hook) → Task 5.
- Item 6 (book remembers) → Task 12.
- Item 7 (/new glow) → Task 8.
- Item 8 (mobile node delete) → Task 9.
- Item 9 (scan-deep gating) → Task 11.
- Item 10 (template default) → Task 13.
- Item 11 (template carries regex) → Task 14.
All items covered.

**Type consistency:** NodeRow emits `updateDefMeta`/`updateDefName`; PromptTree forwards as `updateDefMeta(defId, meta)`/`updateDefName(defId, name)`; BookView consumes via `handleUpdateDefMeta`/`handleUpdateDefName`. `metaToRule`/`scopeFlagsToMeta` reused from `utils/regexRule`. `provider_max_tokens` sentinel `0` is written by Task 4 and read as `None` by Task 3.

**Placeholder scan:** No TBD/TODO; every code step shows full code.
