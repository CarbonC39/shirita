# Frontend Render Fixes + Global Regex Pollution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the `HtmlCardFrame` height-plateau regression, exempt HTML cards from the chat-bubble width cap, add a per-pack panel-visibility message threshold, and replace the "unreferenced = global" regex-rule inference with an explicit `is_global` flag.

**Architecture:** Four independent fixes (Vue: Tasks 1-3; Rust+Vue: Tasks 4-6), each already prototyped and verified against the real test suites before being written into this plan — every task's code below is the exact validated version. Task 7 is a final cross-cutting regression pass.

**Tech Stack:** Vue 3 `<script setup>` (`shirita-ui`), Vitest + `@vue/test-utils`, Rust (`shirita-core`, `shirita-web`, `shirita-tauri`), `cargo test`.

## Global Constraints

- Fix A (Task 1) must not change the `MIN_HEIGHT`/`MAX_HEIGHT` clamps on *reported* heights — only the pre-report default.
- Fix B (Task 2) must not affect `PanelView` (already full-width, unaffected by the bubble cap) — only `HtmlCardFrame`-rendering messages.
- Fix C (Task 3) defaults to `min_messages: 0` (today's always-show behavior) when absent — no behavior change for existing packs until an author opts in.
- Fix D (Tasks 4-6) must preserve current behavior for every existing currently-unreferenced `regex_rule` (the `ensure_global_regex_flag` migration backfills `is_global: true` for all of them) — going forward, a newly-unreferenced rule without `is_global` set no longer applies anywhere.
- No change to `shirita-ui/src/utils/panel.ts` or `PanelView.vue`'s internal `data-show` mechanism.

---

### Task 1: HtmlCardFrame height-plateau fix

**Files:**
- Modify: `shirita-ui/src/components/HtmlCardFrame.vue`
- Test: `shirita-ui/src/components/HtmlCardFrame.test.ts`

**Interfaces:** No props/emits change — internal-only behavior change. `MarkdownText.vue` (the only consumer) needs no changes.

- [ ] **Step 1: Update the test expectations (the failing assertions)**

In `shirita-ui/src/components/HtmlCardFrame.test.ts`, change every occurrence of `'640px'` to `'80px'` (four places), and rename the first test:

```ts
describe('HtmlCardFrame', () => {
  it('starts at the MIN_HEIGHT floor before any size report', () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('80px')
  })

  it('resizes to a reported height carrying its own token', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    postReport({ source: 'shirita-html-card', token: tokenOf(w), height: 1200 })
    await w.vm.$nextTick()
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('1200px')
  })

  it('ignores a message carrying a different token', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    postReport({ source: 'shirita-html-card', token: 'not-the-real-token', height: 1200 })
    await w.vm.$nextTick()
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('80px')
  })

  it('ignores a message with a different source tag', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    postReport({ source: 'something-else', token: tokenOf(w), height: 1200 })
    await w.vm.$nextTick()
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('80px')
  })

  it('clamps a too-small reported height up to 80px', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    postReport({ source: 'shirita-html-card', token: tokenOf(w), height: 50 })
    await w.vm.$nextTick()
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('80px')
  })

  it('clamps a too-large reported height down to 4000px', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    postReport({ source: 'shirita-html-card', token: tokenOf(w), height: 9000 })
    await w.vm.$nextTick()
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('4000px')
  })

  it('removes the message listener on unmount', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    const token = tokenOf(w)
    w.unmount()
    expect(() => postReport({ source: 'shirita-html-card', token, height: 1200 })).not.toThrow()
  })

  it('two instances do not cross-react to each other’s reports', async () => {
    const a = mount(HtmlCardFrame, { props: { html: '<p>a</p>' } })
    const b = mount(HtmlCardFrame, { props: { html: '<p>b</p>' } })
    postReport({ source: 'shirita-html-card', token: tokenOf(a), height: 999 })
    await a.vm.$nextTick()
    await b.vm.$nextTick()
    expect((a.find('iframe').element as HTMLIFrameElement).style.height).toBe('999px')
    expect((b.find('iframe').element as HTMLIFrameElement).style.height).toBe('80px')
  })
})
```

(`tokenOf` and `postReport` helpers at the top of the file are unchanged.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/HtmlCardFrame.test.ts`

Expected: FAIL — the current component still defaults to 640px, so the first, third, fourth, and last assertions (now expecting `80px`) fail.

- [ ] **Step 3: Implement the fix**

In `shirita-ui/src/components/HtmlCardFrame.vue`, find:

```ts
const DEFAULT_HEIGHT = 640
const MIN_HEIGHT = 80
const MAX_HEIGHT = 4000
const height = ref(DEFAULT_HEIGHT)
```

Replace it with:

```ts
// Starting small (not a guessed "typical" height like the old 640px default)
// is deliberate: scrollHeight is defined as max(box height, content height),
// so a box that starts TALLER than the eventual content can never report
// smaller than its own starting height — it plateaus. A card using vh/%-based
// sizing (common for flex-centered layouts) resolves against whatever height
// the iframe currently has; starting at the small MIN_HEIGHT floor instead of
// a large guess means the box is virtually always ≤ content, so scrollHeight
// naturally grows to match content from below with no plateau risk.
const MIN_HEIGHT = 80
const MAX_HEIGHT = 4000
const height = ref(MIN_HEIGHT)
```

Nothing else in the file changes (the `resizeScript` measurement formula, the `onMessage` clamp logic, and the template are all unchanged).

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/HtmlCardFrame.test.ts`

Expected: `Test Files  1 passed (1)`, `Tests  8 passed (8)`.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/HtmlCardFrame.vue shirita-ui/src/components/HtmlCardFrame.test.ts
git commit -m "$(cat <<'EOF'
fix(html-card-frame): start the iframe small instead of a 640px default to avoid a height plateau

scrollHeight is defined as max(box height, content height): starting the
iframe at a confident-looking 640px default meant any card using vh/%-based
sizing (common for flex-centered layouts) resolved its own height against
that box, and once the box was taller than the actual content, scrollHeight
could never report smaller — a permanent plateau, not a transient overshoot.
Confirmed via devtools on examples/怪谈社.json: stuck at ~638px with visible
content filling only about two-thirds of the box, the rest rendering as a
blank/black area.

Starting at the existing MIN_HEIGHT floor (80px) instead is self-correcting:
a box that starts at or below content size lets scrollHeight grow to match
content from below, with no plateau risk.
EOF
)"
```

---

### Task 2: Bubble width cap exempts HTML cards

**Files:**
- Modify: `shirita-ui/src/utils/markdown.ts`
- Modify: `shirita-ui/src/components/MessageItem.vue`
- Test: `shirita-ui/src/utils/markdown.test.ts`
- Test: `shirita-ui/src/components/MessageItem.test.ts`

**Interfaces:**
- Produces: new export `containsHtmlCard(text: string): boolean` from `shirita-ui/src/utils/markdown.ts`, used by `MessageItem.vue`.
- Consumes: existing exports `isHtmlDocument` and `parseMarkdown` from the same file (both already exported, unchanged signatures).

- [ ] **Step 1: Write the failing tests**

Add to `shirita-ui/src/utils/markdown.test.ts`, changing the import line and appending a new `describe` block at the end of the file:

```ts
import { describe, it, expect } from 'vitest'
import { parseMarkdown, containsHtmlCard } from './markdown'
```

```ts
describe('containsHtmlCard', () => {
  it('is true for a raw HTML document', () => {
    expect(containsHtmlCard('<!DOCTYPE html><html><body>x</body></html>')).toBe(true)
  })

  it('is true for a fenced ```html code block', () => {
    expect(containsHtmlCard('before\n```html\n<div>x</div>\n```\nafter')).toBe(true)
  })

  it('is true for an unlabeled fenced block whose content looks like a document', () => {
    expect(containsHtmlCard('```\n<!doctype html><html></html>\n```')).toBe(true)
  })

  it('is false for plain text and non-html fenced code', () => {
    expect(containsHtmlCard('just text')).toBe(false)
    expect(containsHtmlCard('```js\nconst x = 1\n```')).toBe(false)
  })
})
```

Add to `shirita-ui/src/components/MessageItem.test.ts`, inside the existing `describe('MessageItem', () => { ... })` block, directly after the `'edits in place and emits edit-save'` test (the last test in that block, right before its closing `})`):

```ts
  it('caps plain-text bubbles at 78% width', () => {
    const w = mount(MessageItem, {
      props: { message: makeMsg({ raw_content: 'just text' }), style: 'bubble' },
    })
    expect(w.find('[data-test="msg-bubble-wrapper"]').classes()).toContain('max-w-[78%]')
  })

  it('lets an HTML card bubble use the full message-row width', () => {
    const w = mount(MessageItem, {
      props: { message: makeMsg({ raw_content: '<!DOCTYPE html><html><body>card</body></html>' }), style: 'bubble' },
    })
    const wrapper = w.find('[data-test="msg-bubble-wrapper"]')
    expect(wrapper.classes()).toContain('max-w-full')
    expect(wrapper.classes()).not.toContain('max-w-[78%]')
  })
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd shirita-ui && npx vitest run src/utils/markdown.test.ts src/components/MessageItem.test.ts`

Expected: FAIL — `containsHtmlCard` doesn't exist yet (import error in `markdown.test.ts`), and `[data-test="msg-bubble-wrapper"]` doesn't exist yet in the rendered DOM (no matching element, `classes()` on an empty wrapper fails).

- [ ] **Step 3: Implement the fix**

In `shirita-ui/src/utils/markdown.ts`, directly after the existing `isHtmlDocument` function (after its closing `}`, before the `safeHref` function), add:

```ts
/** True if `text`, rendered as Markdown, would produce at least one HtmlCardFrame
 *  (a raw HTML document, or a fenced ```html / detected-HTML code block). Mirrors
 *  MarkdownText.vue's own rendering decision — used by MessageItem.vue to decide
 *  whether the chat-bubble width cap should apply (cards want full width, plain
 *  text bubbles don't). */
export function containsHtmlCard(text: string): boolean {
  if (isHtmlDocument(text)) return true
  return parseMarkdown(text).some((n) => n.type === 'codeblock' && (n.lang === 'html' || isHtmlDocument(n.value)))
}
```

In `shirita-ui/src/components/MessageItem.vue`, add the import directly after the existing `MessageContent` import:

```ts
import MessageContent from './MessageContent.vue'
import { containsHtmlCard } from '../utils/markdown'
```

Add a new computed directly after the existing `displayText` computed:

```ts
const displayText = computed(() => props.message.display_content ?? props.message.raw_content)
// HTML cards are rich, self-contained content meant to use the full message-row
// width — the 78% cap below is a chat-bubble-text aesthetic that squeezes them.
const isCard = computed(() => containsHtmlCard(displayText.value))
```

In the template, find the bubble-mode wrapper div (the one with `:class="['max-w-[78%]', isUser ? 'order-first' : '']"`, directly after the `assistant-avatar` div) and change it to:

```html
<div data-test="msg-bubble-wrapper" :class="[isCard ? 'max-w-full' : 'max-w-[78%]', isUser ? 'order-first' : '']">
```

(Only this one wrapper, in the `style === 'bubble'` branch of the template — the `style === 'flat'` branch further down has no width cap to begin with and is untouched.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd shirita-ui && npx vitest run src/utils/markdown.test.ts src/components/MessageItem.test.ts`

Expected: `Test Files  2 passed (2)`, `Tests  34 passed (34)`.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/utils/markdown.ts shirita-ui/src/utils/markdown.test.ts \
        shirita-ui/src/components/MessageItem.vue shirita-ui/src/components/MessageItem.test.ts
git commit -m "$(cat <<'EOF'
fix(message-item): let HTML cards use the full message-row width

MessageItem.vue's bubble wrapper capped every message at max-w-[78%] — the
right call for chat-bubble text, but wrong for an embedded HTML card (an
already-narrow column squeezed further compounds the card's own layout
assumptions, e.g. designs built around ~520px+ widths). containsHtmlCard()
reuses MarkdownText.vue's own HTML-detection logic so the two stay in sync.
EOF
)"
```

---

### Task 3: Per-pack panel visibility threshold

**Files:**
- Modify: `shirita-ui/src/api/types.ts`
- Modify: `shirita-ui/src/components/PackEditor.vue`
- Modify: `shirita-ui/src/views/ChatView.vue`
- Modify: `shirita-ui/src/locales/en.ts`, `shirita-ui/src/locales/zh-Hans.ts`, `shirita-ui/src/locales/zh-Hant.ts`, `shirita-ui/src/locales/ja.ts`
- Test: `shirita-ui/src/components/PackEditor.test.ts`
- Test: `shirita-ui/src/views/ChatView.test.ts`

**Interfaces:**
- Produces: `Panel.min_messages?: number` on the `Panel` interface (`shirita-ui/src/api/types.ts`).
- Consumes: existing `Panel`/`PanelCaps` types, existing `panelOf(p): Panel` helper and `panelPacks` ref in `ChatView.vue`, existing `chat.messages` (a `ref<Message[]>`) from the chat store.

- [ ] **Step 1: Write the failing tests**

In `shirita-ui/src/components/PackEditor.test.ts`, change the "editing the panel HTML saves meta.panel" test's expected object (it will start failing once Step 3 adds `min_messages` to every saved panel object):

```ts
  it('editing the panel HTML saves meta.panel', async () => {
    const w = mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    const ta = w.find('[data-test="panel-html"]')
    await ta.setValue('<b>{{hp}}</b>')
    await ta.trigger('change')
    await flushPromises()
    expect(api.updatePack).toHaveBeenCalledWith('p1', expect.objectContaining({
      meta: expect.objectContaining({ panel: { html: '<b>{{hp}}</b>', css: '', caps: {}, min_messages: 0 } }),
    }))
  })
```

Add a new test directly after the existing "toggling a capability saves it" test:

```ts
  it('seeds the min-messages threshold from meta.panel and saves edits to it', async () => {
    const withPanel = { ...pack, meta: { panel: { html: '', css: '', caps: {}, min_messages: 3 } } }
    const w = mount(PackEditor, { props: { pack: withPanel }, global: { stubs } })
    await flushPromises()
    expect((w.find('[data-test="panel-min-messages"]').element as HTMLInputElement).value).toBe('3')

    const input = w.find('[data-test="panel-min-messages"]')
    await input.setValue('5')
    await input.trigger('change')
    await flushPromises()
    expect(api.updatePack).toHaveBeenCalledWith('p1', expect.objectContaining({
      meta: expect.objectContaining({ panel: expect.objectContaining({ min_messages: 5 }) }),
    }))
  })
```

In `shirita-ui/src/views/ChatView.test.ts`, add two new tests directly after the existing `'renders a panel for each mounted pack that has a panel'` test:

```ts
  it('hides a panel until the chat reaches its min_messages threshold', async () => {
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's1', active_leaf_id: null, mounted_packs: ['p1'] } as never)
    vi.spyOn(client, 'getPack').mockResolvedValue({
      id: 'p1', name: 'Alice', identity: { display_name: null, avatar: null },
      meta: { panel: { html: '<span>x</span>', css: '', caps: {}, min_messages: 2 } },
      created_at: '', updated_at: '',
    } as never)
    const oneMessage = [{
      id: 'm1', session_id: 's1', parent_id: null, role: 'user' as const,
      raw_content: 'hi', display_content: null, is_hidden: false, is_anchor: false, attachments: [],
      snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    }]
    vi.spyOn(client, 'listMessages').mockResolvedValue(oneMessage)
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const w = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(w.find('[data-test="panel-stack"]').exists()).toBe(false)
  })

  it('shows a panel once the chat reaches its min_messages threshold', async () => {
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's1', active_leaf_id: null, mounted_packs: ['p1'] } as never)
    vi.spyOn(client, 'getPack').mockResolvedValue({
      id: 'p1', name: 'Alice', identity: { display_name: null, avatar: null },
      meta: { panel: { html: '<span>x</span>', css: '', caps: {}, min_messages: 2 } },
      created_at: '', updated_at: '',
    } as never)
    const twoMessages = [0, 1].map((i) => ({
      id: `m${i}`, session_id: 's1', parent_id: null, role: 'user' as const,
      raw_content: 'hi', display_content: null, is_hidden: false, is_anchor: false, attachments: [],
      snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    }))
    vi.spyOn(client, 'listMessages').mockResolvedValue(twoMessages)
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const w = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(w.find('[data-test="panel-stack"]').exists()).toBe(true)
  })
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd shirita-ui && npx vitest run src/components/PackEditor.test.ts src/views/ChatView.test.ts`

Expected: FAIL — the `PackEditor.test.ts` "editing the panel HTML" test fails (current saved object has no `min_messages` key), the new `panel-min-messages` test fails (no such element exists yet), and the new `ChatView.test.ts` "hides a panel" test fails (the panel currently always shows regardless of message count — `panel-stack` exists when it shouldn't).

- [ ] **Step 3: Implement the fix**

In `shirita-ui/src/api/types.ts`, find the `Panel` interface and add the new optional field:

```ts
export interface Panel {
  html: string
  css: string
  caps: PanelCaps
  /** Hide the panel header until the chat has at least this many messages.
   *  Absent/0 = always show (default). */
  min_messages?: number
}
```

In `shirita-ui/src/components/PackEditor.vue`, find:

```ts
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
```

Replace it with:

```ts
const panelHtml = ref('')
const panelCss = ref('')
const panelCaps = ref<PanelCaps>({})
const panelMinMessages = ref(0)
watch(
  () => props.pack.id,
  () => {
    const p = (props.pack.meta as { panel?: Panel }).panel
    panelHtml.value = p?.html ?? ''
    panelCss.value = p?.css ?? ''
    panelCaps.value = { ...(p?.caps ?? {}) }
    panelMinMessages.value = p?.min_messages ?? 0
  },
  { immediate: true },
)
function savePanel() {
  void save({
    meta: {
      ...(props.pack.meta as Record<string, unknown>),
      panel: {
        html: panelHtml.value,
        css: panelCss.value,
        caps: panelCaps.value,
        min_messages: panelMinMessages.value,
      },
    },
  })
}
```

In the same file's template, find the closing `</div>` of the caps checkboxes block (directly after the `data-test="cap-send"` `<label>`, before the `<div>` containing the panel preview) and insert a new field between them:

```html
        <label class="flex items-center gap-1.5"><input type="checkbox" data-test="cap-send" :checked="panelCaps.send" @change="toggleCap('send')" />{{ $t('pack.capSend') }}</label>
      </div>
      <label class="flex items-center gap-2 text-[12px]">
        <span class="text-muted">{{ $t('pack.panelMinMessages') }}</span>
        <input
          type="number"
          min="0"
          data-test="panel-min-messages"
          v-model.number="panelMinMessages"
          class="field w-20 !py-1"
          @change="savePanel"
        />
      </label>
      <div>
        <span class="text-[12px] text-muted block mb-1">{{ $t('pack.panelPreview') }}</span>
```

In `shirita-ui/src/views/ChatView.vue`, find:

```ts
const panelPacks = ref<Pack[]>([])
function panelOf(p: Pack): Panel {
  return (p.meta as { panel: Panel }).panel
}
async function loadPanels() {
```

Replace it with:

```ts
const panelPacks = ref<Pack[]>([])
function panelOf(p: Pack): Panel {
  return (p.meta as { panel: Panel }).panel
}
// Filters out panels whose author set a min_messages threshold the chat
// hasn't reached yet (e.g. a card whose first_mes is an intro/menu screen
// with no meaningful stats to show). chat.messages, not chat.displayed, so
// this tracks the real conversation length regardless of branch/hidden state.
const visiblePanelPacks = computed(() =>
  panelPacks.value.filter((p) => chat.messages.length >= (panelOf(p).min_messages ?? 0)),
)
async function loadPanels() {
```

In the same file's template, find:

```html
    <div v-if="panelPacks.length" data-test="panel-stack" class="flex flex-col gap-2 py-2">
      <details v-for="p in panelPacks" :key="p.id" open class="rounded-xl border border-line bg-card/50 overflow-hidden">
```

Replace with:

```html
    <div v-if="visiblePanelPacks.length" data-test="panel-stack" class="flex flex-col gap-2 py-2">
      <details v-for="p in visiblePanelPacks" :key="p.id" open class="rounded-xl border border-line bg-card/50 overflow-hidden">
```

Add the i18n key `pack.panelMinMessages` to all four locale files, directly after the existing `capSend` key in each (same `pack` block that already holds `panelHtml`/`panelCss`/`panelCaps`/`capWrite`/etc.):

`shirita-ui/src/locales/en.ts`:
```ts
    capSend: 'send',
    panelMinMessages: 'Show panel after N messages',
    panelPreview: 'Preview',
```

`shirita-ui/src/locales/zh-Hans.ts`:
```ts
    capSend: '发送',
    panelMinMessages: '满 N 条消息后显示面板',
    panelPreview: '预览',
```

`shirita-ui/src/locales/zh-Hant.ts`:
```ts
    capSend: '傳送',
    panelMinMessages: '滿 N 條消息後顯示面板',
    panelPreview: '預覽',
```

`shirita-ui/src/locales/ja.ts`:
```ts
    capSend: '送信',
    panelMinMessages: 'N件のメッセージ後にパネルを表示',
    panelPreview: 'プレビュー',
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd shirita-ui && npx vitest run src/components/PackEditor.test.ts src/views/ChatView.test.ts src/locales/parity.test.ts`

Expected: all pass — `PackEditor.test.ts` 8 tests, `ChatView.test.ts` 13 tests, `parity.test.ts` 3 tests (confirms the new key was added to all four locale files in parity, not just `en.ts`).

- [ ] **Step 5: Run the full frontend test suite to check for regressions**

Run: `cd shirita-ui && npx vitest run`

Expected: `Test Files  40 passed (40)`, `Tests  278 passed (278)`.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/components/PackEditor.vue shirita-ui/src/components/PackEditor.test.ts \
        shirita-ui/src/views/ChatView.vue shirita-ui/src/views/ChatView.test.ts \
        shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts
git commit -m "$(cat <<'EOF'
feat(panel): let a pack author hide its status panel until the chat has progressed

ChatView showed every mounted pack's panel header unconditionally the
instant it was mounted, with no way to express "this card's first_mes is an
intro/menu screen with no meaningful stats yet." Panel.min_messages (author-
set per pack in PackEditor, default 0 = always show) gates panel-header
visibility on chat.messages.length, independent of PanelView's own
finer-grained data-show mechanism.
EOF
)"
```

---

### Task 4: `effective_regex_rules` uses `is_global`, not reference-emptiness

**Files:**
- Modify: `shirita-core/src/conversation.rs`
- Modify: `shirita-core/src/storage/mod.rs`

**Interfaces:**
- Consumes: existing `Storage::referenced_definition_ids`, `Storage::list_definitions`, `Definition.meta: serde_json::Value`.
- Produces: no signature changes — `effective_regex_rules(storage: &dyn Storage, session: &Session) -> crate::Result<Vec<Definition>>` is unchanged; only its internal filter logic changes. This is the behavior Task 6's migration and Task 5's route fix both depend on.

- [ ] **Step 1: Update the existing tests to the new `is_global`-driven premise, and write the new regression test**

In `shirita-core/src/conversation.rs`, inside the `#[cfg(test)] mod tests` block, find the `effective_regex_rules_global_plus_scoped` test and change the global rule's meta:

```rust
    #[tokio::test]
    async fn effective_regex_rules_global_plus_scoped() {
        let storage = Arc::new(temp_storage().await);
        // global orphan rule (referenced by no node)
        let mut g = crate::models::definition::Definition::new("regex_rule", "G", "");
        g.meta = serde_json::json!({ "pattern": "g", "replacement": "", "is_global": true });
        storage.create_definition(&g).await.unwrap();
```

(Only that one `g.meta` line changes — the rest of the test, including the `s` scoped-rule setup and both assertions, is unchanged.)

Find `effective_regex_includes_mounted_pack_rules_in_order` and change the global rule's meta the same way:

```rust
        // a global orphan rule (referenced by nothing)
        let mut global = Definition::new("regex_rule", "global", "");
        global.id = "r_global".into();
        global.meta = serde_json::json!({ "pattern": "a", "replacement": "b", "is_global": true });
        storage.create_definition(&global).await.unwrap();
```

Find `converted_panel_capture_folds_into_snapshot_alongside_state_update_tags` and fix its setup to match the real production shape (it currently constructs the capture-vars rule as a true orphan, which is unrealistic — `charcard_to_loreset` always creates a referencing node for every `regex_scripts` entry, including the one chosen for panel conversion):

```rust
        // A regex_rule with capture_vars — same shape `try_convert_status_panel`
        // produces — referenced by the template's tree, same as the real
        // import path (`charcard_to_loreset` always creates a Ref node for
        // every regex_scripts entry, including the one chosen for panel
        // conversion; it is never left as an unreferenced/global rule).
        let mut rule = Definition::new("regex_rule", "status", "");
        rule.meta = serde_json::json!({
            "pattern": "<mood>(\\w+)</mood>",
            "replacement": "$1",
            "capture_vars": ["field1"]
        });
        storage.create_definition(&rule).await.unwrap();
        storage.create_node(&PromptNode::new_ref(OwnerKind::Template, &t.id, None, 0, &rule.id)).await.unwrap();
```

(This adds one new line — the `storage.create_node(...)` call — directly after the existing `storage.create_definition(&rule).await.unwrap();` line, and updates the comment above it. `PromptNode` and `OwnerKind` are already imported at the top of `conversation.rs`.)

Add a new test directly after `effective_regex_rules_global_plus_scoped`'s closing `}` (before `send_chains_under_active_leaf_and_updates_it`):

```rust
    #[tokio::test]
    async fn effective_regex_rules_ignores_unreferenced_rule_without_is_global() {
        // Regression test for the global-regex-pollution fix: an unreferenced
        // regex_rule that never had `is_global` set (e.g. left behind by
        // deleting its owning pack/template without choosing to delete
        // orphans) must NOT silently apply everywhere anymore.
        let storage = Arc::new(temp_storage().await);
        let mut accidental_orphan = crate::models::definition::Definition::new("regex_rule", "Accidental", "");
        accidental_orphan.meta = serde_json::json!({ "pattern": "a", "replacement": "" });
        storage.create_definition(&accidental_orphan).await.unwrap();

        let session = Session::new("x");
        storage.create_session(&session).await.unwrap();
        let rules = super::effective_regex_rules(storage.as_ref(), &session).await.unwrap();
        assert!(rules.is_empty(), "unreferenced rule without is_global must not apply");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shirita-core --lib conversation:: 2>&1 | tail -40`

Expected: `effective_regex_rules_global_plus_scoped`, `effective_regex_includes_mounted_pack_rules_in_order`, `converted_panel_capture_folds_into_snapshot_alongside_state_update_tags`, and the new `effective_regex_rules_ignores_unreferenced_rule_without_is_global` all FAIL (the current implementation still treats *any* unreferenced rule as global, so the new test's `accidental_orphan` wrongly applies, and the three updated tests' fixtures — now carrying `is_global`/a real node reference the old code ignores — don't yet change the production code's behavior).

- [ ] **Step 3: Implement the fix**

In `shirita-core/src/conversation.rs`, find:

```rust
    let referenced: std::collections::HashSet<String> =
        storage.referenced_definition_ids().await?.into_iter().collect();
    let all = storage.list_definitions().await?;
    let mut rules: Vec<Definition> = all
        .iter()
        .filter(|d| d.def_type == "regex_rule" && !referenced.contains(&d.id))
```

Replace it with:

```rust
    let all = storage.list_definitions().await?;
    let mut rules: Vec<Definition> = all
        .iter()
        .filter(|d| {
            d.def_type == "regex_rule" && d.meta.get("is_global").and_then(|v| v.as_bool()).unwrap_or(false)
        })
```

(The `referenced` variable is removed entirely — verify nothing else in the function still references it; it doesn't, the rest of `effective_regex_rules` only uses `all`, `by_id`, and `effective_nodes`/`session.mounted_packs`.)

In `shirita-core/src/storage/mod.rs`, find:

```rust
    /// Distinct `definition_id`s referenced by any prompt node (all owners).
    /// Lets callers tell orphan ("global") defs from tree-mounted ones.
    async fn referenced_definition_ids(&self) -> Result<Vec<String>>;
```

Replace it with:

```rust
    /// Distinct `definition_id`s referenced by any prompt node (all owners).
    /// NOTE: a `regex_rule` Definition with `meta.is_global == true` is, by
    /// design, never referenced by any node — "unreferenced" is NOT the same
    /// as "safe to treat as dead/orphaned" for that one def_type. Any new
    /// code built on this method that reasons about orphans/cleanup must
    /// explicitly exclude `def_type == "regex_rule" && meta.is_global == true`
    /// rows, the way `effective_regex_rules` and `list_regex_scopes` do.
    async fn referenced_definition_ids(&self) -> Result<Vec<String>>;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p shirita-core --lib conversation:: 2>&1 | tail -10`

Expected: `test result: ok. 22 passed; 0 failed`.

- [ ] **Step 5: Run the full shirita-core test suite to check for regressions**

Run: `cargo build -p shirita-core 2>&1 | tail -10 && cargo test -p shirita-core 2>&1 | tail -15`

Expected: clean build (no warnings — `referenced_definition_ids` is a trait method, not flagged as dead code even though `effective_regex_rules` no longer calls it; it's still implemented in `sqlite.rs` and documented for future callers), `test result: ok. 262 passed; 0 failed`.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/conversation.rs shirita-core/src/storage/mod.rs
git commit -m "$(cat <<'EOF'
fix(regex): effective_regex_rules requires an explicit is_global flag, not just zero references

Any regex_rule Definition with zero referencing prompt-tree nodes was
treated as "global" and unconditionally applied to every session. This
conflated two different situations that happen to produce identical DB
state: Settings' "add rule" button deliberately creating a rule with no
node reference (wanted), and deleting a template/pack while choosing not
to delete its orphaned definitions, or unbinding a regex_rule's referencing
node directly (accidental) — the latter then silently started firing on
every other session. is_global is now the only source of truth; reference-
emptiness alone no longer means "applies everywhere."
EOF
)"
```

---

### Task 5: `list_regex_scopes` reports `is_global` and checks Pack-owned nodes too

**Files:**
- Modify: `shirita-web/src/routes/regex_rules.rs`
- Test: `shirita-web/tests/regex_scopes_test.rs`

**Interfaces:** No change to `RegexScope`'s shape (`id`, `scope`, `template_names`, `pattern_error`) or the route's HTTP contract — only the values it computes for `scope`/`template_names` change. Independent of Task 4 (this route calls `Definition.meta` directly, not `effective_regex_rules`).

- [ ] **Step 1: Update the test helper and existing assertions, and write the new Pack-blind-spot regression test**

In `shirita-web/tests/regex_scopes_test.rs`, update the import to bring in `Pack`:

```rust
use shirita_core::{
    Config, Definition, EchoProvider, ModelProvider, OwnerKind, Pack, PromptNode, SqliteStorage,
    Storage, Template, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};
```

Change the `rule` helper to take an explicit `is_global` flag, and update every call site and the comments above them:

```rust
fn rule(name: &str, pattern: &str, is_global: bool) -> Definition {
    let mut d = Definition::new("regex_rule", name, "");
    d.meta = serde_json::json!({ "pattern": pattern, "replacement": "", "is_global": is_global });
    d
}

#[tokio::test]
async fn regex_scopes_reports_scope_sources_and_errors() {
    let state = test_state().await;

    // Explicit global rule (created via Settings' "add rule", no node ref) → global.
    let g = rule("Global", r"\d+", true);
    state.storage.create_definition(&g).await.unwrap();

    // Rule referenced by a template → template-scoped, with the template name.
    let s = rule("Scoped", "foo", false);
    state.storage.create_definition(&s).await.unwrap();
    let tmpl = Template::new("My Card");
    state.storage.create_template(&tmpl).await.unwrap();
    state
        .storage
        .create_node(&PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, 0, &s.id))
        .await
        .unwrap();

    // Explicit global rule with an invalid pattern → global + pattern_error.
    let bad = rule("Bad", "foo(", true);
    state.storage.create_definition(&bad).await.unwrap();
```

(The rest of `regex_scopes_reports_scope_sources_and_errors` — the request/response handling and the three `assert_eq!` blocks at the bottom — is unchanged.)

Add a new test at the end of the file, after `regex_scopes_reports_scope_sources_and_errors`'s closing `}`:

```rust
#[tokio::test]
async fn regex_scopes_attributes_pack_referenced_rules_to_their_pack_not_global() {
    // Regression test for the Pack-blind-spot fix: a rule referenced only by
    // a Pack's node tree (e.g. an imported character card's status-bar regex)
    // must report scope "template" with the pack's name listed — not "global".
    let state = test_state().await;

    let r = rule("PackRule", "foo", false);
    state.storage.create_definition(&r).await.unwrap();
    let pack = Pack::new("Cultist Tracker");
    state.storage.create_pack(&pack).await.unwrap();
    state
        .storage
        .create_node(&PromptNode::new_ref(OwnerKind::Pack, &pack.id, None, 0, &r.id))
        .await
        .unwrap();

    let res = app(state.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/regex-rules/scopes")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let out: Value = serde_json::from_slice(&bytes).unwrap();
    let rj = out.as_array().unwrap().iter().find(|x| x["id"] == r.id).unwrap();
    assert_eq!(rj["scope"], "template");
    assert_eq!(rj["template_names"], serde_json::json!(["Cultist Tracker"]));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shirita-web --test regex_scopes_test --jobs 2 2>&1 | tail -30`

Expected: FAIL — `regex_scopes_reports_scope_sources_and_errors`'s `g`/`bad` rules now carry `is_global: true` but the route still derives scope from reference-emptiness (so this still happens to pass for those two specifically, since they truly are unreferenced — but `s` is also unaffected since it's referenced). The new `regex_scopes_attributes_pack_referenced_rules_to_their_pack_not_global` test FAILS: the route currently only checks `OwnerKind::Template` nodes, so a Pack-only-referenced rule is wrongly reported as `scope: "global"` with empty `template_names`.

- [ ] **Step 3: Implement the fix**

In `shirita-web/src/routes/regex_rules.rs`, find:

```rust
    let defs = state.storage.list_definitions().await.map_err(err)?;
    let templates = state.storage.list_templates().await.map_err(err)?;

    // def_id -> ordered unique template names referencing it
    let mut refs: HashMap<String, Vec<String>> = HashMap::new();
    for t in &templates {
        let nodes = state.storage.list_nodes(&OwnerKind::Template, &t.id).await.map_err(err)?;
        for n in nodes {
            if let Some(did) = n.definition_id {
                let names = refs.entry(did).or_default();
                if !names.contains(&t.name) {
                    names.push(t.name.clone());
                }
            }
        }
    }

    let out = defs
        .iter()
        .filter(|d| d.def_type == "regex_rule")
        .map(|d| {
            let names = refs.get(&d.id).cloned().unwrap_or_default();
            let scope = if names.is_empty() { "global" } else { "template" };
            let pattern = d.meta.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            RegexScope {
                id: d.id.clone(),
                scope: scope.to_string(),
                template_names: names,
                pattern_error: shirita_core::regex_error(pattern),
            }
        })
        .collect();
    Ok(Json(out))
```

Replace it with:

```rust
    let defs = state.storage.list_definitions().await.map_err(err)?;
    let templates = state.storage.list_templates().await.map_err(err)?;
    let packs = state.storage.list_packs().await.map_err(err)?;

    // def_id -> ordered unique template/pack names referencing it. Both
    // owner kinds are checked — a rule referenced only by a Pack's node tree
    // (e.g. an imported character card's status-bar regex) is template-scoped
    // in the real sense (effective_regex_rules only applies it when that pack
    // is mounted), not global; it must not be mislabeled here.
    let mut refs: HashMap<String, Vec<String>> = HashMap::new();
    for t in &templates {
        let nodes = state.storage.list_nodes(&OwnerKind::Template, &t.id).await.map_err(err)?;
        for n in nodes {
            if let Some(did) = n.definition_id {
                let names = refs.entry(did).or_default();
                if !names.contains(&t.name) {
                    names.push(t.name.clone());
                }
            }
        }
    }
    for p in &packs {
        let nodes = state.storage.list_nodes(&OwnerKind::Pack, &p.id).await.map_err(err)?;
        for n in nodes {
            if let Some(did) = n.definition_id {
                let names = refs.entry(did).or_default();
                if !names.contains(&p.name) {
                    names.push(p.name.clone());
                }
            }
        }
    }

    let out = defs
        .iter()
        .filter(|d| d.def_type == "regex_rule")
        .map(|d| {
            let names = refs.get(&d.id).cloned().unwrap_or_default();
            let is_global = d.meta.get("is_global").and_then(|v| v.as_bool()).unwrap_or(false);
            let scope = if is_global { "global" } else { "template" };
            let pattern = d.meta.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            RegexScope {
                id: d.id.clone(),
                scope: scope.to_string(),
                template_names: names,
                pattern_error: shirita_core::regex_error(pattern),
            }
        })
        .collect();
    Ok(Json(out))
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p shirita-web --test regex_scopes_test --jobs 2 2>&1 | tail -15`

Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add shirita-web/src/routes/regex_rules.rs shirita-web/tests/regex_scopes_test.rs
git commit -m "$(cat <<'EOF'
fix(regex-scopes): drive scope from is_global, and check Pack-owned nodes too

list_regex_scopes (the Settings UI's data source) only ever checked
Template-owned nodes when computing which template/pack names reference a
rule — a rule referenced only by a Pack's node tree (e.g. an imported
character card's status-bar regex) was mislabeled "global" in Settings even
though effective_regex_rules correctly treats it as pack-scoped. Also
switches scope derivation from "has zero references" to the explicit
is_global flag, matching the conversation.rs fix.
EOF
)"
```

---

### Task 6: `is_global` migration, Settings creation flag, and startup wiring

**Files:**
- Modify: `shirita-core/src/seed.rs`
- Modify: `shirita-core/src/lib.rs`
- Modify: `shirita-web/src/main.rs`
- Modify: `shirita-tauri/src/main.rs`
- Modify: `shirita-ui/src/views/SettingsView.vue`
- Modify (test fixtures only, no behavior change to the tests' subject): `shirita-web/tests/regex_display_test.rs`

**Interfaces:**
- Produces: `pub async fn ensure_global_regex_flag<S: Storage + ?Sized>(storage: &S) -> Result<()>`, exported from `shirita-core` (re-exported in `lib.rs` alongside the other `ensure_*` functions).
- Consumes: `Storage::referenced_definition_ids`, `Storage::list_definitions`, `Storage::update_definition` (all pre-existing). Depends on Task 4's filter change being in place for the migration to matter, but is independently testable on its own (the migration itself doesn't call `effective_regex_rules`).

- [ ] **Step 1: Write the failing test**

In `shirita-core/src/seed.rs`, inside the `#[cfg(test)] mod tests` block, add a new test directly after `ensure_asset_hashes_backfills_missing`'s closing `}` (before the module's final closing `}`):

```rust
    #[tokio::test]
    async fn ensure_global_regex_flag_backfills_unreferenced_rules_without_clobbering_explicit_false() {
        let storage = mem_storage().await;

        // No is_global key at all, unreferenced -> should become true.
        let mut unflagged = Definition::new("regex_rule", "unflagged", "");
        unflagged.meta = serde_json::json!({ "pattern": "a", "replacement": "" });
        storage.create_definition(&unflagged).await.unwrap();

        // Already explicitly false -> must stay false, not be overwritten.
        let mut explicit_false = Definition::new("regex_rule", "explicit-false", "");
        explicit_false.meta = serde_json::json!({ "pattern": "b", "replacement": "", "is_global": false });
        storage.create_definition(&explicit_false).await.unwrap();

        crate::ensure_global_regex_flag(&storage).await.unwrap();
        crate::ensure_global_regex_flag(&storage).await.unwrap(); // idempotent

        let got_unflagged = storage.get_definition(&unflagged.id).await.unwrap().unwrap();
        assert_eq!(got_unflagged.meta["is_global"], true);

        let got_explicit_false = storage.get_definition(&explicit_false.id).await.unwrap().unwrap();
        assert_eq!(got_explicit_false.meta["is_global"], false);
    }
```

(`mem_storage()` is the existing test helper in this same `mod tests` block; `Definition` is already in scope via `use super::*;`.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shirita-core --lib seed:: 2>&1 | tail -15`

Expected: compile error — `ensure_global_regex_flag` doesn't exist yet.

- [ ] **Step 3: Implement the migration function and exports**

In `shirita-core/src/seed.rs`, directly before the existing `ensure_builtin_definitions` function (after `ensure_asset_hashes`'s closing `}`), add:

```rust
/// One-time backfill for the `is_global` regex-rule flag (added after global
/// rules were inferred from "zero references" instead of stored explicitly —
/// see `conversation::effective_regex_rules`). Every `regex_rule` Definition
/// that is currently unreferenced gets `is_global: true` so already-relied-
/// upon global rules keep applying after the flag becomes the source of
/// truth. Idempotent — a rule that already has `is_global` set (true or
/// false) is left untouched, so this never re-flips a rule a user has since
/// explicitly turned off.
pub async fn ensure_global_regex_flag<S: Storage + ?Sized>(storage: &S) -> Result<()> {
    let referenced: std::collections::HashSet<String> =
        storage.referenced_definition_ids().await?.into_iter().collect();
    for def in storage.list_definitions().await? {
        if def.def_type != "regex_rule" {
            continue;
        }
        if def.meta.get("is_global").is_some() {
            continue;
        }
        if referenced.contains(&def.id) {
            continue;
        }
        let mut updated = def.clone();
        if let Some(obj) = updated.meta.as_object_mut() {
            obj.insert("is_global".to_string(), serde_json::json!(true));
        }
        storage.update_definition(&updated).await?;
    }
    Ok(())
}
```

In `shirita-core/src/lib.rs`, find:

```rust
pub use seed::{
    ensure_asset_hashes, ensure_builtin_definitions, ensure_default_template,
    ensure_templates_have_content_node,
};
```

Replace it with:

```rust
pub use seed::{
    ensure_asset_hashes, ensure_builtin_definitions, ensure_default_template,
    ensure_global_regex_flag, ensure_templates_have_content_node,
};
```

In `shirita-web/src/main.rs`, find:

```rust
    // Backfill: legacy templates gain the undeletable <<content>> mount node.
    shirita_core::ensure_templates_have_content_node(&storage).await?;
    shirita_core::ensure_asset_hashes(&storage, &config.assets_dir).await?;
```

Replace it with:

```rust
    // Backfill: legacy templates gain the undeletable <<content>> mount node.
    shirita_core::ensure_templates_have_content_node(&storage).await?;
    // Backfill: unreferenced regex_rule defs that predate the is_global flag.
    shirita_core::ensure_global_regex_flag(&storage).await?;
    shirita_core::ensure_asset_hashes(&storage, &config.assets_dir).await?;
```

In `shirita-tauri/src/main.rs`, find:

```rust
    shirita_core::ensure_templates_have_content_node(&storage)
        .await
        .map_err(|e| format!("迁移模板 content 节点失败：{e}"))?;
    shirita_core::ensure_asset_hashes(&storage, &config.assets_dir)
        .await
        .map_err(|e| format!("资产哈希回填失败：{e}"))?;
```

Replace it with:

```rust
    shirita_core::ensure_templates_have_content_node(&storage)
        .await
        .map_err(|e| format!("迁移模板 content 节点失败：{e}"))?;
    shirita_core::ensure_global_regex_flag(&storage)
        .await
        .map_err(|e| format!("regex 全局标记回填失败：{e}"))?;
    shirita_core::ensure_asset_hashes(&storage, &config.assets_dir)
        .await
        .map_err(|e| format!("资产哈希回填失败：{e}"))?;
```

In `shirita-ui/src/views/SettingsView.vue`, find the "add rule" button's `createDefinition` call:

```ts
                            const created = await createDefinition({
                                type: 'regex_rule',
                                name: 'New rule',
                                content: '',
                                meta: {
                                    pattern: '',
                                    replacement: '',
                                    disabled: false,
                                    scope: 'display',
                                    targets: ['ai_output'],
                                },
                            });
```

Replace the `meta` object with:

```ts
                                meta: {
                                    pattern: '',
                                    replacement: '',
                                    disabled: false,
                                    scope: 'display',
                                    targets: ['ai_output'],
                                    is_global: true,
                                },
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shirita-core --lib seed:: 2>&1 | tail -15`

Expected: `test result: ok. 7 passed; 0 failed`.

- [ ] **Step 5: Fix the now-stale fixtures in `shirita-web/tests/regex_display_test.rs`**

This file's two tests construct unreferenced `regex_rule` fixtures relying on the old "unreferenced = global" behavior (now fixed by Task 4) — they need `is_global: true` to keep testing the same thing they always tested (display-time regex application to AI output), not a new behavior.

In `shirita-web/tests/regex_display_test.rs`, in `list_messages_applies_display_regex_at_read_time`, find:

```rust
    // Global orphan rule: strip "SECRET" from AI output for display only.
    let mut rule = Definition::new("regex_rule", "redact", "");
    rule.meta = serde_json::json!({
        "pattern": "SECRET", "replacement": "", "scope": "display", "targets": ["ai_output"]
    });
```

Replace it with:

```rust
    // Explicit global rule: strip "SECRET" from AI output for display only.
    let mut rule = Definition::new("regex_rule", "redact", "");
    rule.meta = serde_json::json!({
        "pattern": "SECRET", "replacement": "", "scope": "display", "targets": ["ai_output"], "is_global": true
    });
```

Later in the same test, find:

```rust
    // Editing the rule reflects immediately on the next fetch — no re-write.
    rule.meta = serde_json::json!({
        "pattern": "SECRET", "replacement": "[redacted]", "scope": "display", "targets": ["ai_output"]
    });
```

Replace it with:

```rust
    // Editing the rule reflects immediately on the next fetch — no re-write.
    rule.meta = serde_json::json!({
        "pattern": "SECRET", "replacement": "[redacted]", "scope": "display", "targets": ["ai_output"], "is_global": true
    });
```

In `list_messages_skips_rp_regex_on_a_fenced_html_card_document`, find:

```rust
    let mut rule = Definition::new("regex_rule", "redact", "");
    rule.meta = serde_json::json!({
        "pattern": "SECRET", "replacement": "", "scope": "display", "targets": ["ai_output"]
    });
    state.storage.create_definition(&rule).await.unwrap();

    let html = "```\r\n<!DOCTYPE html>\r\n<html><body>SECRET</body></html>\r\n```";
```

Replace it with:

```rust
    let mut rule = Definition::new("regex_rule", "redact", "");
    rule.meta = serde_json::json!({
        "pattern": "SECRET", "replacement": "", "scope": "display", "targets": ["ai_output"], "is_global": true
    });
    state.storage.create_definition(&rule).await.unwrap();

    let html = "```\r\n<!DOCTYPE html>\r\n<html><body>SECRET</body></html>\r\n```";
```

Run: `cargo test -p shirita-web --test regex_display_test --jobs 2 2>&1 | tail -15`

Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 6: Run the full Rust workspace test suite to check for regressions**

Run: `cargo build -p shirita-web -p shirita-tauri --bins 2>&1 | tail -15 && cargo test --workspace --jobs 2 2>&1 | grep -E "FAILED|error:|test result"`

Expected: clean build of both binaries (confirms the new `ensure_global_regex_flag` calls compile in both `shirita-web/src/main.rs` and `shirita-tauri/src/main.rs`), and every `test result:` line reads `ok.` with `0 failed`.

- [ ] **Step 7: Commit**

```bash
git add shirita-core/src/seed.rs shirita-core/src/lib.rs shirita-web/src/main.rs shirita-tauri/src/main.rs \
        shirita-ui/src/views/SettingsView.vue shirita-web/tests/regex_display_test.rs
git commit -m "$(cat <<'EOF'
feat(regex): backfill is_global for existing orphan rules; flag new ones explicitly at creation

ensure_global_regex_flag (run once at startup, idempotent) sets is_global:
true on every regex_rule Definition that is currently unreferenced and has
no is_global key yet, so upgrading doesn't silently stop any rule a user
currently relies on from applying. Settings' "add rule" button now sets
is_global: true explicitly at creation time instead of relying on starting
unreferenced.
EOF
)"
```

---

### Task 7: Full cross-cutting regression pass

**Files:** none (verification only).

**Interfaces:** none — final confirmation that Tasks 1-6 together produce a fully green build and test suite, with no file conflicts or cross-task interaction effects.

- [ ] **Step 1: Run the full Rust workspace test suite**

Run: `cargo test --workspace --jobs 2 2>&1 | grep -E "FAILED|error:|test result"`

Expected: every `test result:` line reads `ok.` with `0 failed`, no `FAILED` or `error:` lines.

- [ ] **Step 2: Run the full frontend test suite, including locale parity**

Run: `cd shirita-ui && npx vitest run`

Expected: `Test Files  40 passed (40)`, `Tests  278 passed (278)`.

- [ ] **Step 3: Build both Rust binaries to confirm the new startup calls link cleanly**

Run: `cargo build -p shirita-web -p shirita-tauri --bins 2>&1 | tail -10`

Expected: `Finished` with no errors (a pre-existing `is_reserved_prefix` dead-code warning in `shirita-web/src/embed.rs` is unrelated to this plan and may still appear — that's fine).

- [ ] **Step 4: Note manual follow-up (not automated by this plan)**

This plan's automated tests cover each fix's mechanics with synthetic fixtures and unit/integration tests. To confirm Fix A/B against the actual reported card end-to-end (re-import `examples/怪谈社.json`, open the chat, confirm the carousel card now sizes to its real content with no blank/black area, and confirm it uses the full message-row width), use the `/verify` skill in a follow-up session against a running dev server — this requires a browser and is intentionally not part of this code-change plan.

- [ ] **Step 5: Commit (only if Steps 1-3 required any fixups)**

If Steps 1-3 passed with no changes needed, there is nothing to commit for this task — Tasks 1-6 already each committed their own work.
