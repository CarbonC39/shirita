# ST Card Import/Render Fixes — Design

> Follow-on to `2026-06-22-st-status-panel-conversion-design.md`. That spec's `try_convert_status_panel` heuristic (`shirita-core/src/adapters/charcard.rs`) and `HtmlCardFrame.vue`'s fixed-height iframe both ship working code, but real-world cards in `examples/` expose two concrete bugs in them. This spec fixes both. It does not redesign either mechanism.

## 1. Background & motivation

Importing `examples/怪谈社.json` and `examples/密教模拟器.json` and using them in a chat shows two distinct, reproducible problems:

- **怪谈社**: its `first_mes` embeds a full interactive HTML document (a draggable card carousel using `addEventListener('mousedown'/'click'/...)`). This correctly renders via `HtmlCardFrame.vue` (confirmed: `MarkdownText.vue`'s `isHtmlDocument()` check catches it). But the iframe has a **fixed 640px height** (`HtmlCardFrame.vue`'s `.html-card-frame { height: 640px }`), while the card's layout assumes it can size to its own natural content height. The visible symptoms — "size looks wrong", "doesn't fill/enlarge to fit", "buttons aren't clickable" — are one root cause: content taller than 640px is clipped, and the parts a user would click are outside the visible/reachable area. The card's own `addEventListener` calls are not failing; nothing is wrong with its JS.
- **密教模拟器**: its status-bar `regex_scripts` entry (`findRegex` has 60 capture groups, `replaceString` is a 90KB **complete HTML document**: `<!DOCTYPE html><html><head><style>...</style></head><body><nav>...<main>...<header>...<footer>...<script>...</script></body></html>`, with `onclick="..."` handlers and `id`-based CSS selectors) is the only `$N`-template candidate, so `try_convert_status_panel` converts it into a native Panel. Verified by running the project's actual `sanitizePanelHtml`/`fenceCss` config (`shirita-ui/src/utils/panel.ts`) against the substituted output: `html`, `head`, `body`, `nav`, `main`, `header`, `footer`, `textarea` tags and `id`, `onclick` attributes are all stripped (Panel's `ALLOWED_TAGS`/`ALLOWED_ATTR` is a narrow, deliberately script-free allowlist; this is correct behavior for *Panel*, applied to content that should never have become a Panel). The result is a panel with broken layout and dead buttons — not a sanitizer bug, a **conversion-target selection bug**.

Both fixes are narrow corrections to existing logic, not new subsystems. `examples/命定之诗与黄昏之歌v4.2.json` (a card whose status bar depends on loading external JS) is out of scope per explicit user direction — no compatibility layer can run arbitrary fetched third-party JS inside the sandboxed iframe, and that is not addressed here.

## 2. Fix A — reject document-level candidates in `try_convert_status_panel`

Location: `shirita-core/src/adapters/charcard.rs`, inside `try_convert_status_panel` (the existing candidate-collection loop and the post-substitution step that builds `PanelConversion`).

**Problem:** the function's only acceptance criteria today are "exactly one enabled, display-scoped script has ≥1 valid `$N` reference" (per the existing spec's validation rules). It never checks whether the resulting HTML is actually representable as Panel content. A full standalone document is structurally incompatible with Panel (shadow-DOM fragment, narrow tag/attribute allowlist, no inline event handlers, no `<script>`) regardless of how clean its `$N` templating is.

**Fix:** after computing `substituted` (the `$N`→`{{fieldN}}`-substituted replacement string, before the existing `<style>`/`<script>` extraction) and before committing to that candidate, run a cheap structural check. Reject the candidate (function returns `None`, behaving exactly as today's "0 or ≥2 candidates" case — the regex_rule Definition is still created normally, untouched, and flows through the existing display-time regex + `HtmlCardFrame` compatibility layer) if **any** of the following hold on `substituted`:

- It contains an `<html`, `<head`, or `<body` opening tag (case-insensitive). A `$N`-template status bar is, by definition, a fragment meant to be spliced into an existing page; a card that emits these is building a full document, not a status-bar snippet.
- It contains an inline event-handler attribute: a tag attribute matching `(?i)\bon[a-z]+\s*=`. Panel has no script execution story by design (`sanitizePanelHtml` strips all `script`/`on*`); a template that relies on inline handlers for its core interactivity cannot function as a Panel and must stay on the compatibility layer where its handlers actually run.

This is a pure addition to `try_convert_status_panel`; nothing about candidate collection, `$N` validation, or the conversion steps for an *accepted* candidate changes. `examples/怪谈社.json`'s already-converting candidate (the unrelated `<UpdateVariable>`-log beautifier, a `<details>`-wrapped snippet) has neither an `<html>` wrapper nor inline handlers, so it is unaffected by this check and continues to convert as today — it is harmless, and not part of either reported bug.

**Test additions** (`shirita-core/src/adapters/charcard.rs` existing `#[cfg(test)]` module, alongside `try_convert_status_panel_skips_when_ambiguous` etc.):
- `try_convert_status_panel_skips_full_html_document`: a script whose `replaceString` contains `<html>...$1...</html>` → `None`.
- `try_convert_status_panel_skips_inline_event_handlers`: a script whose `replaceString` is a fragment (no `<html>`) but contains `<div onclick="...">$1</div>` → `None`.
- A regression test importing the actual `examples/密教模拟器.json` fixture (or a trimmed inline excerpt reproducing its `<html>...<body>...onclick...` shape) through `charcard_to_loreset` and asserting `template.meta.get("panel")` is `None` while the `regex_rule` Definition for that script is still present with its original `pattern`/`replacement` untouched.

## 3. Fix B — auto-sizing iframe in `HtmlCardFrame.vue`

Location: `shirita-ui/src/components/HtmlCardFrame.vue`.

**Problem:** the iframe has a hardcoded `height: 640px`. Real cards are authored assuming the host will size the frame to their content (this is the de facto SillyTavern convention — `examples/JS-Slash-Runner/src/iframe/adjust_iframe_height.js` ships exactly this behavior, using a `ResizeObserver` on `body` and writing the measured height directly via `frameElement.style.height`). That direct write requires `allow-same-origin`, which Shirita's sandbox intentionally omits (security boundary — out of scope to change). `postMessage` works without `allow-same-origin` (it's available to any window reference regardless of origin), so the same measurement can be relayed across the boundary instead of written directly.

**Fix:**
1. Inject a small script into the `srcdoc` (alongside the existing theme-variable `<style>` injection) that:
   - On `body` `ResizeObserver` firing (and once on initial load), measures `document.body.scrollHeight`.
   - Posts `{ source: 'shirita-html-card', height }` to `window.parent` via `postMessage('*')` — `'*'` is required since the parent's origin is unknown to this opaque-origin document; the message payload carries only a number, nothing sensitive.
2. In `HtmlCardFrame.vue`, add a `message` event listener (added on mount, removed on unmount) that:
   - Ignores any event whose `event.source` is not this component's own iframe `contentWindow` (guards against unrelated `message` events elsewhere on the page, e.g. other open `HtmlCardFrame` instances) and whose `data.source` is not `'shirita-html-card'`.
   - Clamps the reported height to a sane range: minimum 80px (avoids a flash of zero-height while the card's own layout is still settling), maximum 4000px (a card reporting more than that is almost certainly buggy or hostile; beyond the cap the iframe stays at the cap and relies on its own internal scrolling, same as today's fixed-height behavior).
   - Sets the iframe's height (a `ref`-backed reactive value bound in the template) to the clamped value.
3. Before the first `message` arrives (or if a card's script never sends one — e.g. it crashes before `ResizeObserver` registers), fall back to today's fixed height. So: initial height = 640px (unchanged default), replaced once a valid size report arrives; never goes below the 80px floor afterward even if a later report is smaller (avoids visible collapse-then-grow flicker from a card that briefly renders empty before its own content populates) — except that a report of 80px or more always applies, so a card that legitimately shrinks (e.g. collapsing a `<details>`) still resizes down; only reports *below* the 80px floor are clamped up, not ignored.
4. No change to `sandbox` attributes, `referrerpolicy`, or the existing theme-variable injection.

**Test additions** (`shirita-ui/src/components/HtmlCardFrame.test.ts` — check if this file exists; create alongside the component's existing test conventions if not):
- Renders with default height before any message.
- Dispatches a `message` event with `{ source: 'shirita-html-card', height: 1200 }` from the iframe's `contentWindow` and asserts the rendered height updates to `1200px`.
- A `message` from an unrelated source (different `event.source`, or missing `data.source`) does not change the height.
- A reported height of `50` clamps to `80`; a reported height of `9000` clamps to `4000`.

## 4. Out of scope (explicitly, per this conversation)

- `examples/命定之诗与黄昏之歌v4.2.json` (external JS loading) — no fix attempted.
- The broader accessibility/robustness items discussed earlier in this session (PanelView keyboard activation for `data-diff-key`/`data-insert`/`data-send`, import file-size/timeout guards, icon-button `aria-label`s, error-banner `role="alert"`, etc.) — parked as potential follow-up work, not part of this spec.
- Any change to `sanitizePanelHtml`'s `ALLOWED_TAGS`/`ALLOWED_ATTR` allowlist — Fix A keeps document-level cards off the Panel path entirely instead, so the allowlist itself doesn't need to widen.
- A `postMessage`-based heartbeat/liveness check for the iframe's script (considered and declined earlier in this session) — Fix B's message channel is one-directional (size reporting only) and does not attempt to detect a hung or crashed card script.
