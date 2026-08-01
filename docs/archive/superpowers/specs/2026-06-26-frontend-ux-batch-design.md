# Frontend UX batch fixes — design

Date: 2026-06-26
Branch: `fix/frontend-ux-batch`

A batch of 11 frontend bugs/enhancements found during manual testing. Most are
small, contained changes in the Vue UI (`shirita-ui`); two items touch the Rust
core for an "unlimited max output" sentinel.

## Items

### 1. Mobile: action buttons overflow → wrap to next row
`BookView.vue` has two toolbars (Template, Pack), each `EntityPicker(flex-1)` +
five fixed 33px icon buttons. On a narrow viewport the icon group is pushed off
the row. Add `flex-wrap` to these toolbars and `min-w-0` to the picker so the
button group drops to the next line when space runs out. Verify on a 375px
viewport with Playwright and fix any other row that actually overflows.

### 2. Header brand mark is a letter, not an image
`AppShell.vue` shows a letter "S" box top-left. Replace with an `<img>` of
`src/assets/favicon.svg` (Vite import), still linking to `/`, keeping the ~28px
rounded mark.

### 3. API-key dots are crammed together
The API-key `<input>` in `SettingsView.vue` is `font-mono` + `type=password`;
the masked bullets touch. Add letter-spacing only in password mode (not in
plain-text reveal, so a real key isn't widened).

### 4. Generation defaults + max-output slider
- Default temperature `0.7 → 1` (`genTemp` fallback).
- Default max output `4096 → 8192` (`genMaxTokens` fallback).
- Default message style `bubble → flat` (`ui.ts` `messageStyle` fallback).
- Max output becomes **slider + number box + "Unlimited" toggle** (linked).
  "Unlimited" is stored as sentinel `0` in `provider_max_tokens`; backend
  `conversation.rs::provider_max_tokens` and `summarize.rs` add
  `.filter(|&n| n > 0)` so `0 → None` (provider default / omit `max_tokens`).
  When the toggle is on, the slider/number are hidden.

These are defaults for fresh setups (existing saved values are unchanged).

### 5. Custom CSS: drop the hook hint text
Per decision, keep the custom-CSS textarea and global injection; only remove the
`/* hooks: .app-... */` selector list from the textarea placeholder (replace with
a neutral hint or empty). No change to `useCustomCss`.

### 6. Book forgets last-edited template/pack/definition on navigation
`BookView.vue` remounts on navigation, losing in-component selection. Persist
`selectedTemplateId`, `selectedPackId`, and the active definition id to
localStorage; on `onMounted`, restore each if the id still exists, else fall back
(template → default/first). Survives page switches and reload.

### 7. /new button glow
`HomeView.vue` new-chat SVG: add a brand-color glow (an extra `drop-shadow` using
`--color-primary` + a gentle pulse animation), keeping the existing hover lift.

### 8. Mobile: can't trigger Book node delete
`NodeRow.vue` delete button is `text-muted/0 group-hover:text-muted/70` — invisible
on touch (no hover). Change to `text-muted/40 group-hover:text-muted/70
hover:!text-coral`: always faintly visible (tappable on touch), deepens on desktop
hover, consistent with the adjacent add/expand buttons.

### 9. Scan-depth row only for keyword triggers
`DefinitionEditor.vue` shows the scan depth + recursive row for all container
types. Gate it on `triggerFromMeta(meta).mode === 'keyword'`. `wrap_in_tag`
(rendering-related) stays as is.

### 10. Template default + auto-select on new chat
Add a "set as default" star button to the Book template toolbar, writing
`template.meta.default = true` (clearing the previous default). `NewChatView.vue`
and `BookView.vue` auto-select prefer the `meta.default` template, else the first.
`updateTemplate(id, name, meta)` already persists meta — no backend change.

### 11. Template can carry regex
- `PromptTree.vue` omnibox gains a "Regex" brick (beside the existing Variables
  brick) → creates a `regex_rule` definition + ref node in the template tree.
  Backend `effective_regex_rules` already picks up `regex_rule` refs in the
  effective tree, so it scopes to sessions using that template.
- `NodeRow.vue`: for a ref whose definition is `regex_rule`, the expanded area
  renders a compact regex editor (name / find / replace / apply-to) instead of the
  generic content textarea, bridged via `metaToRule` / `scopeFlagsToMeta`. A new
  emit lets `BookView.vue` call `updateDefinition` to persist the definition meta
  (and name). Editing targets the global definition in both template and local
  trees (the rule is the user's freshly created def).

## Testing
- Update/add unit tests per item where it has logic (4, 6, 9, 10, 11, and the
  backend `0 → None` filter).
- Pure-visual items (1, 2, 7, 8) verified by running the app in desktop + 375px
  mobile viewports with Playwright.

## Out of scope
- The unmerged `regex-variables-overhaul`, `fix+frontend-render-and-regex`, and
  `st-status-panel-conversion` worktrees/branches — this work branches off `main`.
