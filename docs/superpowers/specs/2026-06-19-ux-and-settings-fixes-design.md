# UX & Settings Fixes — Design

Date: 2026-06-19
Branch: `ux-and-settings-fixes`
Status: design (awaiting review)

A batch of 11 independent UX/settings fixes the user noticed after the
regex/variables work merged. Each is small and self-contained; this doc records
the decision and approach per item, then a suggested implementation order.
Decisions marked **[confirmed]** were chosen by the user during brainstorming.

---

## 1. Split avatar & background libraries; crop avatars on import

**Problem.** One shared media store backs both avatars and backgrounds
(`stores/media.ts`: *"used by both avatars and backgrounds"*); `AssetPicker.vue`
is reused for both via a `shape` prop. The user wants them treated as two
libraries, and avatars to be croppable.

**Decision [confirmed].** Tag each asset with a `kind` (`avatar` | `background`);
keep one upload store/table but filter pickers by kind. An asset can be
re-tagged. Avatars get a square crop step on upload.

**Approach.**
- Backend: add a `kind` column to `assets` (migration; default `background` for
  existing rows, or `null` = unscoped shown in both). Imported character-card
  PNGs (`import_export.rs::save_png_asset`) register their asset as `avatar`.
  `listAssets` accepts an optional `kind` filter; upload accepts a `kind`.
- Frontend: `media.ts` keys cached lists by kind (or filters client-side);
  `AssetPicker` takes a `kind` prop and only shows/uploads that kind. Background
  picker in Settings → `kind="background"`; `AvatarPicker` → `kind="avatar"`.
- Cropping: a lightweight client-side square cropper (canvas; zoom + drag)
  shown when uploading into the **avatar** library. Produces a square image
  (e.g. 512×512) before upload. Background uploads skip cropping. No new heavy
  dependency — a small in-house cropper component (`<ImageCropper>`).

---

## 2. Rename buttons for template & definition

**Problem.** The template name is an always-editable inline `<input>`
(`BookView.vue:943`). The definition "name" is a *merged search + rename
combobox* (`DefinitionEditor.vue:115`) — typing both renames the current def and
filters the list, which is confusing ("can't rename"). The two text inputs also
stack vertically and read as ugly.

**Decision.** Show each name as a heading/label with a pencil **Rename** button
on the right that toggles inline edit. Separate the definition search field from
the name so searching no longer mutates the current name.

**Approach.**
- Template: replace the bare input with `name text … [✎ Rename]`; clicking
  reveals the input (Enter/blur commits via existing `renameTemplate`).
- DefinitionEditor: split the combobox into (a) a dedicated search field that
  only filters and (b) a name display + Rename button that edits
  `definition.name`. Keeps existing `update:name` emit.

---

## 3. Center area backing over the background

**Problem.** `AppShell.vue:36-39` paints the background image full-screen plus a
flat `bg-surface/75` scrim over the whole viewport. Readability of the chat
column depends entirely on that global scrim.

**Decision [confirmed].** Give the chat column its own semi-transparent panel
(~85% surface); the background shows fully in the side gutters and faintly
through the column. Drop reliance on the full-screen scrim for the chat column.

**Approach.**
- Add a panel background to the centered content column (chat view container,
  `ChatView.vue:118`) using a surface tint (`bg-surface/85` or a token) so text
  always has a consistent base. Keep the gutters showing the image.
- Reduce/remove the global scrim where the panel now covers readability, or keep
  a lighter global tint for non-chat pages. Ensure dark theme variant works
  (surface token already flips in `.dark`).

---

## 4. Make custom CSS actually apply, and easy to write

**Problem.** `custom_css` is saved and has an editor (`SettingsView.vue`), but it
is **never injected into the DOM** — `custom_css`/`customCss` appears only in the
editor and locale strings; `App.vue` only runs `useTheme()`. So overrides never
take effect. Even once injected, the Tailwind utility-class markup gives authors
no stable selectors.

**Decision.** Inject the custom CSS, and add stable CSS hooks so overrides are
easy to write.

**Approach.**
- A `useCustomCss()` composable (called from `App.vue`) maintains a single
  `<style id="user-custom-css">` element whose text content tracks the
  `custom_css` setting. It watches the settings store's reactive `data.custom_css`
  (already a Pinia ref) and updates the element on load and on every edit.
- Add stable, documented hooks on key structural elements: e.g.
  `data-app="shell"`, `.app-chat-column`, `.app-message`, `.app-composer`,
  `data-role="user|assistant"`. Document the available hooks (a short list in
  the CSS editor placeholder/help and/or `docs/`).

---

## 5. Wider center column + configurable width

**Problem.** Chat is capped at `max-w-[600px]` (`ChatView.vue:118`); too narrow
on desktop. No width setting.

**Decision [confirmed].** Default ~760px, adjustable in Settings; applies to chat
and the book editor. Settings page stays narrow.

**Approach.**
- New setting `appearance_content_width` (px, e.g. range 560–1100, default 760).
  Persisted like other appearance settings; cached in the UI store so it paints
  immediately.
- Chat and Book containers use the value (inline `max-width` / CSS var
  `--content-width`) instead of the hardcoded cap. Settings view keeps its own
  narrow width.
- Settings UI: a slider/number under Appearance.

---

## 6. Ignore empty content on import

**Problem.** `charcard.rs:120` creates the `char` description def
*unconditionally* (`unwrap_or("")`), unlike every other field (guarded by
`nonempty()`). A card with no/empty description → an empty def left in the
template. `regex_rule`/`first_message` legitimately have empty *content* (their
payload is in `meta`) and must not be dropped.

**Decision.** Don't create content-bearing definitions with empty content on
import; preserve meta-only types.

**Approach.**
- In `charcard_to_loreset`, guard the description def with `nonempty` like the
  others. Preserve the avatar/`st_raw` anchoring that currently targets the
  `char` def: if there's no description def, attach `avatar`/`st_raw` to a
  fallback (e.g. synthesize a minimal char def only when there's metadata to
  carry, or move the anchor onto template meta).
- Optionally, a defensive filter in `persist_loreset`/`persist_defs` that skips
  defs whose `content` is empty **and** whose type isn't meta-only
  (`regex_rule`, `first_message`). Keep it minimal to avoid surprising drops.

---

## 7. Comments stripped at prompt assembly

**Problem.** No way to leave authoring notes in prompt/definition content that
don't reach the model.

**Decision [confirmed].** Support `{{// ... }}` comments (SillyTavern-style),
stripped during assembly. Works inline or as whole lines.

**Approach.**
- A `strip_comments(&str) -> String` in `assembly.rs`, applied to definition
  content during render (in `effective_def_content` or right before/after
  `render_vars` in `assemble_from_nodes`, and for depth inserts). Remove
  `{{// ... }}` spans; collapse a comment that occupies a whole line so no blank
  line is left behind. Add unit tests (inline, whole-line, multiple,
  unterminated tolerance).
- Order vs `{{var}}`: strip comments before variable rendering so a comment can
  contain `{{var}}`-looking text without being substituted.

---

## 8. Active nav icon black, others gray

**Status.** Appears **already implemented**: `AppShell.vue:53-61` gives the
active section `text-ink` (near-black) and the others `text-muted` (gray),
switched by route (`section` computed). 

**Decision.** Audit, don't rebuild. Verify section detection covers all
sub-routes (`/chat/:id`, `/book/...`, `/settings`) and that contrast holds over
the new semi-transparent header/panel (#3). **Open:** the user should confirm
the specific symptom on review (too-subtle contrast? a route that doesn't
switch?). If a concrete bug surfaces, fix it; otherwise strengthen the
active/inactive contrast slightly.

---

## 9. System/browser notifications

**Problem.** No notification support.

**Decision [confirmed].** Notify when an assistant reply finishes **only if the
tab/window isn't focused**. Off by default; opt-in toggle in Settings.

**Approach.**
- Setting `notify_enabled` (bool, default false). When enabled, request
  `Notification.permission` on toggle-on.
- Fire `new Notification(...)` from the chat completion path (where streaming
  resolves) guarded by `document.visibilityState === 'hidden'` (or
  `!document.hasFocus()`) and permission granted. Title = conversation name,
  body = a short snippet. Clicking focuses the window/route.
- Settings UI: a toggle under Appearance (or a new Notifications group), with a
  hint when permission is denied/blocked.

---

## 10. Per-provider settings (isolation + saving)

**Problem.** Settings are a flat global KV store; provider config is read from
single keys `provider_source`/`provider_base_url`/`provider_api_key`/
`provider_model` (`routes/provider.rs`, `summarize.rs`). Switching source
overwrites base_url and shares one key/model across all providers — no
isolation, and the user perceives settings as "not saved." **Open:** user to
describe the exact "can't save" symptom; suspected to be the isolation effect
(switch provider → previous key/model appears lost). A genuine persistence bug
(if reproduced) is fixed under this item too.

**Decision [confirmed].** Each provider source remembers its own base URL, API
key, and model. Switching source swaps the active set; nothing is lost.

**Approach.**
- Storage: keep `provider_source` as the active selection, and store per-source
  config under namespaced keys `provider.<source>.base_url`,
  `provider.<source>.api_key`, `provider.<source>.model`. (Namespaced keys, not a
  single JSON blob, so concurrent writes don't read-modify-write each other —
  consistent with the existing flat KV store.) Migrate the existing flat
  `provider_*` keys into the current source's namespace on first load, then read
  from the namespace thereafter.
- Backend resolution (`provider.rs`, `summarize.rs`, chat send path): read the
  active source, then that source's namespaced config (fall back to defaults).
- Frontend (`SettingsView.vue`): the provider computeds read/write the active
  source's namespace; changing source loads that source's saved values instead
  of resetting only base_url. Model fetch + autosave watches updated to the
  namespaced shape.
- Verify the round-trip (set → reload → present) with a test.

---

## 11. Auto-compaction (summarize) settings

**Problem.** Backend already supports rolling summarization — `summarize.rs`
reads `context.window` (200k), `context.threshold` (0.8),
`context.keep_recent` (10), `summarize.instruction` — but there's **no UI**, so
users can't tune or disable it. (Note a latent mismatch: `summarize.rs` reads
`provider_max_tokens` while the UI saves `gen_max_response_tokens`; fix while
here.)

**Decision.** Add a "Context / auto-summarize" section in Settings exposing the
existing knobs plus an enable toggle.

**Approach.**
- Settings UI: enable toggle (`summarize.enabled`, default true to match current
  always-on behavior), `context.window`, `context.threshold` (as a percentage),
  `context.keep_recent`, and the `summarize.instruction` textarea.
- Backend: honor `summarize.enabled` (skip `summarize::run` when false). Align
  the max-tokens key so summarization uses the same configured limit.

---

## Suggested implementation order

Grouped so related files change together; each group is independently
shippable/testable.

1. **Backend data/logic:** #6 import empty-def fix, #7 comment stripping,
   #10 per-provider storage + resolution, #11 summarize settings + enable +
   token-key fix, #1 `assets.kind` column/migration.
2. **Settings UI:** #10 per-provider form, #11 context section, #9 notify
   toggle, #5 width control, #4 CSS hooks documented.
3. **Appearance/runtime:** #4 custom-CSS injection, #3 center panel, #5 apply
   width, #9 notification firing, #8 nav-icon audit.
4. **Libraries & editors:** #1 kind-filtered pickers + avatar cropper,
   #2 rename buttons.

## Testing

- Rust: unit tests for `strip_comments` (#7), charcard empty-field guard (#6),
  per-provider resolution round-trip (#10), summarize enable/skip (#11).
- Frontend: component tests for kind-filtered `AssetPicker` (#1), rename toggle
  (#2), width setting application (#5), custom-CSS injection (#4); existing
  i18n/parity tests extended for any new setting strings.
- i18n: every new user-facing string added to `en` (source) and the three other
  locales (parity test enforces this).

## Out of scope

No new heavy dependencies; cropper and CSS injection are in-house. No redesign of
the prompt tree, regex engine, or summarization algorithm — only exposure/config
and the targeted bug fixes above.
