# ST Card Render Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two real-world ST card import/render bugs: `try_convert_status_panel` wrongly converting full HTML documents into broken native Panels, and `HtmlCardFrame.vue`'s fixed-height iframe clipping/misrendering content-sized cards.

**Architecture:** Two independent, narrow fixes to existing code — no new subsystems. Fix A adds a rejection check inside `try_convert_status_panel` (Rust). Fix B adds a `postMessage`-based auto-resize channel to `HtmlCardFrame.vue` (Vue), since the iframe sandbox intentionally has no `allow-same-origin` and so cannot use the direct-DOM-write approach real SillyTavern plugins use.

**Tech Stack:** Rust (`shirita-core`, `regex` crate, `fancy_regex`), Vue 3 `<script setup>` (`shirita-ui`), Vitest + `@vue/test-utils`, `cargo test`.

## Global Constraints

- Fix A must not change behavior for any candidate that is already accepted today (verified by the existing passing test suite in `charcard.rs` staying green).
- Fix B must not change the `sandbox`/`referrerpolicy` attributes or the existing theme-variable `<style>` injection in `HtmlCardFrame.vue`.
- No change to `shirita-ui/src/utils/panel.ts`'s `ALLOWED_TAGS`/`ALLOWED_ATTR`/`fenceCss`.
- `examples/命定之诗与黄昏之歌v4.2.json` (external JS loading) is out of scope — no task here touches it.

---

### Task 1: Reject document-level/inline-handler candidates in `try_convert_status_panel`

**Files:**
- Modify: `shirita-core/src/adapters/charcard.rs` (function `try_convert_status_panel`, around line 171-231; add a new helper function near it)
- Test: `shirita-core/src/adapters/charcard.rs` (`#[cfg(test)] mod tests` block, alongside the existing `try_convert_status_panel_*` tests around line 517-620, and alongside `charcard_to_loreset_*` tests around line 748-790)

**Interfaces:**
- Consumes: existing `try_convert_status_panel(scripts: &[serde_json::Value]) -> Option<PanelConversion>`, existing `script(find_regex: &str, replace_string: &str) -> serde_json::Value` test helper, existing `charcard_to_loreset(card: &serde_json::Value) -> LoreSet`.
- Produces: new private helper `fn is_unrepresentable_as_panel(html: &str) -> bool`, used only inside `try_convert_status_panel`. No public API changes — `PanelConversion`'s fields and `try_convert_status_panel`'s signature are unchanged.

- [ ] **Step 1: Write the failing tests**

Add these to the `#[cfg(test)] mod tests` block in `shirita-core/src/adapters/charcard.rs`, directly after `try_convert_status_panel_repeated_dollar_n_yields_one_variable` (around line 600):

```rust
    #[test]
    fn try_convert_status_panel_skips_full_html_document() {
        // A candidate whose replaceString is a complete standalone document
        // (not a fragment) cannot become Panel content — Panel forbids
        // <html>/<head>/<body> document-wrapper structure.
        let scripts = vec![script(
            r"<hp>(\d+)</hp>",
            "<!DOCTYPE html><html><body><div>HP: $1</div></body></html>",
        )];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_skips_inline_event_handlers() {
        // A fragment (no <html> wrapper) that still relies on inline
        // on*= handlers for its core interactivity cannot work as a Panel
        // (sanitizePanelHtml strips all on*= attributes).
        let scripts = vec![script(
            r"<hp>(\d+)</hp>",
            r#"<div onclick="doStuff()">HP: $1</div>"#,
        )];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_accepts_plain_fragment_without_handlers() {
        // Sanity check: the new rejection check must not affect a normal,
        // already-passing fragment candidate (no <html>, no on*=).
        let scripts = vec![script(r"<hp>(\d+)</hp>", "<div class=\"bar\">HP: $1</div>")];
        let conv = try_convert_status_panel(&scripts).expect("plain fragment must still convert");
        assert_eq!(conv.html, "<div class=\"bar\">HP: {{field1}}</div>");
    }
```

Add this to the `#[cfg(test)] mod tests` block, directly after `charcard_to_loreset_omits_panel_when_no_status_bar_detected` (around line 781):

```rust
    #[test]
    fn charcard_to_loreset_omits_panel_for_full_document_status_bar() {
        // Reproduces the 密教模拟器.json shape: a single $N-template candidate
        // whose replaceString is a complete HTML document with onclick
        // handlers and id-based CSS — must stay on the compatibility layer
        // (regex_rule untouched, no panel written) rather than become a
        // broken Panel.
        let card = serde_json::json!({
            "data": {
                "name": "Cultist", "description": "desc",
                "extensions": { "regex_scripts": [
                    { "scriptName": "status", "findRegex": "<hp>(\\d+)</hp>",
                      "replaceString": "<!DOCTYPE html><html><head><style>#bar{color:red}</style></head><body><div id=\"bar\" onclick=\"tick()\">HP: $1</div></body></html>",
                      "disabled": false, "markdownOnly": true }
                ] }
            }
        });
        let ls = charcard_to_loreset(&card);
        assert!(ls.template.meta.get("panel").is_none());
        let rule = ls.definitions.iter().find(|d| d.def_type == "regex_rule").unwrap();
        assert!(rule.meta.get("capture_vars").is_none());
        // the compatibility-layer fields are untouched — this script still
        // works exactly as before via apply_regex_rules_for + HtmlCardFrame.
        assert_eq!(rule.meta["pattern"], "<hp>(\\d+)</hp>");
        assert!(rule.meta["replacement"].as_str().unwrap().contains("onclick=\"tick()\""));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shirita-core --lib charcard:: 2>&1 | tail -40`

Expected: the three new `try_convert_status_panel_*` tests and `charcard_to_loreset_omits_panel_for_full_document_status_bar` FAIL (the first two `assert!(...is_none())` fail because conversion currently succeeds; `try_convert_status_panel_accepts_plain_fragment_without_handlers` should already PASS since it's a sanity check of unchanged behavior — that one is allowed to pass before the fix, it only matters that it still passes after).

- [ ] **Step 3: Implement the fix**

In `shirita-core/src/adapters/charcard.rs`, add this new function directly before `try_convert_status_panel` (i.e. right after the `PanelConversion` struct definition, before line 167's doc comment):

```rust
/// True if `html` cannot be represented as Panel content: either it's a
/// full standalone document (contains an `<html`, `<head`, or `<body`
/// opening tag — a `$N`-template status bar is, by definition, a fragment
/// meant to be spliced into an existing page, not a complete document), or
/// it relies on inline event-handler attributes (`onclick=...` etc. —
/// `sanitizePanelHtml` strips every `on*=` attribute, so a template that
/// needs one for its core interactivity cannot function as a Panel and
/// must stay on the compatibility layer where its handlers actually run).
fn is_unrepresentable_as_panel(html: &str) -> bool {
    static DOC_TAG_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)<(?:html|head|body)\b").unwrap());
    static INLINE_HANDLER_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)\bon[a-z]+\s*=").unwrap());
    DOC_TAG_RE.is_match(html) || INLINE_HANDLER_RE.is_match(html)
}
```

Then in `try_convert_status_panel`, insert the rejection check right after `substituted` is computed (find this line, currently just before the `extract_tag_blocks(&substituted, "style")` call):

```rust
    let substituted = substitute_dollar_refs(&c.replace_string, &c.valid_ns);
```

Change it to:

```rust
    let substituted = substitute_dollar_refs(&c.replace_string, &c.valid_ns);
    if is_unrepresentable_as_panel(&substituted) {
        return None;
    }
```

Leave everything else in the function (the `extract_tag_blocks` calls, `var_decls` building, the final `Some(PanelConversion { .. })`) unchanged.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p shirita-core --lib charcard:: 2>&1 | tail -40`

Expected: `test result: ok. 30 passed; 0 failed` (26 pre-existing + 4 new tests added in Step 1), all `try_convert_status_panel_*` and `charcard_to_loreset_*` tests passing including the four added in Step 1.

- [ ] **Step 5: Run the full shirita-core test suite to check for regressions**

Run: `cargo test -p shirita-core 2>&1 | tail -15`

Expected: `test result: ok.` with no failures (this confirms nothing outside `charcard.rs` depended on the old over-eager conversion behavior).

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/adapters/charcard.rs
git commit -m "$(cat <<'EOF'
fix(import): don't convert full-HTML-document status bars into native panels

try_convert_status_panel accepted any single $N-template candidate without
checking whether the result is actually representable as Panel content.
A card whose status bar is a complete <html>/<body> document with onclick
handlers (examples/密教模拟器.json) was being converted into a Panel that
sanitizePanelHtml then gutted (html/head/body/onclick/id all stripped),
producing a broken-looking, dead panel. Such candidates now correctly fall
through to the existing compatibility layer (regex_rule + HtmlCardFrame),
where their full markup and onclick handlers actually work.
EOF
)"
```

---

### Task 2: Auto-sizing iframe in `HtmlCardFrame.vue`

**Files:**
- Modify: `shirita-ui/src/components/HtmlCardFrame.vue`
- Test: Create `shirita-ui/src/components/HtmlCardFrame.test.ts`

**Interfaces:**
- Consumes: nothing from Task 1 (fully independent). Existing component prop `html: string` is unchanged.
- Produces: the rendered `<iframe>`'s height becomes dynamic (driven by a `message` event listener) instead of the previous fixed `640px` CSS rule. No new props or emits — purely an internal behavior change, so `MarkdownText.vue` (the only current consumer) needs no changes.

**Important implementation note (found by prototyping before writing this plan):** the obvious approach — verify a `message` event by comparing `event.source` against the iframe's `contentWindow` — does not work reliably. In `jsdom` (this project's test environment), `iframe.contentWindow` stays `null` for a `srcdoc` iframe unless real resource loading is enabled, so any test asserting against it is flaky/wrong, and relying on `contentWindow` identity is also just unnecessary complexity even in real browsers. Use a **per-instance random token** embedded in the injected script instead: it sidesteps cross-frame window-identity entirely, is trivial to test, and correctly distinguishes multiple `HtmlCardFrame` instances on the same page from each other.

- [ ] **Step 1: Write the failing test**

Create `shirita-ui/src/components/HtmlCardFrame.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import HtmlCardFrame from './HtmlCardFrame.vue'

function tokenOf(w: ReturnType<typeof mount>): string {
  const iframe = w.find('iframe').element as HTMLIFrameElement
  const srcdoc = iframe.getAttribute('srcdoc') || ''
  const m = srcdoc.match(/token: '([a-z0-9]+)'/)
  if (!m) throw new Error('token not found in srcdoc')
  return m[1]
}

function postReport(data: unknown) {
  window.dispatchEvent(new MessageEvent('message', { data }))
}

describe('HtmlCardFrame', () => {
  it('starts at the default 640px height before any size report', () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('640px')
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
    expect(iframe.style.height).toBe('640px')
  })

  it('ignores a message with a different source tag', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    postReport({ source: 'something-else', token: tokenOf(w), height: 1200 })
    await w.vm.$nextTick()
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('640px')
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
    expect((b.find('iframe').element as HTMLIFrameElement).style.height).toBe('640px')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/HtmlCardFrame.test.ts`

Expected: FAIL — `starts at the default 640px height` fails too (current fixed height is set via the `<style scoped>` CSS rule, not an inline style, so `iframe.style.height` is currently `''`, not `'640px'`), and all the resize/clamp/ignore/token tests fail since there is no `message` listener and no `srcdoc` token at all yet.

- [ ] **Step 3: Implement the fix**

Replace the full contents of `shirita-ui/src/components/HtmlCardFrame.vue` with:

```vue
<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'

// Renders a SillyTavern "HTML card" front-end (a full HTML/CSS/JS document
// embedded in a message) inside a sandboxed iframe. `sandbox="allow-scripts"`
// without `allow-same-origin` gives the document an opaque origin: its script
// can run, but it cannot reach the parent DOM, cookies, localStorage, or this
// app's auth state, and it cannot navigate the top-level page.
const props = defineProps<{ html: string }>()

function themeVar(name: string, fallback: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return v || fallback
}

// Unique per component instance, so a `message` event can be matched back to
// this card without relying on cross-frame window-identity (`event.source`),
// which is unset until the iframe document has actually loaded and is
// awkward to assert on in tests.
const token = Math.random().toString(36).slice(2)

// Real cards are authored assuming the host sizes the iframe to their
// content (the SillyTavern/JS-Slash-Runner convention — see
// examples/JS-Slash-Runner/src/iframe/adjust_iframe_height.js, which writes
// `frameElement.style.height` directly; that requires `allow-same-origin`,
// which this sandbox intentionally omits). `postMessage` works without it,
// so this injected script relays the same measurement across the boundary
// instead of writing the DOM directly.
function resizeScript(t: string): string {
  return `
<script>
(function () {
  function measure() {
    return Math.max(
      document.body.scrollHeight,
      document.body.offsetHeight,
      document.documentElement.scrollHeight,
      document.documentElement.offsetHeight,
    )
  }
  function report() {
    window.parent.postMessage({ source: 'shirita-html-card', token: '${t}', height: measure() }, '*')
  }
  if (document.body) {
    new ResizeObserver(report).observe(document.body)
  }
  report()
})()
<\/script>
`
}

// The srcdoc document has no access to the app's CSS (opaque origin), so without
// this it always falls back to the browser's white default when a card doesn't
// set its own background. Inject the app's theme colors as a base rule the
// card's own CSS (which comes later in source order) can still override.
const srcdoc = computed(() => {
  const bg = themeVar('--color-card', '#fff')
  const fg = themeVar('--color-ink', '#1b1b1b')
  return `<style>html,body{background:${bg};color:${fg};margin:0}</style>${props.html}${resizeScript(token)}`
})

const DEFAULT_HEIGHT = 640
const MIN_HEIGHT = 80
const MAX_HEIGHT = 4000
const height = ref(DEFAULT_HEIGHT)

function onMessage(e: MessageEvent) {
  const data = e.data as { source?: string; token?: string; height?: number } | null
  if (!data || data.source !== 'shirita-html-card' || data.token !== token || typeof data.height !== 'number') return
  height.value = Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, data.height))
}

onMounted(() => window.addEventListener('message', onMessage))
onUnmounted(() => window.removeEventListener('message', onMessage))
</script>

<template>
  <iframe
    class="html-card-frame"
    sandbox="allow-scripts"
    referrerpolicy="no-referrer"
    :srcdoc="srcdoc"
    :style="{ height: height + 'px' }"
  />
</template>

<style scoped>
.html-card-frame {
  display: block;
  width: 100%;
  border: 1px solid var(--color-line, #e5e5e5);
  border-radius: 10px;
  background: var(--color-card, #fff);
}
</style>
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/HtmlCardFrame.test.ts`

Expected: `Test Files  1 passed (1)`, `Tests  8 passed (8)`.

- [ ] **Step 5: Run the full frontend test suite to check for regressions**

Run: `cd shirita-ui && npx vitest run`

Expected: all test files pass, including `PanelView.test.ts` and any test that mounts `MarkdownText.vue` (which renders `HtmlCardFrame` for ` ```html ` blocks / raw `<!DOCTYPE>` messages) — confirm none of them assert a fixed iframe height that this change would break.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/components/HtmlCardFrame.vue shirita-ui/src/components/HtmlCardFrame.test.ts
git commit -m "$(cat <<'EOF'
fix(html-card-frame): auto-size the iframe to its content instead of a fixed 640px

Cards are authored assuming the host sizes the iframe to their natural
content height (examples/怪谈社.json's draggable carousel relies on this).
The previous fixed 640px height clipped taller cards, making their content
appear squished/cut off and their interactive elements unreachable — not a
script failure, just content rendered outside the visible/clickable area.

Since the sandbox intentionally has no allow-same-origin, the card can't
write frameElement.style.height directly (the SillyTavern/JS-Slash-Runner
convention); instead an injected script posts its measured height via
postMessage, which works across the opaque-origin boundary, and the host
clamps and applies it.
EOF
)"
```

---

### Task 3: Full regression pass across both fixes

**Files:** none (verification only).

**Interfaces:** none — this task runs the full test suites from Tasks 1 and 2 together to confirm no cross-cutting regression, and documents the manual follow-up.

- [ ] **Step 1: Run the full Rust workspace test suite**

Run: `cargo test --workspace 2>&1 | tail -30`

Expected: `test result: ok.` for every crate (`shirita-core`, `shirita-web`, `shirita-tauri`), no failures.

- [ ] **Step 2: Run the full frontend test suite**

Run: `cd shirita-ui && npx vitest run`

Expected: all test files pass.

- [ ] **Step 3: Note manual follow-up (not automated by this plan)**

This plan's automated tests cover the bug mechanics with synthetic fixtures. To confirm the fix against the actual reported cards end-to-end (re-import `examples/怪谈社.json` and `examples/密教模拟器.json` into a running instance, open a chat, and visually confirm the carousel card now sizes correctly and 密教模拟器's status bar renders via `HtmlCardFrame` instead of a broken Panel), use the `/verify` skill in a follow-up session against a running dev server — this requires a browser and is intentionally not part of this code-change plan.

- [ ] **Step 4: Commit (only if Steps 1-2 required any fixups)**

If Steps 1-2 passed with no changes needed, there is nothing to commit for this task — Tasks 1 and 2 already each committed their own work.
