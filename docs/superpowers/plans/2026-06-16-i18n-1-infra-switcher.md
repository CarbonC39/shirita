# i18n Plan 1 — Infrastructure + Language Switcher

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up vue-i18n in `shirita-ui` with 4 locales (en / zh-Hans / zh-Hant / ja), browser-language detection, a persisted SettingsView switcher, type-safe keys, and global test injection — with the catalog holding only the first few namespaces (full string extraction is Plans 2 & 3).

**Architecture:** Centralized per-locale catalogs (`locales/{en,zh-Hans,zh-Hant,ja}.ts`), `en` as source-of-truth + `fallbackLocale`. A pure `resolve.ts` maps `navigator.language`/`localStorage` to a supported `AppLocale`. The `ui` Pinia store owns `locale` + `setLocale` (mirrors the existing `theme` pattern) and drives `i18n.global.locale`. Component tests get i18n globally via a Vitest setup file, so no per-file injection.

**Tech Stack:** Vue 3 `<script setup>`, vue-i18n@^10 (Composition API, `legacy:false`), Pinia, Vitest + @vue/test-utils, vue-tsc/Volar.

**Spec:** `docs/superpowers/specs/2026-06-16-i18n-zh-ja-design.md` (§3–§8).

---

## File Structure

- Create `shirita-ui/src/locales/resolve.ts` — `AppLocale` type, `SUPPORTED`, `normalizeLocale`, `resolveInitialLocale`.
- Create `shirita-ui/src/locales/resolve.test.ts` — unit tests for the above.
- Create `shirita-ui/src/locales/en.ts` — source-of-truth catalog (skeleton namespaces only this plan).
- Create `shirita-ui/src/locales/zh-Hans.ts`, `zh-Hant.ts`, `ja.ts` — translations of the en skeleton, typed `: MessageSchema`.
- Create `shirita-ui/src/locales/parity.test.ts` — key-set parity of the 3 non-en locales vs en.
- Create `shirita-ui/src/types/vue-i18n.d.ts` — `DefineLocaleMessage` augmentation from `typeof en`.
- Create `shirita-ui/src/i18n.ts` — `createI18n` instance.
- Create `shirita-ui/src/i18n.switch.test.ts` — switching the global locale re-renders `$t`.
- Create `shirita-ui/src/test/setup.ts` — global `config.global.plugins = [i18n]`.
- Modify `shirita-ui/package.json` — add `vue-i18n` dependency.
- Modify `shirita-ui/src/main.ts` — `.use(i18n)`.
- Modify `shirita-ui/src/stores/ui.ts` — `locale` state + `setLocale` action.
- Modify `shirita-ui/vite.config.ts` — `test.setupFiles`.
- Modify `shirita-ui/src/views/SettingsView.vue` — replace the placeholder Language `<section>` with the bound 4-option switcher.

---

### Task 1: Install vue-i18n

**Files:**
- Modify: `shirita-ui/package.json`

- [ ] **Step 1: Install the dependency**

Run from repo root:
```bash
npm --prefix shirita-ui install vue-i18n@^10
```
Expected: `package.json` gains `"vue-i18n": "^10.x"` under `dependencies`; `package-lock.json` updated; exit 0.

- [ ] **Step 2: Verify it resolves**

Run:
```bash
npm --prefix shirita-ui ls vue-i18n
```
Expected: prints `vue-i18n@10.x.x` (a 10.x version), no `UNMET DEPENDENCY`.

- [ ] **Step 3: Commit**

```bash
git add shirita-ui/package.json shirita-ui/package-lock.json
git commit -m "build(ui): add vue-i18n@^10 for UI i18n

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Locale resolution (`resolve.ts`) — TDD

**Files:**
- Create: `shirita-ui/src/locales/resolve.ts`
- Test: `shirita-ui/src/locales/resolve.test.ts`

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/locales/resolve.test.ts`:
```ts
import { describe, it, expect, afterEach, vi } from 'vitest'
import { normalizeLocale, resolveInitialLocale, SUPPORTED } from './resolve'

describe('normalizeLocale', () => {
  it.each([
    ['zh-CN', 'zh-Hans'],
    ['zh', 'zh-Hans'],
    ['zh-Hans', 'zh-Hans'],
    ['zh-SG', 'zh-Hans'],
    ['zh-TW', 'zh-Hant'],
    ['zh-HK', 'zh-Hant'],
    ['zh-MO', 'zh-Hant'],
    ['zh-Hant', 'zh-Hant'],
    ['zh-Hant-HK', 'zh-Hant'],
    ['ja-JP', 'ja'],
    ['ja', 'ja'],
    ['en-US', 'en'],
    ['en', 'en'],
  ])('maps %s -> %s', (input, expected) => {
    expect(normalizeLocale(input)).toBe(expected)
  })

  it.each([['fr'], ['de-DE'], [''], [null], [undefined]])(
    'returns null for unsupported %s',
    (input) => {
      expect(normalizeLocale(input as string | null | undefined)).toBeNull()
    },
  )
})

describe('SUPPORTED', () => {
  it('lists the four app locales', () => {
    expect(SUPPORTED).toEqual(['en', 'zh-Hans', 'zh-Hant', 'ja'])
  })
})

describe('resolveInitialLocale', () => {
  afterEach(() => {
    localStorage.clear()
    vi.unstubAllGlobals()
  })

  it('prefers a valid localStorage ui.locale', () => {
    localStorage.setItem('ui.locale', 'ja')
    expect(resolveInitialLocale()).toBe('ja')
  })

  it('falls back to navigator.language when no localStorage', () => {
    vi.stubGlobal('navigator', { language: 'zh-TW' })
    expect(resolveInitialLocale()).toBe('zh-Hant')
  })

  it('falls back to en when nothing matches', () => {
    vi.stubGlobal('navigator', { language: 'fr-FR' })
    expect(resolveInitialLocale()).toBe('en')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```bash
npm --prefix shirita-ui run test -- src/locales/resolve.test.ts
```
Expected: FAIL — cannot resolve module `./resolve`.

- [ ] **Step 3: Write the implementation**

Create `shirita-ui/src/locales/resolve.ts`:
```ts
export type AppLocale = 'en' | 'zh-Hans' | 'zh-Hant' | 'ja'

export const SUPPORTED: AppLocale[] = ['en', 'zh-Hans', 'zh-Hant', 'ja']

/** Map any BCP-47 tag to a supported locale; null if unsupported. */
export function normalizeLocale(
  tag: string | null | undefined,
): AppLocale | null {
  if (!tag) return null
  const t = tag.toLowerCase()
  if (t.startsWith('zh')) {
    // Traditional: explicit `hant`, or the TW/HK/MO regions.
    if (
      t.includes('hant') ||
      /\b(tw|hk|mo)\b/.test(t) ||
      t.endsWith('-tw') ||
      t.endsWith('-hk') ||
      t.endsWith('-mo')
    ) {
      return 'zh-Hant'
    }
    return 'zh-Hans'
  }
  if (t.startsWith('ja')) return 'ja'
  if (t.startsWith('en')) return 'en'
  return null
}

/** Startup value: localStorage first, then browser language, then en. */
export function resolveInitialLocale(): AppLocale {
  const saved = normalizeLocale(localStorage.getItem('ui.locale'))
  if (saved) return saved
  const nav = typeof navigator !== 'undefined' ? navigator.language : null
  return normalizeLocale(nav) ?? 'en'
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:
```bash
npm --prefix shirita-ui run test -- src/locales/resolve.test.ts
```
Expected: PASS — all cases green.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/locales/resolve.ts shirita-ui/src/locales/resolve.test.ts
git commit -m "feat(ui): i18n locale resolution (browser-language detection)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Skeleton catalogs (en source + 3 translations)

**Files:**
- Create: `shirita-ui/src/locales/en.ts`
- Create: `shirita-ui/src/locales/zh-Hans.ts`
- Create: `shirita-ui/src/locales/zh-Hant.ts`
- Create: `shirita-ui/src/locales/ja.ts`

This plan seeds only `common`, `shell`, and `settings` namespaces (plus the `common.tokensEstimate` plural example). Plans 2 & 3 grow them.

- [ ] **Step 1: Write the en source-of-truth catalog**

Create `shirita-ui/src/locales/en.ts`:
```ts
// Source of truth. Every other locale's key set must match this exactly
// (enforced by parity.test.ts). en is also the i18n fallbackLocale.
// NB: NO `as const` — the schema's leaf type must be `string` so the other
// locales can hold their own strings. `as const` would pin values to literals
// (e.g. save: 'Save') and reject translations at compile time.
const en = {
  common: {
    save: 'Save',
    cancel: 'Cancel',
    delete: 'Delete',
    duplicate: 'Duplicate',
    add: 'Add',
    close: 'Close',
    import: 'Import',
    export: 'Export',
    // Plural example: en distinguishes 1 vs many; zh/ja use a single form.
    tokensEstimate: '~{count} token | ~{count} tokens',
  },
  shell: {
    chats: 'Chats',
    new: 'New',
    book: 'Book',
    settings: 'Settings',
  },
  settings: {
    title: 'Settings',
    language: 'Language',
  },
}

export default en
```

- [ ] **Step 2: Define the shared schema type via the en module**

The schema type is `typeof en`. The three translations import it through the type-augmentation module (Task 4) — but to type their default exports independently of that module, re-export the type from en. Append to `shirita-ui/src/locales/en.ts`:
```ts
export type MessageSchema = typeof en
```

- [ ] **Step 3: Write zh-Hans**

Create `shirita-ui/src/locales/zh-Hans.ts`:
```ts
import type { MessageSchema } from './en'

const zhHans: MessageSchema = {
  common: {
    save: '保存',
    cancel: '取消',
    delete: '删除',
    duplicate: '复制',
    add: '添加',
    close: '关闭',
    import: '导入',
    export: '导出',
    tokensEstimate: '~{count} tokens',
  },
  shell: {
    chats: '对话',
    new: '新建',
    book: '设定集',
    settings: '设置',
  },
  settings: {
    title: '设置',
    language: '语言',
  },
}

export default zhHans
```

- [ ] **Step 4: Write zh-Hant**

Create `shirita-ui/src/locales/zh-Hant.ts` (Traditional vocabulary: 軟體/設定/螢幕, not pure char conversion):
```ts
import type { MessageSchema } from './en'

const zhHant: MessageSchema = {
  common: {
    save: '儲存',
    cancel: '取消',
    delete: '刪除',
    duplicate: '複製',
    add: '新增',
    close: '關閉',
    import: '匯入',
    export: '匯出',
    tokensEstimate: '~{count} tokens',
  },
  shell: {
    chats: '對話',
    new: '新增',
    book: '設定集',
    settings: '設定',
  },
  settings: {
    title: '設定',
    language: '語言',
  },
}

export default zhHant
```

- [ ] **Step 5: Write ja**

Create `shirita-ui/src/locales/ja.ts`:
```ts
import type { MessageSchema } from './en'

const ja: MessageSchema = {
  common: {
    save: '保存',
    cancel: 'キャンセル',
    delete: '削除',
    duplicate: '複製',
    add: '追加',
    close: '閉じる',
    import: 'インポート',
    export: 'エクスポート',
    tokensEstimate: '~{count} トークン',
  },
  shell: {
    chats: 'チャット',
    new: '新規',
    book: 'ブック',
    settings: '設定',
  },
  settings: {
    title: '設定',
    language: '言語',
  },
}

export default ja
```

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts
git commit -m "feat(ui): i18n skeleton catalogs (en/zh-Hans/zh-Hant/ja)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Type-safe keys (`vue-i18n.d.ts`)

**Files:**
- Create: `shirita-ui/src/types/vue-i18n.d.ts`

- [ ] **Step 1: Write the augmentation**

Create `shirita-ui/src/types/vue-i18n.d.ts`:
```ts
import type { MessageSchema } from '../locales/en'

declare module 'vue-i18n' {
  // Constrain t / $t keys to the en structure: completion, spell-check,
  // and a compile error on a missing/typo'd key.
  export interface DefineLocaleMessage extends MessageSchema {}
}

export {}
```

- [ ] **Step 2: Verify the type-check passes**

Run:
```bash
npm --prefix shirita-ui run build
```
Expected: exit 0, no errors. (The three translation modules already satisfy `MessageSchema`; this only adds the global key constraint.)

- [ ] **Step 3: Commit**

```bash
git add shirita-ui/src/types/vue-i18n.d.ts
git commit -m "feat(ui): type-safe i18n keys via DefineLocaleMessage augmentation

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: The i18n instance (`i18n.ts`)

**Files:**
- Create: `shirita-ui/src/i18n.ts`

- [ ] **Step 1: Write the instance**

Create `shirita-ui/src/i18n.ts`:
```ts
import { createI18n } from 'vue-i18n'
import en from './locales/en'
import zhHans from './locales/zh-Hans'
import zhHant from './locales/zh-Hant'
import ja from './locales/ja'
import { resolveInitialLocale } from './locales/resolve'

export const i18n = createI18n({
  legacy: false,
  locale: resolveInitialLocale(),
  fallbackLocale: 'en',
  messages: { en, 'zh-Hans': zhHans, 'zh-Hant': zhHant, ja },
})
```

- [ ] **Step 2: Verify it type-checks**

Run:
```bash
npm --prefix shirita-ui run build
```
Expected: exit 0.

- [ ] **Step 3: Commit**

```bash
git add shirita-ui/src/i18n.ts
git commit -m "feat(ui): create vue-i18n instance (legacy:false, en fallback)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 6: Parity test (key-set alignment)

**Files:**
- Create: `shirita-ui/src/locales/parity.test.ts`

- [ ] **Step 1: Write the test**

Create `shirita-ui/src/locales/parity.test.ts`:
```ts
import { describe, it, expect } from 'vitest'
import en from './en'
import zhHans from './zh-Hans'
import zhHant from './zh-Hant'
import ja from './ja'

/** Recursively collect dotted leaf-key paths from a nested message object. */
function leafKeys(obj: Record<string, unknown>, prefix = ''): string[] {
  return Object.entries(obj).flatMap(([k, v]) => {
    const path = prefix ? `${prefix}.${k}` : k
    return v !== null && typeof v === 'object'
      ? leafKeys(v as Record<string, unknown>, path)
      : [path]
  })
}

const enKeys = leafKeys(en).sort()

describe.each([
  ['zh-Hans', zhHans],
  ['zh-Hant', zhHant],
  ['ja', ja],
])('%s key parity with en', (_name, catalog) => {
  it('has exactly the same key set as en', () => {
    expect(leafKeys(catalog as Record<string, unknown>).sort()).toEqual(enKeys)
  })
})
```

- [ ] **Step 2: Run test to verify it passes**

Run:
```bash
npm --prefix shirita-ui run test -- src/locales/parity.test.ts
```
Expected: PASS — three locales align with en. (This test is the standing safety net for Plans 2 & 3.)

- [ ] **Step 3: Commit**

```bash
git add shirita-ui/src/locales/parity.test.ts
git commit -m "test(ui): i18n catalog key-parity across four locales

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 7: Global test injection (Vitest setup)

**Files:**
- Create: `shirita-ui/src/test/setup.ts`
- Modify: `shirita-ui/vite.config.ts`

- [ ] **Step 1: Write the setup file**

Create `shirita-ui/src/test/setup.ts`:
```ts
import { config } from '@vue/test-utils'
import { i18n } from '../i18n'

// Every mounted component gets i18n automatically — no per-file injection,
// so `$t` / useI18n() never throw "is not a function" in tests.
config.global.plugins = [i18n]
```

- [ ] **Step 2: Wire it into Vitest**

In `shirita-ui/vite.config.ts`, add `setupFiles` to the `test` block. Change:
```ts
  test: {
    environment: 'jsdom',
    globals: true,
    env: { VITE_API_BASE: '', VITE_API_TOKEN: 'test-token' },
  },
```
to:
```ts
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    env: { VITE_API_BASE: '', VITE_API_TOKEN: 'test-token' },
  },
```

- [ ] **Step 3: Verify the full existing suite still passes**

Run:
```bash
npm --prefix shirita-ui run test --
```
Expected: PASS — all existing tests green. Existing English assertions (`'Character'`, `'World'`, `'Prompt'`, `'zion'`, etc.) are unchanged because jsdom has no `ui.locale` localStorage and no zh `navigator.language`, so `resolveInitialLocale()` → `en`.

- [ ] **Step 4: Commit**

```bash
git add shirita-ui/src/test/setup.ts shirita-ui/vite.config.ts
git commit -m "test(ui): inject i18n globally for all component tests

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 8: Switch test (locale change re-renders)

**Files:**
- Create: `shirita-ui/src/i18n.switch.test.ts`

- [ ] **Step 1: Write the test**

Create `shirita-ui/src/i18n.switch.test.ts`:
```ts
import { describe, it, expect, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { i18n } from './i18n'

const Probe = defineComponent({
  setup() {
    return () => h('span', i18n.global.t('shell.settings'))
  },
})

describe('i18n locale switching', () => {
  afterEach(() => {
    i18n.global.locale.value = 'en'
  })

  it('re-renders $t output when the global locale changes', async () => {
    const wrapper = mount(Probe) // global i18n plugin comes from setup.ts
    expect(wrapper.text()).toBe('Settings')
    i18n.global.locale.value = 'zh-Hant'
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toBe('設定')
    i18n.global.locale.value = 'ja'
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toBe('設定')
  })
})
```

- [ ] **Step 2: Run test to verify it passes**

Run:
```bash
npm --prefix shirita-ui run test -- src/i18n.switch.test.ts
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add shirita-ui/src/i18n.switch.test.ts
git commit -m "test(ui): locale switch re-renders translated text

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 9: Wire i18n into the app + ui store

**Files:**
- Modify: `shirita-ui/src/main.ts`
- Modify: `shirita-ui/src/stores/ui.ts`

- [ ] **Step 1: Use the plugin in `main.ts`**

Change `shirita-ui/src/main.ts` to:
```ts
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { router } from './router'
import { i18n } from './i18n'
import './styles.css'

createApp(App).use(createPinia()).use(router).use(i18n).mount('#app')
```

- [ ] **Step 2: Add `locale` + `setLocale` to the ui store**

In `shirita-ui/src/stores/ui.ts`, add imports at the top (after the pinia import):
```ts
import { i18n } from '../i18n'
import { resolveInitialLocale, type AppLocale } from '../locales/resolve'
```
Add to the `state` object (after `activeChatId`):
```ts
    // UI language. Persisted to localStorage (key `ui.locale`); resolved on
    // boot from localStorage -> navigator.language -> en. Mirrors `theme`.
    locale: resolveInitialLocale() as AppLocale,
```
Add to the `actions` object (after `setBackground`):
```ts
    setLocale(locale: AppLocale) {
      this.locale = locale
      localStorage.setItem('ui.locale', locale)
      i18n.global.locale.value = locale
    },
```

- [ ] **Step 3: Verify type-check + tests**

Run:
```bash
npm --prefix shirita-ui run build && npm --prefix shirita-ui run test --
```
Expected: both green.

- [ ] **Step 4: Commit**

```bash
git add shirita-ui/src/main.ts shirita-ui/src/stores/ui.ts
git commit -m "feat(ui): wire i18n into app + ui store locale/setLocale

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 10: Language switcher in SettingsView

Replaces the existing placeholder Language `<section>` (currently an unbound 2-option `<select>` at `SettingsView.vue:715-726`) with a bound 4-option switcher.

**Files:**
- Modify: `shirita-ui/src/views/SettingsView.vue`
- Test: `shirita-ui/src/views/SettingsView.i18n.test.ts` (create)

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/views/SettingsView.i18n.test.ts` (follows the repo's `vi.spyOn(client, …)` + `flushPromises` pattern, e.g. `HomeView.test.ts`):
```ts
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import * as client from '../api/client'
import { i18n } from '../i18n'
import SettingsView from './SettingsView.vue'

describe('SettingsView language switcher', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
    i18n.global.locale.value = 'en'
    // onMounted: settings.load() -> getSettings, then listDefinitions.
    // Empty settings means no API key, so no live model fetch fires.
    vi.spyOn(client, 'getSettings').mockResolvedValue({})
    vi.spyOn(client, 'listDefinitions').mockResolvedValue([])
  })

  it('renders a 4-option locale switcher and switches locale on change', async () => {
    const wrapper = mount(SettingsView)
    await flushPromises()

    const switcher = wrapper.get('[data-test="locale-switcher"]')
    const options = switcher.findAll('option')
    expect(options.map((o) => o.text())).toEqual([
      'English',
      '简体中文',
      '繁體中文',
      '日本語',
    ])

    await switcher.setValue('zh-Hant') // 繁體中文
    expect(i18n.global.locale.value).toBe('zh-Hant')
    expect(localStorage.getItem('ui.locale')).toBe('zh-Hant')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```bash
npm --prefix shirita-ui run test -- src/views/SettingsView.i18n.test.ts
```
Expected: FAIL — no `[data-test="locale-switcher"]` element.

- [ ] **Step 3: Add the `Languages` icon import**

In `SettingsView.vue`, change the lucide import (line 19):
```ts
import { Maximize2, Eye, EyeOff, Check } from "lucide-vue-next";
```
to:
```ts
import { Maximize2, Eye, EyeOff, Check, Languages } from "lucide-vue-next";
```

- [ ] **Step 4: Replace the placeholder Language section**

In `SettingsView.vue`, replace the entire existing Language `<section>` block:
```html
            <!-- Language -->
            <section class="mb-8">
                <h3
                    class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4"
                >
                    Language
                </h3>
                <select class="field w-full">
                    <option value="en">English</option>
                    <option value="zh">中文</option>
                </select>
            </section>
```
with:
```html
            <!-- Language -->
            <section class="mb-8">
                <h3
                    class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4 flex items-center gap-1.5"
                >
                    <Languages :size="14" />{{ $t("settings.language") }}
                </h3>
                <select
                    data-test="locale-switcher"
                    :value="ui.locale"
                    class="field w-full"
                    @change="
                        ui.setLocale(
                            ($event.target as HTMLSelectElement).value as
                                | 'en'
                                | 'zh-Hans'
                                | 'zh-Hant'
                                | 'ja',
                        )
                    "
                >
                    <option value="en">English</option>
                    <option value="zh-Hans">简体中文</option>
                    <option value="zh-Hant">繁體中文</option>
                    <option value="ja">日本語</option>
                </select>
            </section>
```

> Rationale for `<select>` over `SegmentedControl`: four CJK labels overflow the segmented pill on the 520px settings column (spec §7 note). The Step 1 test already targets `<select>`/`<option>` and `setValue`, so it goes green against this markup.

- [ ] **Step 5: Run test to verify it passes**

Run:
```bash
npm --prefix shirita-ui run test -- src/views/SettingsView.i18n.test.ts
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/views/SettingsView.vue shirita-ui/src/views/SettingsView.i18n.test.ts
git commit -m "feat(ui): bound 4-locale switcher in SettingsView

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 11: Full green gate

**Files:** none (verification only)

- [ ] **Step 1: Type-check**

Run:
```bash
npm --prefix shirita-ui run build
```
Expected: exit 0.

- [ ] **Step 2: Full test run**

Run:
```bash
npm --prefix shirita-ui run test --
```
Expected: all suites green.

- [ ] **Step 3: Production build**

Run:
```bash
npm --prefix shirita-ui run build
```
Expected: `vue-tsc -b` + `vite build` succeed, `dist/` produced, no errors.

- [ ] **Step 4: Manual smoke (optional, recommended)**

Run `npm --prefix shirita-ui run dev`, open Settings, switch language to 繁體中文 / 日本語 / 简体中文 / English, confirm the Settings heading + nav update live and the choice survives a page reload (localStorage). Full UI translation lands in Plans 2 & 3 — at this point only the seeded `common`/`shell`/`settings` strings change.

This is the terminal task of Plan 1; do **not** run finishing-a-development-branch here — Plans 2 and 3 continue on the same branch. Finish the branch only after Plan 3.
