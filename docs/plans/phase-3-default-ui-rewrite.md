# Phase 3 implementation plan: default UI rewrite

> Status: implemented
> Date: 2026-08-01
> Scope: rebuild the application shell and chat workspace around a compact, mobile-first default layout

## Goal

Make Shirita's default UI small, predictable, and inexpensive to maintain. The chat transcript and text input are the primary workspace; navigation, message actions, panels, variables, and diagnostics must not permanently compete with them.

This phase is a structural layout rewrite, not a visual-design project. It should prefer ordinary document structure, a small neutral token set, and explicit component ownership over decorative effects or viewport-specific patches. Users may still customize the result with CSS, but the unmodified UI must work on both self-hosted Web and Tauri.

Phase 3 resolves the three layout defects deliberately deferred from Phase 1:

1. the mobile top navigation consumes too much vertical space;
2. mobile message actions are constrained by the message/content width instead of using the screen edge safely;
3. Composer controls and reserved whitespace leave too little room for text entry.

## Required invariants

The phase is complete only when all of these are true:

- The chat page has exactly one vertical scroll owner for the transcript. `AppShell`, `ChatView`, and `MessageList` do not form nested vertical scrollers on that route.
- The Composer and compact chat header remain reachable while the transcript scrolls.
- Mobile navigation uses one compact row and respects `safe-area-inset-top`; breadcrumbs never add a second permanent header row on the chat route.
- Every message action remains available on mobile through a viewport-level action surface that respects left, right, and bottom safe areas.
- Message actions are operable by click/tap and keyboard. No action depends on hover or long-press.
- The Composer gives the textarea the flexible width, grows vertically to a bounded height, and adds no permanently empty token/status row.
- Draft-token information and conversation-token information remain available without reserving blank layout space.
- Panels and variables remain usable, including panel `diff`, `insert`, and `send` actions, but they do not permanently reduce transcript height.
- Initial/refresh/generation error recovery, bottom-following scroll behavior, hidden-message recovery, branching/swipes, edit, regenerate, fork, copy, delete, attachments, Stop, and HTML cards retain their Phase 1 behavior.
- Bubble and flat message styles, light/dark themes, backgrounds, configurable content width, locale switching, and custom CSS remain supported.
- Desktop and mobile use the same component behavior and event paths; there is no second mobile-only chat implementation.
- The native application contains no newly added dependency solely for layout, drawers, menus, or media-query detection.

## Out of scope

- Prompt composition changes, Prompt modes, or an advanced/basic mode switch.
- Tool registration, state schema redesign, provider event normalization, or token-accounting changes.
- Changing the meaning or persistence of UI settings.
- Replacing Tailwind, Vue, Pinia, router structure, icon library, Markdown rendering, or the custom-CSS injection mechanism.
- A theme marketplace, user-authored layout editor, plugin UI API, or large component/design system.
- Redesigning every Book and Settings form. They should inherit the simpler shell and shared primitives, but Phase 3 does not rewrite their domain workflows.
- Backend, database, API, or Tauri command changes unless a UI regression exposes a correctness defect that cannot be fixed within the frontend contract.

## Layout contract

### Application shell

`AppShell` owns the viewport, global background, primary navigation, and the outer content-width constraint.

- Use a single `100dvh` flex/grid shell with safe-area padding.
- Keep primary navigation in one compact bar. The brand and the three current destinations remain; do not add labels that force a taller mobile bar.
- Desktop breadcrumbs may remain inline in that bar. Non-chat mobile pages may render a compact breadcrumb row in their own scrolling content, but the chat route must not receive it.
- The route content host must expose two explicit overflow modes:
  - ordinary pages: the host scrolls;
  - chat: the host is `min-height: 0; overflow: hidden`, and the transcript owns scrolling.
- Continue applying `ui.contentWidth` to the centered shell/content boundary. On narrow screens, width is always capped by the viewport; no horizontal scroll may be introduced by a saved desktop width.

### Chat workspace

The chat route is a three-region layout:

```text
compact chat bar
transcript (the only flexible, vertically scrolling region)
composer
```

Transient notices may appear adjacent to the chat bar or Composer, but panels and variables must open in an overlay/drawer rather than becoming permanent fourth and fifth rows.

### Secondary chat information

Add one explicit control in the compact chat bar for session information. It opens a shared secondary-information surface containing:

- visible session panels in their existing resolution order;
- variables, grouped into system and custom values as today;
- a non-empty state when only one of those sources exists, and no trigger when neither exists.

Implement this as one small component (for example `ChatDetailsDrawer.vue`) controlled by `ChatView`. It may be a modal sheet on narrow screens and a side drawer on wider screens, but it must use one DOM/event implementation. The surface must:

- be outside the transcript flow and layered above the application shell;
- have a labelled close button, close on `Escape`, and restore focus to its trigger;
- prevent background interaction while open;
- scroll internally when its own content exceeds the viewport;
- preserve existing `PanelView` events exactly.

Do not move session state into a new store merely to open the drawer. `ChatView` already owns the resolved panels and state.

## Task 1: lock the responsive shell contract

### Tests first

Extend `shirita-ui/src/components/AppShell.test.ts` with behavior-oriented assertions:

1. the route host exposes ordinary-page scrolling on `/book`;
2. the route host exposes the non-scrolling workspace mode on `/chat/:id`;
3. chat does not render the mobile breadcrumb row;
4. a non-chat route with crumbs still renders them;
5. primary navigation and active-chat return behavior remain unchanged.

Use stable attributes such as `data-test="route-host"`, `data-layout="page|workspace"`, and `data-test="mobile-crumbs"`. Do not assert Tailwind class strings or simulated pixel sizes in jsdom.

### Implementation

- Simplify `AppShell.vue` to one viewport container, one compact top bar, and one route host.
- Derive the route-host layout mode from the named chat route, not from a global CSS selector matching incidental descendants.
- Remove the decorative centered divider and excess top/bottom spacing from the navigation.
- Keep the existing active-conversation memory behavior and brand/navigation semantics.
- Put shared colors, spacing, focus treatment, safe-area behavior, and basic controls in `styles.css`. Avoid recreating the old appearance through a new collection of one-off utility combinations.

### Acceptance

- At 320 px CSS width, the top bar stays on one line and does not horizontally scroll.
- At desktop width, breadcrumbs truncate instead of displacing navigation.
- `/book`, `/settings`, `/new`, and `/` remain independently scrollable.
- `/chat/:id` cannot scroll at the shell/content-host level.

## Task 2: establish the chat workspace and secondary-information drawer

### Tests first

Update `shirita-ui/src/views/ChatView.test.ts` and add focused tests for the new drawer component:

1. ChatView renders the stable three-region workspace markers (`chat-bar`, `transcript-region`, `composer-region`);
2. the details trigger is absent when there are no panels or variables;
3. the trigger opens a surface containing both resolved panels and grouped variables;
4. close button and `Escape` close it and return focus;
5. existing panel `diff`, `insert`, and `send` events still reach ChatView handlers;
6. opening/closing details does not remount `MessageList` or `Composer` and does not clear an entered draft;
7. initial-load, refresh, and streaming-error recovery remain visible in the appropriate workspace region.

Keep `VariablesPanel` only if it becomes reusable drawer content. Otherwise replace it with a presentational variables section and remove the obsolete permanent-panel wrapper and tests.

### Implementation

- Rebuild `ChatView.vue` around an explicit `min-height: 0` three-region layout.
- Move the current always-open panel stack and bottom `VariablesPanel` into the secondary-information surface.
- Keep panel/state/identity loading parallelism and all existing panel action handlers.
- Keep the back action, assistant identity, and optional details trigger in a short chat bar.
- Place fork/load/refresh notices so they consume space only while present. Generation failure remains next to the Composer/transcript boundary and must retain Dismiss.
- Prefer native Vue state and a small overlay component. Do not add a general overlay framework during this phase.

### Accessibility boundary

If a native `<dialog>` is used, its WebKit/Tauri behavior must be manually verified. If a custom modal is used, it must provide `role="dialog"`, `aria-modal="true"`, an accessible name, focus entry, focus restoration, Escape handling, and a backdrop. A partial focus trap must not be invented: either implement and test a complete minimal trap or use the native dialog behavior.

## Task 3: make MessageList the sole transcript scroll owner

### Tests first

Preserve all Phase 1 cases in `MessageList.test.ts` and extend them:

1. the root marked `message-scroll` is the only element in MessageList with transcript scrolling responsibility;
2. changing drawer/notices/identity props outside MessageList does not replace the scroller node;
3. streaming growth and streaming-to-persisted replacement still follow when near the bottom;
4. both still preserve position while the user is reading above;
5. switching to another session through keyed routing begins at that session's bottom;
6. oversized Markdown, code blocks, attachments, and HTML cards do not create horizontal page overflow.

The route-level single-scroller contract belongs in `ChatView.test.ts`/`AppShell.test.ts`; jsdom cannot prove real layout, so complement it with the manual viewport matrix below.

### Implementation

- Retain the Phase 1 bottom-following algorithm and local `isFollowingBottom` state.
- Give MessageList `min-height: 0`, vertical overflow, and safe horizontal transcript padding; remove scroll ownership from all chat ancestors.
- Keep scroll writes coalesced after DOM updates. Do not replace this with unconditional `scrollIntoView`, which can scroll ancestors and reintroduce the completion jump.
- Preserve a stable scroller DOM node across empty, loading, error, streaming, and populated states whenever the route remains mounted.
- Keep conversation-token display attached to real content or a compact status/action surface; never restore a fixed-height placeholder row.

## Task 4: move mobile message actions out of message content

### Interaction contract

Desktop may keep a compact inline action row. On narrow/coarse-pointer layouts, each non-streaming message exposes an explicit labelled `More actions` button. Activating it opens one viewport-level action sheet rendered outside the message/bubble subtree (Vue `Teleport` to `body` is appropriate).

The sheet owns the selected message ID and exposes only actions valid for that message:

- previous/next variation and the current variation count when assistant siblings exist;
- regenerate for assistant messages;
- fork for assistant messages;
- copy;
- edit;
- hide/unhide;
- delete.

Selecting edit may close the sheet and activate the existing inline editor for that message. Destructive confirmation remains owned by ChatView. The sheet must close after invoking an action, when tapping its backdrop, on `Escape`, and when its selected message disappears or the route changes.

### Tests first

Refactor `MessageItem.test.ts` and `MessageList.test.ts` so tests cover behavior rather than duplicated markup:

1. one explicit action trigger selects a message;
2. the teleported sheet is a direct viewport-level surface, not a child of `msg-bubble-wrapper` or message content;
3. user and assistant messages expose the correct action sets;
4. variation controls retain disabled boundaries and emit the same `swipe` events;
5. all existing events carry the selected message ID exactly once;
6. invoking edit opens the correct message editor;
7. backdrop, close button, and Escape dismiss without firing an action;
8. the sheet has an accessible name and actions have visible or accessible labels.

Because `matchMedia` is not reliable as the only source in jsdom and two rendered action implementations can drift, prefer CSS media queries for presentation. Keep a single action description/event map in JavaScript and reuse it for inline and sheet rendering where practical.

### Implementation

- Separate message content from message controls in `MessageItem.vue`; content width must not define the mobile control surface width.
- Let `MessageList` (or one dedicated `MessageActions.vue`) own selection and the teleported sheet so only one sheet exists per transcript.
- Keep stable custom-CSS hooks on message rows and add a documented hook for the sheet.
- Account for `safe-area-inset-left`, `safe-area-inset-right`, and `safe-area-inset-bottom`. The backdrop spans the viewport; the sheet content remains reachable at 320 px width and with 200% text zoom.
- Do not use hover-only reveal, swipe gestures, or long-press as the only way to discover actions.

## Task 5: rebuild Composer around the textarea

### Layout contract

The Composer is one compact surface. The attachment control, textarea, and send/Stop control share the main row. Optional attachment previews appear only when attachments exist. Draft-token text appears only when the draft is non-empty and shares an existing status line/surface; it must not create an empty fixed-height row.

The textarea:

- starts at one comfortable row;
- owns all remaining horizontal width;
- auto-grows up to a viewport-aware bound (use a CSS maximum such as `min(40dvh, 12rem)` rather than only a hard-coded pixel value);
- becomes internally scrollable above that bound;
- remains at least 16 px on coarse pointers to avoid iOS zoom;
- resets to its minimum height after send while preserving attachment-only send behavior.

### Tests first

Extend `Composer.test.ts`:

1. no token/status placeholder exists for an empty draft;
2. token information appears for non-empty text without replacing the textarea or send control;
3. autosize clamps to the documented maximum and resets after send;
4. attachments render only when present and can be removed;
5. attachment upload failure, attachment-only send, Enter/Shift+Enter, disabled state, Stop, and `setText()` retain their behavior;
6. Stop occupies the same control slot as Send rather than changing Composer width;
7. the textarea remains the flexible element in the main control row (assert a stable semantic class/data attribute, not a full utility string).

### Implementation

- Remove the permanent `h-[18px]` draft-token row and its asymmetric `pl`/`pr` spacers.
- Base autosizing on the element's `scrollHeight`, reset it after send, and set overflow according to whether the cap is reached.
- Keep upload state and pending attachments local. Do not introduce a draft store in this phase.
- Preserve safe-area bottom padding, but avoid adding it twice between AppShell and Composer.
- Keep Send/Stop icon buttons at an accessible target size even while reducing surrounding whitespace.

## Task 6: simplify default CSS and preserve extension hooks

### CSS boundary

Rewrite the default shell/chat CSS around a small set of semantic hooks and tokens. Component templates may continue using utilities for local details, but the structural layout must be readable from named classes rather than dispersed combinations of `h-full`, `min-h-0`, and overflow utilities across several ancestors.

Preserve these documented hooks:

- `[data-app="shell"]`;
- `.app-chat-column`;
- `.app-message[data-role]`;
- `.app-composer`.

Add stable hooks for the top bar, transcript scroller, message-action sheet, and chat-details surface. Update the README's Custom CSS section with the final hook list and explicitly state that internal Tailwind classes are not a compatibility API.

### Tests and cleanup

- Keep `useCustomCss` injection/cache tests unchanged.
- Add a small DOM-contract test for the documented hooks; do not snapshot full class attributes or generated CSS.
- Remove CSS rules, animation classes, component branches, locale keys, and tests made unreachable by the old layout.
- Keep visible focus, reduced-motion behavior, light/dark variables, Markdown classes, and the coarse-pointer 16 px input safeguard.
- Do not promise compatibility for arbitrary selectors into old internal wrappers. Record the intentional boundary in README and the Phase 3 handoff notes.

## Integration order

Implement in reviewable commits. Each commit must leave the frontend tests and build green.

1. Add semantic layout/action/composer regression tests and stable DOM markers without changing behavior where practical.
2. Simplify `AppShell` and establish route-specific overflow ownership.
3. Rebuild `ChatView` and move panels/variables into the secondary-information drawer.
4. Make MessageList the sole transcript scroller and revalidate Phase 1 anchoring.
5. Extract mobile message actions into the viewport-level sheet.
6. Compact Composer and remove reserved empty status space.
7. Consolidate default CSS, remove obsolete layout code, update all locale files and documentation.

Do not combine the whole rewrite into one commit. In particular, scroll ownership and message-action extraction should be independently reviewable because regressions in either can make chat unusable on mobile.

## Automated verification

Run from `shirita-ui/` after every task-sized commit:

```bash
npm test
npm run build
```

Before Phase 3 handoff, also run from the repository root:

```bash
cargo test --workspace
git diff --check
```

The Rust suite is a regression guard; Phase 3 should ordinarily produce no Rust source or migration diff.

## Manual viewport matrix

Automated component tests do not calculate layout, safe areas, soft keyboards, or real scroll geometry. Verify the completed phase in at least these environments:

| Environment | Viewport / condition | Required checks |
| --- | --- | --- |
| Browser responsive mode | 320 × 568 | one-line navigation, no horizontal overflow, action sheet reachable, textarea usable |
| Browser responsive mode | 390 × 844 | safe-area spacing, details sheet, attachments, long streaming reply |
| Desktop browser | 1280 × 800 | content-width setting, inline actions, drawer, keyboard navigation |
| Desktop browser | 200% text zoom | controls remain reachable and sheets scroll internally |
| Self-hosted Web | touch device if available | soft keyboard, Enter behavior, Composer growth, backdrop dismissal |
| Tauri/WebKit | smallest supported window | resize, scrolling, dialog/sheet behavior, Stop during streaming |

For both message styles and both themes:

1. open a long existing conversation and confirm only the transcript scrolls;
2. stream a reply at the bottom, then repeat while reading above;
3. complete, stop, regenerate, and swipe without a scroll reset;
4. open actions for the first and last message and perform edit/hide/unhide/copy;
5. open panels/variables, run each permitted panel action, then close and confirm the draft/transcript are unchanged;
6. attach/remove/send an image and send an attachment without text;
7. trigger load and generation errors and recover through Retry/Dismiss;
8. navigate Chat → Book/Settings → Chat and confirm the active conversation return link still works.

## Completion and handoff

Before marking this plan implemented:

- update this status from `proposed` to `implemented`;
- update `docs/README.md` and `docs/current-direction.md` so Phase 3 is described as completed behavior rather than future intent;
- update README screenshots or layout descriptions only if they exist and are now inaccurate;
- summarize any intentional custom-CSS compatibility break and list the supported hooks;
- record the manual environments actually checked, especially Tauri/WebKit and a real touch browser;
- confirm there are no backend, API, database, Prompt, tool/state-semantics, or ST-compatibility changes hidden in the UI rewrite.

## Review checklist

- [ ] Scope is limited to the default shell/chat layout and closely related CSS/docs.
- [ ] AppShell uses route-specific page/workspace overflow modes.
- [ ] MessageList is the only transcript scroll owner.
- [ ] Phase 1 bottom-following and recovery tests still pass.
- [ ] Mobile message actions use one viewport-level, accessible surface.
- [ ] Composer reserves no empty token/status row and prioritizes textarea area.
- [ ] Panels and variables remain functional without permanently shrinking chat.
- [ ] Token visibility remains available.
- [ ] Existing UI settings and documented custom-CSS hooks remain supported.
- [ ] All four locales remain in parity.
- [ ] Web build, UI tests, Rust workspace tests, and diff checks pass.
- [ ] Manual mobile, desktop, zoom, and Tauri/WebKit checks are recorded.

