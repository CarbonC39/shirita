# Native Card Panels (Point 2 — v1) — Design

> The first slice of "stop indulging SillyTavern's HTML/JS." **v1** is a native, secure, variable-bound **status panel** owned by a Pack. v2 (ST card → native conversion) and v3 (sandboxed JS bridge) are deliberately out of scope and get their own specs later.

## 1. Background & motivation

What an ST "frontend card" actually is — read from `examples/怪谈社.json` and `examples/密教模拟器.json` — is a themed, variable-driven **status panel**, assembled out of three brittle layers:

1. the LLM emits `<update>` / `<UpdateVariable>` tags — i.e. **variable diffs**;
2. 58–115 KB of `regex_scripts` find/replace those tags and a `<StatusPlaceHolderImpl/>` placeholder into a block of **inline-styled HTML**, and also hide the raw tags from the AI;
3. `tavern_helper` JS — often `import`ed from a CDN (e.g. `MagVarUpdate/bundle.js`) — i.e. **arbitrary remote code**.

The panel is redrawn every message via first-message injection.

Shirita already owns the data layer natively (M5 typed-diff variables: `<state_update>`, `apply_updates`, per-scope schema, `effective_state`). So we can replace the regex + remote-JS stack with a **native panel that is a reactive view over the Pack's variables** — persistent (no per-message redraw), themed via scoped CSS, interactive via declarative attributes, and **running zero card JS**.

## 2. Vocabulary

- **Panel** — a Pack-owned, persistent UI surface: author HTML + scoped CSS + variable bindings + declarative actions. Stored on the Pack; rendered in chat as an inline collapsible block. **At most one per Pack (v1).**
- **Binding** — a placeholder in the panel HTML the host fills from the session's resolved variable state and keeps in sync.
- **Action** — a declarative `data-*` attribute wiring a panel element to one of the four capabilities.
- **Capability tiers** (the agreed "③ tiered" model): **read** variables (implicit) · **write** variables = typed diff (declared) · **insert** into the chat input (declared) · **send** a message (declared).

## 3. Data model

`pack.meta.panel` (optional; absent ⇒ the Pack has no panel):

```json
{
  "html": "<details>…{{hp}}…</details>",
  "css": ".sealed-box{ … }",
  "caps": { "write": true, "insert": false, "send": false }
}
```

- `html` — author markup (sanitized at render).
- `css` — author CSS (fenced at render), scoped to the panel's shadow root.
- `caps` — which non-read capabilities the panel uses. **v1 is self-authored, so declared ⇒ granted** (no consent prompt). v2 (imported packs) turns `caps` into the user-facing permission gate.

No new tables — this rides on the existing `packs.meta` JSON column.

## 4. Rendering: `<PanelView>` (Shadow DOM, zero card JS)

A new component renders one panel:

1. **Shadow root** attached to a host `<div>`. The host carries **forced inline styles** `position: relative; overflow: hidden; contain: content;` — so any `position:absolute` inside the panel is trapped to the host box, overflow is clipped, and layout/paint/style containment keeps the subtree from affecting the rest of the page (`content`, not `strict`, so the panel can still grow to its natural height). All author CSS and HTML live inside the shadow root → native style isolation (nothing leaks out; host styles don't leak in, so the author has full visual control).
2. **Sanitize** the author HTML with a vetted library (DOMPurify — a new frontend dependency; today `MarkdownText` avoids HTML entirely, so this is the first place we render author HTML and must sanitize properly). Allowed: structural/text tags, `details`/`summary`, `span`/`div`/`p`/`ul`/`li`/headings/`table`, `class`/`style`/`data-*`, and local `<img src="/assets/…">`. Forbidden: `<script>`, `<iframe>`/`<object>`/`<embed>`, `<form>`, `<link>`/`<meta>`/`<base>`, any `on*` handler, and `javascript:` / non-image `data:` URLs.
3. **Fence** the author CSS: reject/strip `@import`, `position: fixed|sticky`, external `url(http…)` (allow only `/assets/…`), `expression()`, and `behavior`. (Selector scoping itself is free via the shadow root — no prefixing needed.)
4. **Bind** reactively from the session variable state. The frontend already exposes `{ schema, values }` via `GET /api/sessions/{id}/state`. Supported bindings:
   - `{{var}}` — text interpolation.
   - `data-bind="var"` — element `textContent`.
   - `data-show="var"` — element rendered only when the var is truthy.
   On every state change (an LLM turn's diff, or a panel action), the host interpolates a fresh target tree from the sanitized template + new `values`, then **morphs** the live shadow DOM toward it with **morphdom** (a new, tiny frontend dependency) — never `innerHTML` replacement. A full destroy/rebuild would slam shut a `<details>` the user just opened, drop an active text selection or caret, and interrupt CSS transitions; morphing touches only the nodes that actually changed. An `onBeforeElUpdated` guard preserves user-interaction state the variables don't own (a `<details open>` toggled in the live DOM, focus, scroll). Action handlers are attached once via **event delegation on the shadow root**, so morphing never drops them.
5. **Actions** (event-delegated on the shadow root):
   - `data-diff-key="location"` + `data-diff-op="set"` + `data-diff-value="The Dark Forest"` → one `Update` → applied through the existing `apply_updates` pipeline (typed-validated, scoped to the pack's declared vars). `op ∈ { set, add, sub, toggle, append, remove }` (`toggle` omits the value). **Three separate attributes, not a space-delimited string**: values are arbitrary text (multi-word locations, enum names), so a `split(" ")` micro-syntax would truncate `"The Dark Forest"` to `"The"`. Requires `caps.write`.
   - `data-insert="text"` (interpolated) → set/append the chat composer input. Requires `caps.insert`.
   - `data-send="text"` (optional; interpolated) → submit a message. Requires `caps.send`.

The same `<PanelView>` powers the **live preview** in the Pack editor (WYSIWYG).

## 5. Applying a panel diff (backend)

Panel `data-diff` / `data-send` changes happen mid-conversation, outside an LLM turn, so the change needs somewhere to live on the **message tree** (the source of truth for branch state since M4/M5). v1 adds one endpoint:

`POST /api/sessions/{id}/state-updates` — body `{ "updates": [ { "action", "key", "value" }, … ] }`. It:

1. resolves the schema via `resolve_schema_with_packs` and applies the updates with `apply_updates` (typed validation; undeclared or type-mismatched keys ignored);
2. **inserts a hidden system node** — a state-carrier message, child of the current `active_leaf_id`, `is_hidden`, role `system`, carrying the post-diff state in its `snapshot_state` and no prompt-visible content;
3. **advances `active_leaf_id` to that node**;
4. returns the new `{ values }`.

The frontend then refetches state → both the panels and the read-only VariablesPanel update. Anchoring the change to a node (rather than mutating `current_state` out-of-band) keeps every transition on the branch: it survives regenerate/fork (copied and id-remapped like any node) and is reachable by branch navigation, and because it's hidden + content-less it never shows in the chat nor enters the prompt — the next assembly already reads the new state from the leaf's `snapshot_state`.

## 6. Placement (chat)

`ChatView` renders a **stack of inline collapsible panels** at the top of the chat column — one per mounted Pack that has `meta.panel`, in mount order, each headed by the Pack's name/avatar for attribution. Single column is preserved (consistent with the established UI preference: single column, no view-switching). Cinema-mode presentation (enlarge + hide the message stream) is **point 3**, not this spec.

## 7. Authoring (Pack editor)

`PackEditor` gains a **Panel** section:
- an HTML editor and a CSS editor,
- a capability toggle row (write / insert / send → `meta.panel.caps`),
- a **live preview** via `<PanelView>` bound to the Pack's declared variables (using their initial/sample values).

Saved through the existing `updatePack(id, { meta })`.

## 8. Security summary

- **No card JS in this path** — the entire script / remote-code / XSS class is removed by sanitization, not merely sandboxed.
- **Shadow DOM** scopes styles; the **CSS fence** removes the few host-affecting / exfil properties.
- **Writes are typed, pack-scoped diffs** through `apply_updates` — a panel cannot touch undeclared keys or another Pack's vars.
- **Network egress is structurally impossible**: no JS, no external `url()` / `@import`, no remote `<img>`.
- The sensitive "act as the user" capabilities (`insert` / `send`) are explicit, declared, and — from v2 — user-gated.

## 9. Out of scope (later)

- **v2** — ST card → native conversion: map `regex_scripts` / MVU / `StatusPlaceHolderImpl` to a native panel + variable schema; `<UpdateVariable>` → `<state_update>`; remote-JS imports rejected.
- **v3** — sandboxed JS bridge (opaque-origin iframe + `postMessage` + the ③ consent prompt) for cards that genuinely need logic.
- Per-message ephemeral cards; multiple panels per Pack; binary/asset embedding beyond `/assets` references; cinema mode (point 3); zip export/import (point 1).

## 10. Testing

- **Sanitizer / fence** (unit): `<script>` / `on*` / `javascript:` / remote `url()` / `@import` / `position:fixed` are stripped; safe markup and CSS pass through.
- **Binding** (component): `{{var}}` / `data-bind` / `data-show` render correctly and update when the state changes.
- **Morphing** (component): a state change while a `<details>` is open and text is selected preserves both (morphdom, not `innerHTML` replacement) — and a multi-word `data-diff-value` is applied whole.
- **Actions** (component): `data-diff-key/op/value` produces the correct `Update`; `data-insert` sets the composer; `data-send` triggers a send; each is gated by its `cap`.
- **Backend** (integration): `state-updates` applies typed diffs, ignores undeclared / type-mismatched keys, inserts a hidden system state-carrier node, and advances `active_leaf_id`.
- **Authoring** (component): the Pack-editor Panel section round-trips `meta.panel`.
- **Placement** (component): `ChatView` renders one panel per mounted Pack that has `meta.panel`, in mount order.

## 11. Decomposition (for writing-plans)

Likely plan split:
1. backend `state-updates` endpoint (hidden state-carrier node + `active_leaf_id` advance) + `pack.meta.panel` typing;
2. `<PanelView>` core — host containment + sanitize + CSS fence + shadow DOM + morphdom-based bindings;
3. actions (`data-diff-key/op/value` / insert / send) + `ChatView` placement;
4. Pack-editor Panel authoring section + live preview.
