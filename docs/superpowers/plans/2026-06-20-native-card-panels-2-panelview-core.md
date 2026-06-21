# Native Card Panels — Plan 2: `<PanelView>` core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the read-only rendering core of a Pack panel: a `<PanelView>` that renders sanitized author HTML + fenced scoped CSS inside a Shadow DOM, binds `{{var}}` / `data-bind` / `data-show` reactively from the session variable values, and updates via **morphdom** (so an opened `<details>`, a text selection, and CSS transitions survive a state change).

**Architecture:** Two units. (1) `utils/panel.ts` — pure, heavily-unit-tested security helpers: `sanitizePanelHtml` (DOMPurify allowlist) and `fenceCss` (strips the few host-affecting / exfil CSS properties). (2) `PanelView.vue` — attaches a Shadow DOM to a containment-forced host `<div>`, parses the sanitized template once, and on every `values` change clones it, applies the bindings, and **morphs** the live shadow tree toward it. Actions (`data-diff` / insert / send) and chat placement are **Plan 3**; authoring UI is **Plan 4**.

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, `dompurify`, `morphdom`, Vitest + `@vue/test-utils` (jsdom — supports `attachShadow`, DOMPurify, and morphdom).

## Global Constraints

- **Zero card JS** — the panel never executes author script; sanitization removes the entire script / `on*` / `javascript:` class.
- **Shadow DOM** scopes the CSS; the **CSS fence** removes `@import`, `position: fixed|sticky`, external `url(http…)`, `expression()`, `behavior` — applied to both the `css` field and every element's inline `style` attribute (status cards lean on inline styles).
- **Host containment**: the host `<div>` carries forced inline `position: relative; overflow: hidden; contain: content` (traps `position:absolute`, clips overflow, contains layout/paint/style, and `content` not `strict` so height still grows).
- **Bindings, not actions**: this plan renders only — `{{var}}`, `data-bind`, `data-show`. `data-diff-*` / `data-insert` / `data-send` are parsed-through (sanitizer keeps the attributes) but wired in Plan 3.
- **morphdom, never `innerHTML` replacement**, with an `onBeforeElUpdated` guard preserving user-toggled `<details open>`.
- Only local image sources (`/assets/…`, relative) survive sanitization — no remote `<img>`.
- Comments/commits in English. Tests: `npm --prefix shirita-ui test -- <pattern>`; build: `npm --prefix shirita-ui run build`.

---

## File Structure

- `shirita-ui/src/api/types.ts` — add `Panel` + `PanelCaps`. (Task 1)
- `shirita-ui/src/utils/panel.ts` — `sanitizePanelHtml`, `fenceCss`. (Task 1)
- `shirita-ui/src/utils/panel.test.ts` — unit tests. (Task 1)
- `shirita-ui/src/components/PanelView.vue` — the shadow-DOM renderer. (Task 2)
- `shirita-ui/src/components/PanelView.test.ts` — component tests. (Task 2)
- `shirita-ui/package.json` — `dompurify`, `morphdom`, `@types/morphdom`. (Tasks 1 & 2)

---

### Task 1: Security helpers + `Panel` type

**Files:**
- Modify: `shirita-ui/src/api/types.ts`
- Create: `shirita-ui/src/utils/panel.ts`
- Test: `shirita-ui/src/utils/panel.test.ts`
- Modify: `shirita-ui/package.json` (add `dompurify`)

**Interfaces:**
- Produces: `sanitizePanelHtml(html: string): string`; `fenceCss(css: string): string`; types `Panel { html: string; css: string; caps: PanelCaps }`, `PanelCaps { write?: boolean; insert?: boolean; send?: boolean }`. (`PanelView` (Task 2) consumes the helpers; `Panel` is consumed by Plans 3/4.)

- [ ] **Step 1: Install DOMPurify**

Run: `npm --prefix shirita-ui install dompurify`
(DOMPurify v3 ships its own TypeScript types — no `@types/dompurify` needed.)

- [ ] **Step 2: Add the `Panel` / `PanelCaps` types**

In `shirita-ui/src/api/types.ts`, immediately after the `Pack` interface (it ends with `updated_at: string\n}`), add:

```ts
/** The non-read capability tiers a panel declares it uses. v1: declared == granted. */
export interface PanelCaps {
  write?: boolean
  insert?: boolean
  send?: boolean
}

/** A Pack's status panel: author HTML + scoped CSS + declared capabilities.
 *  Lives at `pack.meta.panel`; absent == the Pack has no panel. */
export interface Panel {
  html: string
  css: string
  caps: PanelCaps
}
```

- [ ] **Step 3: Write the failing tests**

Create `shirita-ui/src/utils/panel.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { sanitizePanelHtml, fenceCss } from './panel'

describe('sanitizePanelHtml', () => {
  it('strips <script>, on* handlers and javascript: urls', () => {
    const out = sanitizePanelHtml(
      '<div onclick="evil()">hi<script>alert(1)</script><a href="javascript:alert(1)">x</a></div>',
    )
    expect(out).not.toContain('<script')
    expect(out).not.toContain('onclick')
    expect(out).not.toContain('javascript:')
  })

  it('keeps safe structure, details/summary, and data-* bindings/actions', () => {
    const out = sanitizePanelHtml(
      '<details data-show="poisoned"><summary>S</summary>' +
      '<span data-bind="hp">x</span>' +
      '<button data-diff-key="hp" data-diff-op="sub" data-diff-value="1">-</button></details>',
    )
    expect(out).toContain('<details')
    expect(out).toContain('data-show="poisoned"')
    expect(out).toContain('data-bind="hp"')
    expect(out).toContain('data-diff-key="hp"')
  })

  it('drops remote img src but keeps /assets', () => {
    expect(sanitizePanelHtml('<img src="https://evil.com/x.png">')).not.toContain('evil.com')
    expect(sanitizePanelHtml('<img src="/assets/a.png">')).toContain('/assets/a.png')
  })
})

describe('fenceCss', () => {
  it('strips @import, position:fixed/sticky, remote url(), expression(), behavior', () => {
    const out = fenceCss(
      '@import url(http://x);a{position:fixed}b{background:url(https://e/x.png)}' +
      'c{width:expression(1)}d{behavior:url(#x)}',
    )
    expect(out).not.toMatch(/@import/i)
    expect(out).not.toMatch(/position\s*:\s*fixed/i)
    expect(out).not.toMatch(/url\(\s*https?:/i)
    expect(out).not.toMatch(/expression\(/i)
    expect(out).not.toMatch(/behavior\s*:/i)
  })

  it('leaves safe css and local urls intact', () => {
    const out = fenceCss('.x{color:red;background:url(/assets/b.png)}')
    expect(out).toContain('color:red')
    expect(out).toContain('/assets/b.png')
  })
})
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `npm --prefix shirita-ui test -- panel`
Expected: FAIL — `./panel` module doesn't exist yet.

- [ ] **Step 5: Implement `utils/panel.ts`**

Create `shirita-ui/src/utils/panel.ts`:

```ts
import DOMPurify from 'dompurify'

// What a status panel legitimately needs. Everything else (script, iframe,
// object, embed, form, link, meta, base, event handlers, javascript:/data: urls)
// is dropped. data-* attributes survive (bindings in this plan, actions in Plan 3).
const ALLOWED_TAGS = [
  'div', 'span', 'p', 'a', 'b', 'i', 'em', 'strong', 'small', 'br', 'hr',
  'ul', 'ol', 'li', 'table', 'thead', 'tbody', 'tr', 'td', 'th',
  'details', 'summary', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'img',
  'svg', 'path', 'g', 'circle', 'rect',
]
const ALLOWED_ATTR = [
  'class', 'style', 'title', 'alt', 'src', 'width', 'height', 'open', 'colspan', 'rowspan',
  'data-bind', 'data-show', 'data-diff-key', 'data-diff-op', 'data-diff-value', 'data-insert', 'data-send',
  'viewBox', 'fill', 'stroke', 'stroke-width', 'd', 'cx', 'cy', 'r', 'x', 'y',
]

/** Sanitize author panel HTML to a safe subset — no script, no remote/js URLs. */
export function sanitizePanelHtml(html: string): string {
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    ALLOW_DATA_ATTR: true,
    FORBID_TAGS: ['script', 'iframe', 'object', 'embed', 'form', 'link', 'meta', 'base', 'style'],
    // href/src may only be local or relative (blocks remote exfil, javascript:, data:).
    ALLOWED_URI_REGEXP: /^(?:\/assets\/|\/|\.\/|#)/i,
  }) as unknown as string
}

// Defensive CSS fence — the shadow root already scopes selectors; this removes the
// few properties that escape the box or phone home. Applied to the css field and
// to each element's inline style attribute (status cards rely on inline styles).
export function fenceCss(css: string): string {
  return css
    .replace(/@import[^;]*;?/gi, '')
    .replace(/position\s*:\s*(?:fixed|sticky)\s*;?/gi, '')
    .replace(/url\(\s*['"]?\s*(?:https?:)?\/\/[^)]*\)/gi, 'url()')
    .replace(/expression\s*\([^)]*\)/gi, '')
    .replace(/behavior\s*:[^;]*;?/gi, '')
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `npm --prefix shirita-ui test -- panel`
Expected: PASS — all five `sanitizePanelHtml` / `fenceCss` cases.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/utils/panel.ts shirita-ui/src/utils/panel.test.ts shirita-ui/package.json shirita-ui/package-lock.json
git commit -m "feat(ui): panel security helpers (sanitize + CSS fence) + Panel type"
```

---

### Task 2: `PanelView.vue` (Shadow DOM + morphdom bindings)

**Files:**
- Create: `shirita-ui/src/components/PanelView.vue`
- Test: `shirita-ui/src/components/PanelView.test.ts`
- Modify: `shirita-ui/package.json` (add `morphdom`, `@types/morphdom`)

**Interfaces:**
- Consumes: `sanitizePanelHtml`, `fenceCss` (Task 1); `morphdom`.
- Produces: component `PanelView` with props `{ html: string; css: string; values: Record<string, unknown> }`, rendering into a Shadow DOM on a `data-test="panel-host"` div. (Plan 3 adds an `action` emit + the chat host; Plan 4 uses it for the editor preview.)

- [ ] **Step 1: Install morphdom**

Run: `npm --prefix shirita-ui install morphdom && npm --prefix shirita-ui install -D @types/morphdom`

- [ ] **Step 2: Write the failing tests**

Create `shirita-ui/src/components/PanelView.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { nextTick } from 'vue'
import { mount } from '@vue/test-utils'
import PanelView from './PanelView.vue'

function shadowOf(w: ReturnType<typeof mount>): ShadowRoot {
  return (w.element as HTMLElement).shadowRoot as ShadowRoot
}

describe('PanelView', () => {
  it('renders sanitized html bound to values inside a shadow root', async () => {
    const w = mount(PanelView, { props: { html: '<span data-bind="hp">x</span>', css: '', values: { hp: 100 } } })
    await nextTick()
    const sr = shadowOf(w)
    expect(sr).toBeTruthy()
    expect(sr.querySelector('[data-bind="hp"]')!.textContent).toBe('100')
  })

  it('interpolates {{var}} and updates on a values change', async () => {
    const w = mount(PanelView, { props: { html: '<p>HP: {{hp}}</p>', css: '', values: { hp: 100 } } })
    await nextTick()
    const sr = shadowOf(w)
    expect(sr.querySelector('p')!.textContent).toContain('100')
    await w.setProps({ values: { hp: 90 } })
    await nextTick()
    expect(sr.querySelector('p')!.textContent).toContain('90')
  })

  it('hides data-show elements when the var is falsy', async () => {
    const w = mount(PanelView, { props: { html: '<span data-show="poisoned">poison</span>', css: '', values: { poisoned: false } } })
    await nextTick()
    const el = shadowOf(w).querySelector('[data-show]') as HTMLElement
    expect(el.style.display).toBe('none')
    await w.setProps({ values: { poisoned: true } })
    await nextTick()
    expect((shadowOf(w).querySelector('[data-show]') as HTMLElement).style.display).toBe('')
  })

  it('preserves an opened <details> across a value change (morph, not rebuild)', async () => {
    const w = mount(PanelView, {
      props: { html: '<details><summary>s</summary><span data-bind="hp">x</span></details>', css: '', values: { hp: 1 } },
    })
    await nextTick()
    const sr = shadowOf(w)
    ;(sr.querySelector('details') as HTMLDetailsElement).open = true // user opens it
    await w.setProps({ values: { hp: 2 } })
    await nextTick()
    expect((sr.querySelector('details') as HTMLDetailsElement).open).toBe(true) // still open
    expect(sr.querySelector('[data-bind="hp"]')!.textContent).toBe('2')         // and updated
  })

  it('forces containment styles on the host', () => {
    const w = mount(PanelView, { props: { html: '', css: '', values: {} } })
    const style = (w.element as HTMLElement).getAttribute('style') || ''
    expect(style).toContain('position: relative')
    expect(style).toContain('overflow: hidden')
    expect(style).toContain('contain: content')
  })
})
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `npm --prefix shirita-ui test -- PanelView`
Expected: FAIL — `PanelView.vue` doesn't exist yet.

- [ ] **Step 4: Implement `PanelView.vue`**

Create `shirita-ui/src/components/PanelView.vue`:

```vue
<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import morphdom from 'morphdom'
import { sanitizePanelHtml, fenceCss } from '../utils/panel'

const props = defineProps<{
  html: string
  css: string
  values: Record<string, unknown>
}>()

const host = ref<HTMLDivElement | null>(null)
let shadow: ShadowRoot | null = null
let styleEl: HTMLStyleElement | null = null
let template: HTMLElement | null = null // parsed, sanitized, inline-fenced (no bindings applied)

// Parse the author HTML once per html change: sanitize, then fence inline styles.
function parseTemplate() {
  const root = document.createElement('div')
  root.setAttribute('data-panel-root', '')
  root.innerHTML = sanitizePanelHtml(props.html)
  root.querySelectorAll<HTMLElement>('[style]').forEach((el) => {
    el.setAttribute('style', fenceCss(el.getAttribute('style') || ''))
  })
  template = root
}

// Build a target tree from the template + current values, then morph the live tree.
function render() {
  if (!shadow || !template) return
  const target = template.cloneNode(true) as HTMLElement

  // {{var}} text interpolation (textContent assignment auto-escapes — no injection).
  const walker = document.createTreeWalker(target, NodeFilter.SHOW_TEXT)
  const texts: Text[] = []
  for (let n = walker.nextNode(); n; n = walker.nextNode()) texts.push(n as Text)
  for (const t of texts) {
    if (t.nodeValue && t.nodeValue.includes('{{')) {
      t.nodeValue = t.nodeValue.replace(/\{\{\s*(\w+)\s*\}\}/g, (_, k) => String(props.values[k] ?? ''))
    }
  }
  // data-bind → textContent; data-show → display.
  target.querySelectorAll<HTMLElement>('[data-bind]').forEach((el) => {
    el.textContent = String(props.values[el.getAttribute('data-bind')!] ?? '')
  })
  target.querySelectorAll<HTMLElement>('[data-show]').forEach((el) => {
    el.style.display = props.values[el.getAttribute('data-show')!] ? '' : 'none'
  })

  const live = shadow.querySelector('[data-panel-root]') as HTMLElement | null
  if (!live) {
    shadow.appendChild(target) // first paint
  } else {
    morphdom(live, target, {
      onBeforeElUpdated(fromEl, toEl) {
        // preserve user-toggled disclosure widgets the variables don't own
        if (fromEl instanceof HTMLDetailsElement && toEl instanceof HTMLDetailsElement) {
          toEl.open = fromEl.open
        }
        return true
      },
    })
  }
}

function applyCss() {
  if (styleEl) styleEl.textContent = fenceCss(props.css)
}

onMounted(() => {
  if (!host.value) return
  shadow = host.value.attachShadow({ mode: 'open' })
  styleEl = document.createElement('style')
  shadow.appendChild(styleEl)
  parseTemplate()
  applyCss()
  render()
})

watch(() => props.html, () => { parseTemplate(); render() })
watch(() => props.css, applyCss)
watch(() => props.values, render, { deep: true })
</script>

<template>
  <div ref="host" data-test="panel-host" style="position: relative; overflow: hidden; contain: content;" />
</template>
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `npm --prefix shirita-ui test -- PanelView`
Expected: PASS — render/binding, `{{var}}` update, `data-show` toggle, `<details>` preservation, host containment.

- [ ] **Step 6: Typecheck/build**

Run: `npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: clean — `dompurify`/`morphdom` resolve with types, no unused symbols.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/components/PanelView.vue shirita-ui/src/components/PanelView.test.ts shirita-ui/package.json shirita-ui/package-lock.json
git commit -m "feat(ui): PanelView — shadow DOM + morphdom variable bindings"
```

---

## Final Verification

- [ ] **UI test + build sweep**

Run: `npm --prefix shirita-ui test -- panel PanelView 2>&1 | tail -10 && npm --prefix shirita-ui run build 2>&1 | tail -6`
Expected: all panel + PanelView tests pass; build clean.

---

## Self-Review

**Spec coverage (spec §4):**
- Shadow root on a containment-forced host `<div>` (`position:relative; overflow:hidden; contain:content`) — Task 2 (`forces containment styles` test).
- Sanitize author HTML (DOMPurify allowlist, no script/remote/js URLs) — Task 1.
- CSS fence (`@import`, `position:fixed|sticky`, remote `url()`, `expression()`, `behavior`) on the css field and inline styles — Task 1 (`fenceCss`) + Task 2 (inline-style pass in `parseTemplate`).
- Bindings `{{var}}` / `data-bind` / `data-show`, reactive — Task 2.
- morphdom morph (never `innerHTML` replace) with `<details open>` preservation — Task 2 (`preserves an opened <details>` test).
- Local-only images — Task 1 (`ALLOWED_URI_REGEXP`, `drops remote img src` test).
- `pack.meta.panel` typing — Task 1 (`Panel` / `PanelCaps`).

**Out of this plan (later):** `data-diff-*` / `data-insert` / `data-send` action wiring + the `action` emit, and `ChatView` placement — Plan 3. The editor preview reuse — Plan 4. (The sanitizer already preserves the action attributes so Plan 3 only adds the listener.)

**Placeholder scan:** none — full helper + component code, complete test code, exact install/test/build commands.

**Type consistency:** `sanitizePanelHtml(string): string` and `fenceCss(string): string` are used in `PanelView` exactly as defined. `PanelView` props `{ html, css, values }` match the test mounts. `Panel { html, css, caps: PanelCaps }` matches the spec's `pack.meta.panel` shape and is the type Plans 3/4 will read off `pack.meta`. `values: Record<string, unknown>` matches `SessionState.values`.
