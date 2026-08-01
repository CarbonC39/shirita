# Frontend Render Fixes + Global Regex Pollution — Design

> Follow-on to `2026-06-23-st-card-render-fixes-design.md`. That spec's `HtmlCardFrame.vue` auto-sizing fix shipped but introduced a new bug (Fix A below); this spec corrects it, fixes an unrelated bubble-width constraint affecting cards (Fix B), adds a missing panel-visibility affordance (Fix C), and fixes the regex global-pollution mechanism reported separately (Fix D). The four fixes are independent of each other and of the previous spec's Fix A/B.

## 1. Fix A — HtmlCardFrame height-plateau bug

**Problem:** `HtmlCardFrame.vue` starts the iframe at a 640px default height (`const height = ref(DEFAULT_HEIGHT)`, `DEFAULT_HEIGHT = 640`) before any real size report arrives, then narrows it down via `postMessage`. This is wrong for any card whose layout resolves its own height against the iframe's *current* box size — e.g. `examples/怪谈社.json`'s card has `body { display: flex; align-items: center; ... }` with no explicit height, relying on `100vh`/`100%`-style sizing common in this card genre to center its content vertically within "the available space." On first paint, "available space" is our 640px default. The injected measurement script then reports `Math.max(document.body.scrollHeight, document.body.offsetHeight, document.documentElement.scrollHeight, document.documentElement.offsetHeight)` — but `scrollHeight` is defined as `max(box's own height, its content's height)`. Once the box (body, sized via `vh`/`%` against the 640px iframe) is *taller* than the actual visible content, `scrollHeight` can never report smaller than that box height — it's mathematically a plateau, not a transient overshoot that self-corrects. The result (confirmed by the user inspecting devtools: `style="height: 638px"`, with visible content occupying roughly 2/3 of that box) is a small centered card floating in a too-tall, blank/black iframe.

Starting *small* does not have this problem: if the box starts ≤ content size, `scrollHeight = max(box, content) = content`, and `ResizeObserver` naturally grows the box to match as layout settles. The plateau only occurs when the starting box is *larger* than the eventual content — which is exactly what `DEFAULT_HEIGHT = 640` guarantees for any percentage/viewport-relative card layout.

**Fix:** in `shirita-ui/src/components/HtmlCardFrame.vue`, change the initial value of `height` from `DEFAULT_HEIGHT` (640) to `MIN_HEIGHT` (80, the existing clamp floor) — i.e. delete the separate `DEFAULT_HEIGHT` constant entirely and seed `height = ref(MIN_HEIGHT)`. This means:
- Before the first size report, the iframe renders at 80px (a brief, intentional "small while measuring" state) instead of falsely confident 640px.
- The first real `ResizeObserver` report (which fires essentially immediately after initial layout, same tick/microtask) replaces it with the correct measured height, with no plateau risk since 80px is smaller than virtually any real card.
- The existing `MIN_HEIGHT`/`MAX_HEIGHT` clamps on *reported* values are unchanged — this only changes the pre-report default.

No change to the `resizeScript` measurement logic itself (the `Math.max` of four metrics stays — it's the *starting point*, not the measurement formula, that caused the plateau; switching to a single metric wouldn't fix a too-large starting default, and the four-metric approach remains the more defensive choice for cards that don't hit this specific failure mode).

**Test impact:** `shirita-ui/src/components/HtmlCardFrame.test.ts`'s `'starts at the default 640px height before any size report'` test changes its expectation to `80px` and is renamed to reflect the new floor-based default.

## 2. Fix B — bubble width cap exempts HTML cards

**Problem:** `shirita-ui/src/components/MessageItem.vue:64` wraps a message's entire rendered content (text, and anything `MarkdownText.vue` renders, including `HtmlCardFrame`) in `<div :class="['max-w-[78%]', isUser ? 'order-first' : '']">`. This 78% cap is the right call for chat-bubble text (a deliberate readability/aesthetic constraint) but wrong for an embedded HTML card, which is rich, self-contained content meant to use the full message-row width — capping it at 78% of an already-narrow column squeezes a card designed for ~520px+ widths into less space than intended, compounding any layout assumptions the card's own CSS makes.

**Fix:**
1. Add a small exported helper to `shirita-ui/src/utils/markdown.ts`:
   ```ts
   /** True if `text`, rendered as Markdown, would produce at least one HtmlCardFrame
    *  (a raw HTML document, or a fenced ```html / detected-HTML code block). Mirrors
    *  MarkdownText.vue's own rendering decision — used by MessageItem.vue to decide
    *  whether the bubble-width cap should apply. */
   export function containsHtmlCard(text: string): boolean {
     if (isHtmlDocument(text)) return true
     return parseMarkdown(text).some((n) => n.type === 'codeblock' && (n.lang === 'html' || isHtmlDocument(n.value)))
   }
   ```
2. In `MarkdownText.vue`, no change needed (it already does this check inline) — `containsHtmlCard` is a new, separate export for `MessageItem.vue` to reuse the same decision, not a refactor of the existing render path (keeps this change minimal and low-risk).
3. In `MessageItem.vue`, compute `const isCard = computed(() => containsHtmlCard(props.displayText ?? props.message.display_content ?? props.message.raw_content))` (using the same text the bubble actually renders — `displayText` per the existing `computed` at line ~36) and change the wrapper class from `'max-w-[78%]'` to `[isCard ? 'max-w-full' : 'max-w-[78%]']`.

This only affects the *HtmlCardFrame* path. `PanelView` is unaffected — it's already rendered outside any bubble, at the top of `ChatView.vue` (`panel-stack`, `ChatView.vue:175-182`), with no width cap.

**Test impact:** new test in `MessageItem.test.ts` asserting the wrapper has `max-w-full` (not `max-w-[78%]`) when `message.raw_content` is a raw HTML document, and still has `max-w-[78%]` for plain text.

## 3. Fix C — per-pack panel visibility threshold

**Problem:** `ChatView.vue:175` shows the panel header for every mounted pack with a `meta.panel` the instant it's mounted (`v-if="panelPacks.length"`), with no way to express "this card's status panel should only appear once the story has actually started" (a common authoring intent — many cards' `first_mes` is an intro/menu screen with no meaningful stats yet).

**Fix:** add an optional `min_messages` field to the Panel data model, defaulting to `0` (today's always-show behavior) when absent — author-configurable per pack, not a global rule, since different cards want different thresholds (or none).

1. `shirita-ui/src/api/types.ts`: extend the `Panel` interface:
   ```ts
   export interface Panel {
     html: string
     css: string
     caps: PanelCaps
     /** Hide the panel header until the chat has at least this many messages.
      *  Absent/0 = always show (today's behavior). */
     min_messages?: number
   }
   ```
2. `shirita-ui/src/components/PackEditor.vue`: add a `panelMinMessages` ref seeded from `p?.min_messages ?? 0` alongside the existing `panelHtml`/`panelCss`/`panelCaps` refs (same `watch` block, same `savePanel()` persistence — add `min_messages: panelMinMessages.value` to the object `savePanel()` writes). Add a number input in the panel section template (after the caps checkboxes, before the preview):
   ```html
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
   ```
3. `shirita-ui/src/views/ChatView.vue`: change the template condition from rendering all of `panelPacks` to filtering by the live message count:
   ```html
   <div v-if="visiblePanelPacks.length" data-test="panel-stack" class="flex flex-col gap-2 py-2">
     <details v-for="p in visiblePanelPacks" :key="p.id" open ...>
   ```
   with a new computed property near `panelPacks`:
   ```ts
   const visiblePanelPacks = computed(() =>
     panelPacks.value.filter((p) => chat.messages.length >= (panelOf(p).min_messages ?? 0)),
   )
   ```
   `chat.messages.length` (not `chat.displayed.length`) is the right count — it's the full conversation history regardless of hidden/branch-filtered display state, matching "has the story actually progressed" rather than "how many bubbles happen to be visible right now."

**i18n:** add `pack.panelMinMessages` key (English source + the three other locales, per this project's i18n parity test) — short label, e.g. "Show panel after N messages".

**Test impact:** `ChatView.test.ts` (or wherever `panelPacks`/`panel-stack` is currently tested) gets a case asserting the panel is hidden when `chat.messages.length < min_messages` and shown once it meets/exceeds it. `PackEditor.test.ts` gets a case round-tripping `panel-min-messages` through `savePanel`.

## 4. Fix D — global regex pollution (`is_global` flag)

**Problem:** `effective_regex_rules` (`shirita-core/src/conversation.rs:108-117`) treats *any* `regex_rule` Definition with zero referencing prompt-tree nodes anywhere in the database as "global" and unconditionally applies it to every session — there is no actual "global" flag, "global" is computed purely as "currently unreferenced." This conflates two unrelated situations that happen to produce identical DB state:
- **Intentional**: Settings' "add rule" button (`shirita-ui/src/views/SettingsView.vue`, the `createDefinition({ type: 'regex_rule', ... })` call around line 887) deliberately creates a rule with no node reference, meaning "apply everywhere" — this is a real, wanted feature.
- **Accidental**: deleting a template/pack while choosing *not* to delete its orphaned definitions (the existing `deleteOrphans` confirm-dialog escape hatch in `BookView.vue:509-516,758-765` — clicking "keep" on that second confirm), or unbinding/removing a single `regex_rule`'s referencing node directly, leaves the rule with zero references too — and it then silently starts firing on every other session, which the user (and the rule's original author) never intended.

There is no way today to tell these apart, and no way to "undo" the accidental case other than finding and deleting the rule by hand in Settings.

**Fix:** add an explicit `is_global: bool` to `regex_rule` Definitions' `meta`, and make it — not reference-emptiness — the source of truth for "applies everywhere."

**Known trade-off (deliberately accepted, not a bug):** a `regex_rule` with `is_global: true` remains, in raw `prompt_nodes` terms, exactly as unreferenced as an accidental orphan — `is_global` is a flag bolted onto otherwise-identical DB state, not a structural anchor (the alternative — giving global rules a real `Ref` node under a reserved synthetic "Global" owner, so "referenced" stays an unambiguous signal — was considered and rejected as too invasive for this fix: new `OwnerKind` variant, a migration to attach every existing global rule to it, and changes to the Settings create/delete flow, for a problem the flag already solves at the only two call sites that currently care). Concretely, this means: **any future code that reasons generically about "unreferenced definitions" (a cleanup sweep, an integrity checker, a new orphan-detection feature) must explicitly check `def_type == "regex_rule" && meta.is_global == true` and skip those rows**, the same way `effective_regex_rules` and `list_regex_scopes` do in this fix — `referenced_definition_ids` alone is no longer sufficient to mean "safe to treat as dead." `orphaned_definitions_for_template`/`orphaned_definitions_for_pack` are unaffected today (they only ever consider definitions a *specific* template/pack's own tree references, and a global rule is referenced by no tree at all, so it can never appear in either's result) — but this exception must be carried forward by anyone adding a new orphan-aware code path later.

0. **`shirita-core/src/storage/mod.rs:24-25`**: the `Storage::referenced_definition_ids` doc comment currently reads "Lets callers tell orphan (\"global\") defs from tree-mounted ones" — that sentence is the misleading premise this whole fix corrects (unreferenced no longer means global). Replace it with:
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

1. **`effective_regex_rules`** (`shirita-core/src/conversation.rs:108-117`): change the filter from
   ```rust
   .filter(|d| d.def_type == "regex_rule" && !referenced.contains(&d.id))
   ```
   to
   ```rust
   .filter(|d| {
       d.def_type == "regex_rule"
           && d.meta.get("is_global").and_then(|v| v.as_bool()).unwrap_or(false)
   })
   ```
   The `referenced` computation (and the rest of the function — template-tree rules, mounted-pack rules) is unchanged; only this one filter's condition changes. A rule that is both `is_global: true` *and* referenced by some node (an edge case with no current UI path to create, but not prevented by storage) is intentionally still included by the global branch — `is_global` always wins, since the meaning is "this should apply regardless of where it's mounted."

2. **`list_regex_scopes`** (`shirita-web/src/routes/regex_rules.rs:24-61`): change the `scope` determination from `if names.is_empty() { "global" } else { "template" }` to checking the same `is_global` meta flag:
   ```rust
   let is_global = d.meta.get("is_global").and_then(|v| v.as_bool()).unwrap_or(false);
   let scope = if is_global { "global" } else { "template" };
   ```
   This is also where the existing Template-only blind spot lives (`refs` is built only from `OwnerKind::Template` nodes, `regex_rules.rs:33-43`, never `OwnerKind::Pack`) — fixed as part of this same change since it's the same `template_names` computation: add a second loop over `state.storage.list_packs()` / `OwnerKind::Pack` nodes, merging into the same `refs` map, so a pack-scoped rule's `template_names` correctly lists the *pack's* name too (the field stays named `template_names` — renaming the wire field is out of scope here; it now means "names of the templates/packs whose tree references this rule").

3. **Settings "add rule" button** (`shirita-ui/src/views/SettingsView.vue`, the `createDefinition` call ~line 887-897): add `is_global: true` to the `meta` object passed to `createDefinition`, so newly-created global rules are explicitly flagged from the start (today they rely entirely on starting unreferenced).

4. **One-time data migration** (new function, `shirita-core/src/seed.rs`, following the existing `ensure_*` idempotent-startup-backfill pattern used by `ensure_asset_hashes` etc.):
   ```rust
   /// One-time backfill for the `is_global` regex-rule flag (added after global
   /// rules were inferred from "zero references" instead of stored explicitly).
   /// Every regex_rule Definition that is currently unreferenced gets
   /// `is_global: true` so already-relied-upon global rules keep applying
   /// after the flag becomes the source of truth — idempotent (a rule that
   /// already has `is_global` set, true or false, is left untouched).
   pub async fn ensure_global_regex_flag<S: Storage + ?Sized>(storage: &S) -> Result<()> {
       let referenced: std::collections::HashSet<String> =
           storage.referenced_definition_ids().await?.into_iter().collect();
       for def in storage.list_definitions().await? {
           if def.def_type != "regex_rule" { continue }
           if def.meta.get("is_global").is_some() { continue }
           if referenced.contains(&def.id) { continue }
           let mut updated = def.clone();
           if let Some(obj) = updated.meta.as_object_mut() {
               obj.insert("is_global".to_string(), serde_json::json!(true));
           }
           storage.update_definition(&updated).await?;
       }
       Ok(())
   }
   ```
   Exported from `shirita-core/src/lib.rs` alongside the other `ensure_*` functions; called once at startup in both `shirita-web/src/main.rs` and `shirita-tauri/src/main.rs`, right after the existing `ensure_templates_have_content_node` call (same ordering tier — both are idempotent backfills that must run after templates/defs exist but before the server starts accepting requests).

   This migration preserves current behavior for every existing currently-orphaned rule (nothing that used to apply stops applying on upgrade) — going forward, the accidental-orphan path (deleting a pack/template and choosing not to delete its definitions, or unbinding a node) produces a rule with `is_global` absent/false, which is now correctly inert instead of silently global.

**Test impact:**
- `shirita-core/src/conversation.rs`'s existing `effective_regex_rules_global_plus_scoped` test: update the "global orphan rule" fixture to set `meta.is_global = true` explicitly (it currently relies on the old "unreferenced = global" inference) — without this, the test's premise no longer matches the fixed code.
- New test: an unreferenced `regex_rule` *without* `is_global` is **not** included by `effective_regex_rules` (the core regression test for this fix).
- `shirita-web/src/routes/regex_rules.rs`: new test asserting a pack-referenced (not template-referenced) rule with `is_global: false` reports `scope: "template"` and a non-empty `template_names` containing the pack's name (covers both the `is_global` switch and the Pack-blind-spot fix).
- `shirita-core/src/seed.rs`: new test for `ensure_global_regex_flag` — mirrors `ensure_asset_hashes_backfills_missing`'s structure: create an unreferenced regex_rule with no `is_global` key, run the backfill, assert `is_global == true`; create a second regex_rule with `is_global: false` explicitly set, run the backfill, assert it stays `false` (not overwritten); run the backfill twice total, assert idempotency (no error, no double-toggle).

## 5. Out of scope (explicitly, per this conversation)

- A JS-Slash-Runner/TavernHelper API bridge (`triggerSlash`, `generate`, `getVariables`, etc.) for `HtmlCardFrame`-rendered cards' buttons to call into the chat. Investigated: neither `examples/怪谈社.json` nor `examples/密教模拟器.json` calls any such API (confirmed by grep across both cards' `first_mes` and `regex_scripts.*.replaceString` for `triggerSlash`, `generate(`, `getVariables`, `replaceVariables`, `insertOrAssignVariables`, `TavernHelper`, `SillyTavern`, `sendMessage`, `eventOn` — only `getChatMessages` appears, twice, in 怪谈社, consistent with read-only local display logic, not sending). The "buttons don't work" report for these two cards is attributed to Fix A/B's layout bugs (content rendered too small / too narrow, making real click targets hard or impossible to hit) rather than a missing API surface. Re-test after Fix A/B ship; this bridge is deferred to a future spec if a concrete card is found that actually needs it.
- `examples/命定之诗与黄昏之歌v4.2.json` (external JS loading) — unrelated, still out of scope per the prior spec.
- Any change to `PanelView.vue`'s own internal `data-show` mechanism (per-element conditional visibility within an already-shown panel) — Fix C is about the panel *header's* mount/visibility, a different, coarser-grained gate.
