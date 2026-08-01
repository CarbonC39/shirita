# Phase 1 implementation plan: chat correctness and recovery

> Status: proposed implementation plan for review  
> Date: 2026-08-01  
> Scope: four user-visible correctness fixes only

## Goal

Make the existing chat path dependable before removing SillyTavern compatibility or rebuilding the UI.

This phase fixes:

1. hiding a message truncates the visible ancestor path;
2. load or generation failures do not provide a reliable recovery path;
3. replacing a streaming reply with the persisted reply loses the intended scroll position;
4. post-history system content can appear after the latest user turn in the provider request.

The implementation must be incremental. It must not redesign the message schema, Prompt tree, application shell, Composer layout, tool/state system, or provider event protocol.

## Required invariants

The phase is complete only when all of these are covered by automated tests:

- Walking from an active leaf always reaches all existing ancestors, regardless of `is_hidden`.
- A hidden message remains addressable in the UI so the user can unhide it, but it remains excluded from model context as it is today.
- A failed initial transcript load can be retried without navigating away.
- A failed background refresh does not destroy an already displayed transcript.
- A generation error can be dismissed/reset, and a later send or regenerate starts with a clean error state.
- When the user is following the bottom of the transcript, streaming updates and the final persisted-message replacement keep the view at the bottom.
- When the user has intentionally scrolled upward, streaming updates do not pull the view back down.
- When a current user turn exists, the provider-visible conversational sequence ends with that turn. Later system/protocol material must not separate it from generation.

## Out of scope

- Splitting `is_hidden` into new database fields or adding a migration.
- Changing regenerate/swipe semantics beyond preventing regressions.
- Deleting ST-specific depth inserts; that belongs to Phase 2.
- Replacing the two existing scroll containers; the single-scroll-container layout belongs to the UI rewrite.
- Mobile navigation, message-action placement, Composer density, Panel layout, or default CSS redesign.
- Prompt composition redesign, new Prompt modes, slot editors, or Prompt inspector work.
- Native tool calls or provider event normalization.
- Token usage UI changes.

## Task 1: preserve ancestors when a message is hidden

### Current behavior

The frontend helper in `shirita-ui/src/utils/tree.ts` walks from the active leaf toward the root. It stops when the next ancestor has `is_hidden = true`. As a result, hiding an intermediate message removes every earlier ancestor from `chat.displayed`.

The Rust helper in `shirita-core/src/tree.rs` does not stop at hidden ancestors. Backend context assembly walks the complete branch and filters hidden messages separately in `conversation.rs`. Phase 1 should make the frontend traversal match that separation without changing backend storage semantics.

### Tests first

Add tests to `shirita-ui/src/utils/tree.test.ts`:

1. `activePath keeps ancestors across a hidden intermediate message`
   - construct `root user -> hidden assistant -> leaf user`;
   - select the leaf;
   - expect IDs `root, hidden, leaf`, not only `leaf`.
2. `activePath still falls back deterministically when the newest message is hidden`
   - document the intended fallback explicitly;
   - prefer the newest non-hidden message when no active leaf is known, matching the current frontend contract.

Add a store regression test to `shirita-ui/src/stores/chat.test.ts`:

- load a three-message linear branch, toggle the middle message through the mocked `editMessage` endpoint, and assert that `store.displayed` still contains the full root-to-leaf chain.

Do not test only the helper: the store-level test must prove the original user action no longer loses messages.

### Implementation

- Remove the hidden-ancestor early termination from `activePath()`.
- Keep the current active-leaf lookup and newest-non-hidden fallback unless the new tests expose a separate defect.
- Do not filter hidden nodes out of `activePath()`. `MessageItem` currently renders them with reduced opacity and provides the unhide control; filtering them would make unhide impossible.
- Do not change the backend `conversation.rs` filters that exclude hidden messages from model context.

### Acceptance

- Hiding the first, middle, or latest message never removes its ancestors from the transcript.
- The hidden row remains visible and can be unhidden.
- Sending or regenerating still excludes hidden content from the assembled model context.

## Task 2: make load and generation errors recoverable

### Current behavior

`chat.loadMessages()` sets a shared `error`, and `ChatView.vue` replaces the chat content with a plain error paragraph. There is no retry action. If a refresh fails after messages were already loaded, the stored messages remain in Pinia but the view hides them behind the error branch.

Generation failures use `streamingError`, but there is no explicit reset action. Subsequent `consume()` calls clear it as an implementation side effect; the UI does not make recovery discoverable.

### State contract

Keep the two existing error channels rather than introducing a new state-machine abstraction in Phase 1:

- `error`: transcript/session loading failure;
- `streamingError`: send/regenerate failure.

Add explicit store actions:

- `retryLoad()` or an equivalently narrow action that reloads `activeSessionId`;
- `clearStreamingError()`;
- optionally `clearLoadError()` only if needed by the view implementation.

`loadMessages(sessionId)` must retain the last successfully loaded transcript if a later refresh fails. A load error with cached messages is non-destructive and should be shown alongside the transcript. An initial load error with no messages may use a full empty error state.

### Tests first

Add store tests in `shirita-ui/src/stores/chat.test.ts`:

1. `loadMessages exposes an initial load error and retry succeeds`
   - first `listMessages` call rejects;
   - retry resolves with messages;
   - assert `error` is cleared and messages render.
2. `failed refresh preserves the last successful transcript`
   - load successfully;
   - make the next list call reject;
   - assert the old messages remain and the error is exposed.
3. `clearStreamingError dismisses a generation error`.
4. `a later send starts with no stale generation error`.

Update `shirita-ui/src/views/ChatView.test.ts`:

- replace the existing error-text-only test with an initial-error test that also finds and activates a retry button;
- add a cached-transcript refresh-error test proving message content remains visible;
- add a generation-error reset/dismiss test.

Add locale keys in all four locale files and update locale parity tests as required. Use short labels equivalent to `Retry` and `Dismiss`; do not embed English strings directly in the component.

### Implementation

- Await `chat.loadMessages(sessionId)` on mount where necessary so view tests and follow-up state loads have deterministic ordering; do not serialize unrelated identity/panel requests unless required.
- Render initial-load failure with a retry action.
- Render refresh failure as a non-destructive inline notice when cached messages exist.
- Render generation failure near the message/composer boundary with a dismiss/reset action.
- Reset stale `streamingError` at the start of both send and regenerate, not only inside a shared loop after work has begun.
- A retry must call existing APIs; it must not reload the browser or navigate away.

### Failure boundary

Do not add automatic retries, backoff, offline queues, toast-only errors, or deletion/reset of persisted messages. A provider error must remain visible until the user dismisses it or starts a new generation attempt.

## Task 3: preserve intentional scroll position across streaming completion

### Current behavior

`MessageList.vue` has a `bottom` ref but no scroll policy. During generation, a synthetic `__streaming__` message grows. On `done`, `streamingText` is cleared and `loadMessages()` replaces the message array with the server result. The DOM height and final node identity change without preserving either bottom-following state or distance from the bottom.

Phase 1 should fix behavior inside the existing `MessageList` scroll container. Moving scroll ownership into a redesigned Chat layout is deferred.

### Scroll contract

Use a small threshold, defined once in `MessageList.vue` (for example 48 px):

```text
distanceFromBottom = scrollHeight - clientHeight - scrollTop
isFollowingBottom = distanceFromBottom <= threshold
```

- Initial successful load scrolls to the bottom.
- User scrolling within the threshold enables following.
- User scrolling above the threshold disables following.
- While following, changes to `streamingText`, the visible message IDs, and transition from streaming to persisted reply schedule a bottom scroll after Vue updates the DOM.
- While not following, those changes preserve the user's scroll position and do not call `scrollTo`.

### Tests first

Add component tests to `shirita-ui/src/components/MessageList.test.ts`. Because jsdom has no layout engine, define `scrollHeight`, `clientHeight`, and `scrollTop` explicitly on the scroll element and spy on `scrollTo` or the selected scrolling primitive.

Required cases:

1. initial populated transcript scrolls to bottom after mount;
2. streaming text growth scrolls when already near bottom;
3. streaming text growth does not scroll after a synthetic upward scroll event;
4. replacing the streaming ghost with a persisted assistant message scrolls when following;
5. the same replacement does not scroll when the user is reading above.

Use a stable selector such as `data-test="message-scroll"`; do not couple tests to Tailwind class strings.

### Implementation

- Put the scroll-element ref on the existing `MessageList` root scroller.
- Track bottom-following as component-local state; it does not belong in Pinia.
- Watch only the minimum render signals needed: visible message identity/count, streaming text, and streaming state.
- Coalesce DOM writes through `nextTick`; avoid scrolling once per token if an update is already scheduled in the same render turn.
- Prefer `behavior: 'auto'` during token streaming and final replacement. Smooth scrolling on every delta causes lag and motion accumulation.
- Remove the unused bottom sentinel if the implementation no longer needs it.

### Manual verification

Test with a reply taller than one viewport:

1. remain at bottom throughout generation and completion;
2. scroll upward during generation and verify the view stays put;
3. return to bottom and verify following resumes;
4. repeat for regenerate and Stop;
5. verify in a mobile viewport and a Tauri/WebKit build if available.

## Task 4: keep the current user turn last in provider-visible conversation

### Current behavior

`assemble_request()` passes a history ending in the current user turn to `build_chat_messages()`. That builder currently emits:

```text
before-history system -> complete history -> after-history system/protocol
```

State and HTML-patch protocols are injected as `AfterHistory`, so an OpenAI-compatible request can end in a system message rather than the latest user interaction. Anthropic extracts all system messages into its top-level `system` field, which masks the ordering issue for that adapter but does not make the core request contract correct.

### Required contract

When included history ends in a user message, treat that last message as the current turn:

```text
before-history system
historical turns excluding current turn
after-history system/protocol
depth inserts in their existing relative historical positions
current user turn
```

After adjacent-role normalization and provider adaptation, the last conversational message must still be the current user turn, including its image attachments.

When there is no trailing user turn, preserve existing behavior. When history is disabled, preserve the existing behavior of omitting history; do not smuggle the current user turn past an explicitly disabled History node.

### Tests first

Update and extend `shirita-core/src/assembly.rs` tests:

1. replace the existing expectation that `AfterHistory` follows a lone current user turn;
2. assert `BeforeHistory -> historical user/assistant -> AfterHistory -> current user` ordering;
3. assert the current user's images survive relocation;
4. assert a history-disabled plan still contains only system segments;
5. retain a depth-insert test proving the current user remains last and depth content remains before it;
6. assert behavior is unchanged when the supplied history ends in `assistant`.

Add or extend conversation tests in `shirita-core/src/conversation.rs`:

- with a declared variable causing state protocol injection, capture the `ChatRequest` and assert its final message is the newest user input;
- cover regenerate, whose context already ends at the target assistant's parent user message;
- keep assertions that protocol content is present so the change cannot pass by accidentally dropping protocols.

Add provider serialization tests:

- `shirita-core/src/model/openai.rs`: the final serialized message is the current user turn when system protocol material exists;
- `shirita-core/src/model/anthropic.rs`: the final non-system message remains the current user turn and the protocol remains in top-level `system`.

### Implementation

- Implement the ordering rule in `build_chat_messages()` rather than adding provider-specific reordering.
- Preserve the complete `ChatMessage`, including `images`, when holding out and appending the current turn.
- Keep summary behavior unchanged in this phase: OpenAI attaches it to the first system message and Anthropic prepends it to the first available user message. The ordering tests must still prove that the final provider-visible conversational role is the current user turn.
- Update comments that currently define AfterHistory as necessarily appearing after the complete history.
- Do not change activation, regex processing, summary selection, budget trimming, or protocol contents.

### Budget-trimming check

`trim_history()` is documented as protecting the last message. Add or retain a focused test showing that the reordered current user turn remains protected when the request exceeds its context window. This is required because the ordering fix and trimming logic share the same invariant.

## Integration order

Implement and verify tasks in this order:

1. hidden ancestor traversal;
2. error recovery;
3. scroll anchoring;
4. current-turn ordering.

The first three touch overlapping Vue components and stores, so each task should leave the frontend tests green before the next begins. Task 4 is primarily Rust and can be reviewed independently, but it should still land after its contract is accepted.

If commits are desired, use one commit per task. Do not combine formatting cleanup, dependency upgrades, authentication changes, ST removal, or UI redesign with these commits.

## Verification commands

Run focused tests while implementing:

```bash
npm --prefix shirita-ui run test -- src/utils/tree.test.ts
npm --prefix shirita-ui run test -- src/stores/chat.test.ts
npm --prefix shirita-ui run test -- src/components/MessageList.test.ts
npm --prefix shirita-ui run test -- src/views/ChatView.test.ts
cargo test -p shirita-core assembly::tests
cargo test -p shirita-core conversation::tests
cargo test -p shirita-core model::openai::tests
cargo test -p shirita-core model::anthropic::tests
```

Before handoff, run the full relevant suites:

```bash
npm --prefix shirita-ui run test
npm --prefix shirita-ui run build
cargo test --workspace
```

If a full suite fails because of unrelated pre-existing work, record the exact command and failure. Do not weaken or delete unrelated tests to obtain a green result.

## Definition of done

- Every required invariant has an automated regression test.
- The four manual user-visible defects are reproducibly fixed.
- No database migration or API shape change was introduced.
- Hidden messages remain reversible and remain excluded from model context.
- Existing regex, token estimation, attachments, branching, regenerate, Stop, summary, and protocol tests remain green.
- Desktop and self-hosted builds use the same corrected behavior without runtime-specific branches.
- Documentation comments and tests describe the new current-turn ordering accurately.
- No work from Phase 2, Phase 3, or Phase 4 is included.
