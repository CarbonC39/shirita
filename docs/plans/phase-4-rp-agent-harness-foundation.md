# Phase 4 implementation plan: RP Agent harness foundation

> Status: proposed implementation plan for review  
> Date: 2026-08-01  
> Scope: establish a minimal, configurable GenerationRun and Tool loop without prescribing how users apply it to role-play

## Goal

Give Shirita its first real RP Agent harness: a model may call registered Tools while producing a reply, receive their results, update the run's visible or internal status, and continue until it explicitly submits the user-visible response through a registered finish Tool.

The harness supplies mechanics, not a prescribed RP workflow. Shirita must not decide that a Tool is for adjudication, editing, inspiration, state, or any other privileged use. Users decide how generation behaves through their existing Prompt composition and through Agent/Tool settings.

After this phase, the following generic flow works in both Tauri and self-hosted Web:

```text
assemble the initial request once
  -> call the model
  -> receive Tool calls
  -> validate and execute registered Tools in order
  -> return Tool results to the model
  -> repeat within configured limits
  -> call shirita.run.finish with the user-visible response
  -> persist that response once
```

Providers/models with native Tool calling use their native transport. RP-oriented models without native support can use a generic XML Tool transport. Both transports normalize into the same runtime calls and results.

## Product boundary

Shirita is a role-play text-generation platform with an Agent harness. It is not a task-automation product, but the runtime must remain neutral about how users employ it.

Phase 4 therefore provides:

- a reliable generation-run lifecycle;
- a trusted Tool registry;
- native and XML Tool transports;
- a bounded Tool/model loop;
- registered `update_status` and `finish` run-control Tools;
- an editable default Agent Prompt defining the model's harness self-understanding and output-channel contract;
- complete global defaults, per-conversation overrides, and provider capability settings;
- transient Tool activity in the chat UI;
- a few safe built-in Tools that prove the framework is not state-specific.

It does not introduce a fixed sequence such as plan, adjudicate, draft, review, or commit. A user may ask the model to call a Tool before writing, after drafting, repeatedly, or not at all.

## Required invariants

The phase is complete only when all of these hold:

- One send or regenerate action creates one `GenerationRun` with a stable ID.
- Send and regenerate share the same run/loop implementation.
- The initial Prompt, activation results, summary, history, attachments, regex prompt transforms, and available Tool set are resolved once at run start.
- Later model rounds append Tool calls/results to the run context; they do not re-roll random Prompt activation or reconstruct a different initial request.
- A run persists at most one ordinary assistant message: the validated `response` argument submitted to `shirita.run.finish`.
- A model round ending without Tool calls is not implicitly complete. The harness appends the configured unfinished-run instruction and continues within the declared limits.
- Ordinary model text, reasoning, Tool protocol, Tool results, and internal status are not persisted or shown as the final reply.
- User-visible transient status is emitted only through `shirita.run.update_status` with explicit user visibility.
- A run with Agent disabled follows the existing single-provider-call behavior.
- Native and XML transports produce the same normalized `ToolCall` and `ToolResult` contracts.
- A capability Tool executes only when it is registered and enabled in the effective settings for that conversation. The two run-control Tools are registered implicitly whenever Agent mode is enabled.
- Tool calls execute sequentially and results return to the model in source order.
- Unknown, disabled, malformed, oversized, timed-out, or failed calls never invoke an unintended handler.
- Every loop/Tool limit is declared in the typed Agent limits or the registered Tool specification. There are no scattered, undocumented magic limits in parsers, providers, handlers, or UI code.
- User-configurable limits have documented server safety ceilings; fixed security bounds are named, centralized, documented, and tested.
- Stop cancels the active provider stream, prevents subsequent calls/rounds, and cooperatively cancels a running Tool where possible.
- Tool-only intermediate rounds never appear as ordinary chat messages or contaminate branch traversal.
- Final token usage is accumulated across all model rounds and remains observable.
- Existing branching, regenerate/swipe, hide, fork, summaries, attachments, token budgeting, state snapshots, Panels, regex, and Phase 1 recovery behavior remain functional.
- The existing `<state_update>` system remains functional but separate during this foundation phase. It is not presented as a registered Tool and is applied only to the terminal response as today.
- Both application modes construct the same built-in registry and enforce the same settings.
- Existing sessions inherit global defaults without a database migration.

## Explicit non-goals

- Migrating variables or `<state_update>` into the Tool runtime.
- Deleting `/state-updates`, variable bricks, local variables, or state-carrier messages.
- Merging Prompt and Agent templates.
- A visual workflow/graph editor or arbitrary completion policies.
- A mutable response workspace or response patching beyond the required final `finish(response)` submission.
- Metadata Tools for identity, avatar, background, CSS, Panel structure, or other UI/runtime metadata.
- External plugins, MCP, WASM, subprocess, HTTP, or network Tools.
- Plugin installation, marketplace, approval UI, or capability grants.
- Persistent Agent traces or a Tool-call audit database.
- Parallel Tool calls, background jobs, multi-Agent collaboration, or irreversible external side effects.
- User-authored executable code or arbitrary expression evaluation.
- A general rewrite of Prompt assembly or provider settings unrelated to Agent capability.

## Runtime concepts

### GenerationRun

Introduce a runtime object equivalent to:

```rust
pub struct GenerationRun {
    pub id: String,
    pub session_id: String,
    pub parent_message_id: Option<String>,
    pub kind: RunKind,
    pub round: u32,
    pub tool_calls: u32,
    pub status: RunStatus,
    pub usage: Usage,
}

pub enum RunKind { Send, Regenerate }
pub enum RunStatus { Running, Completed, Stopped, Failed }
```

This may remain in memory for Phase 4. Do not add a persistence table merely to preserve traces across reloads.

The run owns:

- the immutable initial assembled request;
- the effective Agent settings;
- the resolved Tool specifications;
- the appended provider-neutral Tool conversation;
- cancellation state;
- accumulated usage;
- transient activity events;
- intermediate model text kept in the run context;
- the latest internal/user-visible status;
- the final response supplied to the finish Tool.

Do not expose provider JSON types in this object.

### Tool contracts

Introduce provider-neutral contracts in `shirita-core`:

```rust
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
    pub source: ToolSource,
}

pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub transport: ToolTransport,
}

pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub status: ToolResultStatus,
    pub output: serde_json::Value,
    pub error_code: Option<String>,
}
```

Stable Tool names are non-localized, namespaced identifiers. Display names/descriptions may be localized by the UI.

### Registry and providers

The registry maps stable names to trusted handlers. Design it around a provider interface rather than a hard-coded enum so future sources can register without changing conversation logic:

```rust
pub trait ToolProvider: Send + Sync {
    fn source(&self) -> ToolSource;
    fn specs(&self) -> Vec<ToolSpec>;
    fn handler(&self, name: &str) -> Option<Arc<dyn ToolHandler>>;
}
```

Phase 4 registers only `BuiltinToolProvider`. `Plugin`, `Mcp`, and other sources may exist as serializable/source variants or documented extension points, but no non-built-in loader is implemented.

Registry construction must:

- reject malformed or duplicate names;
- freeze before sharing;
- happen through the same function for Web and Tauri;
- never allow a Definition, Template, Pack, Prompt, request, or model output to replace a handler.

### Run-control Tools

Two built-in control Tools are mandatory whenever Agent mode is enabled. They use the same registry, schema validation, native/XML transports, activity events, and limits as other Tools, but their handlers update `GenerationRun` rather than calling an external capability.

#### `shirita.run.update_status`

Input:

```json
{
  "message": "Checking the candidate for repeated phrasing",
  "visibility": "internal"
}
```

`visibility` is `internal` or `user`:

- `internal` is retained only in the transient run trace/context;
- `user` may appear in the compact activity UI while the run is active;
- neither becomes an ordinary chat message or part of the final assistant response.

The Tool does not impose what a status means or when the model should use it. Its message length limit is a named Tool-spec bound.

#### `shirita.run.finish`

Input:

```json
{
  "response": "The final text intended for the user"
}
```

This is the only Agent-mode completion signal in Phase 4:

- validate a non-empty, bounded string;
- stop scheduling model rounds immediately after successful validation;
- process the response through existing terminal display/state/regex/Panel logic exactly once;
- persist it as the single assistant message and advance the active leaf;
- never return a Tool result to another model round after successful finish;
- reject a second finish call in the same run.
- do not execute any later calls ordered after a successful finish in the same provider round.

The control Tools are always enabled when Agent mode is enabled and are shown as required harness capabilities in settings. Users may choose other Tools freely, but disabling `finish` while keeping Agent mode active would make the runtime contract impossible and is therefore not represented as a valid configuration.

## Basic loop contract

### Round behavior

For each model round:

1. Enforce round and token limits before sending.
2. Stream provider events into a round-local accumulator.
3. Append ordinary text/reasoning to the private run context; do not treat it as the final response.
4. If the round contains Tool calls, validate and execute them sequentially.
5. `update_status` updates transient run status; ordinary capability Tool results are appended to the run context.
6. A valid `finish` call commits its response and ends the run.
7. If the round has no valid finish call, append the configured unfinished-run instruction and begin the next round within limits—even when the model emitted plain text but no Tool call.

The Agent Prompt must explain that ordinary output is private working text and only `finish.response` is returned to the user. The runtime enforces this channel boundary rather than trusting the Prompt alone.

### Completion

A run completes only after a valid `shirita.run.finish` call. Then:

- apply existing terminal-response processing (state tags, display transforms, Panel capture, snapshot folding) to `finish.response` exactly once;
- persist one assistant message;
- advance the active leaf atomically as today;
- emit accumulated usage and `done`;
- discard the in-memory trace after the UI no longer needs it.

Agent mode therefore establishes a minimal response boundary, but not a general mutable response document. Intermediate text cannot yet be patched or selected through a response API; the model supplies the complete response when it calls finish.

### Failure and limits

Use conservative defaults and hard caps:

| Limit | User default | Server hard cap |
| --- | ---: | ---: |
| Model rounds per run | 4 | 8 |
| Tool calls per run | 8 | 32 |
| Tool execution timeout | 5 seconds | 30 seconds |
| Serialized arguments | declared security bound | 64 KiB |
| Serialized result returned to model | declared security bound | 64 KiB |
| Finish response | declared security bound | 1 MiB |

All defaults and ceilings above must live in one typed `AgentLimits` definition and be returned by the settings API for UI help/validation. Argument/result/finish byte ceilings are fixed security settings in Phase 4, but they are named public contract values rather than hidden handler constants. Individual Tool specifications may declare stricter named bounds and must expose them in registry metadata.

When a limit is reached:

- do not execute the over-limit call;
- stop the loop with a stable, visible error;
- do not silently switch transport;
- do not persist intermediate Tool-round text as a successful assistant reply;
- allow the user to dismiss/reset and retry through the existing recovery path.

Unknown/disabled Tool and invalid arguments produce structured Tool errors. Phase 4 should return a Tool error result to the model for one correction opportunity if limits permit. Internal registry corruption or storage failure is fatal.

Repeated identical calls count normally toward the limit. Repetition handling must be an explicit setting (`agent.max_identical_call_rounds`, default 3, bounded by the model-round ceiling), not an unconfigurable special case.

## Tool transports

### Transport setting

When Agent mode is enabled, support:

- `auto`: use native when the active provider/model is configured as supporting it, otherwise XML;
- `native`: require native Tool support and fail before generation when unavailable;
- `xml`: use the generic XML Tool protocol.

Agent-disabled mode is the explicit single-call/off state. Do not also encode `off` as a transport value: an enabled Agent run requires the registered finish Tool, so an off transport would be internally contradictory.

Never change transport after a run begins. A provider error must not trigger an automatic native-to-XML retry because earlier calls may already have executed.

### Provider capability

Store a capability override independently for each provider source:

```text
provider.<source>.native_tools = auto | supported | unsupported
```

`auto` uses a maintained first-party capability default where known. Unknown OpenAI-compatible endpoints must not be assumed to support native Tools merely because their JSON resembles OpenAI; they fall back to XML under `agent.transport=auto` unless the user marks them supported.

### Native transport

Implement both existing provider families:

- OpenAI-compatible `tools`, streamed `tool_calls`, assistant Tool-call messages, and `role=tool` results;
- Anthropic `tools`, streamed `tool_use` blocks, and `tool_result` content blocks.

Provider adapters serialize/parse only. Registry lookup, limits, execution, and loop policy remain in the conversation runtime.

### XML transport

Use one generic format:

```xml
<tool_call id="call_1" name="shirita.random.choose">
{"items":["rain","visitor","blackout"]}
</tool_call>
```

Return results in the next model round using:

```xml
<tool_result id="call_1" name="shirita.random.choose">
{"status":"ok","output":{"selected":"visitor"}}
</tool_result>
```

Requirements:

- parse tags across arbitrary network chunk boundaries;
- never render protocol tags in the candidate UI;
- execute only fully closed calls with complete valid JSON;
- bound buffered protocol size;
- reject duplicate call IDs in one round;
- escape/serialize result content rather than interpolating untrusted raw text;
- inject Tool specifications and XML instructions before the current user turn, preserving the Phase 1 invariant that the current user interaction remains last in the initial provider-visible conversation;
- append subsequent XML Tool result turns without rerunning Prompt assembly.

Do not add Tool-specific XML tags for individual built-ins.

## Built-in Tools

Implement the two required run-control Tools plus a deliberately small, safe capability set:

- `shirita.run.update_status`;
- `shirita.run.finish`;
- `shirita.random.number`;
- `shirita.random.choose`;
- `shirita.math.evaluate`.

The run-control Tools are defined in the loop contract above. The remaining capability Tools are:

### `shirita.random.number`

Inputs: inclusive integer bounds. Output: selected integer.

- reject invalid/reversed ranges and unsafe width;
- use an injectable RNG so tests are deterministic;
- do not expose cryptographic/security claims.

### `shirita.random.choose`

Inputs: a bounded non-empty JSON array of JSON values. Output: selected index and value.

- enforce item-count and serialized-size bounds;
- preserve the selected value's JSON type;
- use the same injectable RNG boundary.

### `shirita.math.evaluate`

Inputs: a bounded arithmetic expression. Output: finite numeric result.

- support an explicitly documented arithmetic grammar only;
- reject variables, assignments, function calls not in the allowlist, non-finite results, excessive nesting, and oversized input;
- use a parser/evaluator library only after reviewing its grammar and dependency impact, or implement a small parser;
- never use JavaScript evaluation, shell, Python, SQL, template evaluation, or dynamic code execution.

These Tools prove random and deterministic capabilities. They do not imply a closed future catalog.

Do not implement `text.inspect` unless a concrete, neutral contract is agreed during implementation review; it is optional and must not delay the three required Tools.

## Settings model

### Effective settings

Resolve settings in this order:

```text
server hard caps
  -> global Agent defaults
  -> optional Session override
  -> immutable effective settings captured by GenerationRun
```

Changing settings during a run affects only later runs.

### Global defaults

Persist validated settings using stable keys equivalent to:

```text
agent.enabled
agent.transport
agent.enabled_tools
agent.max_rounds
agent.max_tool_calls
agent.tool_timeout_ms
agent.show_activity
agent.show_user_status
agent.max_identical_call_rounds
agent.system_prompt
agent.unfinished_prompt
```

Compatibility defaults for existing installations:

- Agent disabled;
- transport `auto` (used only when Agent is enabled);
- all three safe built-ins available for selection but none exposed while Agent is disabled;
- limits set to the table above;
- activity visible when Agent is enabled.

Provide a conservative built-in `agent.system_prompt` that users can edit and reset. It must explain, without prescribing an RP workflow:

- that the model is operating inside Shirita's text-generation harness;
- which registered Tools are available and that their schemas are authoritative;
- that ordinary model text and reasoning are private working output;
- that `update_status` is the explicit transient status channel;
- that only `finish.response` is returned to the user and persisted;
- that the model must call `finish` exactly once when it considers the response ready;
- that Tool results may be used in later rounds;
- the selected transport's call syntax where necessary.

Provide a separate editable `agent.unfinished_prompt`, appended when a round ends without a valid finish call. Its default briefly reminds the model to continue working or submit the completed response through `shirita.run.finish`; it must not inject a writing style or RP method.

The runtime may append a non-editable generated protocol appendix containing exact Tool schemas, call IDs, escaping rules, and security/channel facts. Clearly distinguish this machine contract from the editable behavioral Prompt. Do not hide additional behavioral instructions in provider adapters.

Settings values require server-side validation. The generic settings endpoint must not be the only validation boundary for typed Agent configuration.

### Per-provider capability

Add `native_tools` to each provider's isolated settings namespace and UI section:

- Auto;
- Supported;
- Unsupported.

Switching provider source must preserve each source's value just like API key/base URL/model isolation does today.

### Per-conversation override

Store an optional typed block at `session.override_config.agent`:

```json
{
  "mode": "custom",
  "enabled": true,
  "transport": "xml",
  "enabled_tools": ["shirita.random.choose"],
  "max_rounds": 3,
  "max_tool_calls": 6,
  "tool_timeout_ms": 5000,
  "show_activity": true,
  "show_user_status": true,
  "max_identical_call_rounds": 3,
  "system_prompt": "...optional conversation override...",
  "unfinished_prompt": "...optional conversation override..."
}
```

Absence of the block means dynamic inheritance from global defaults. Resetting to global deletes the block rather than copying current defaults.

Add a narrow atomic storage operation and dedicated API. The frontend must never read-modify-write the complete `override_config` object.

The API should return:

- global defaults;
- optional stored override;
- effective validated settings;
- registered Tool summaries needed by the selector.

Do not let Template or Pack set Agent configuration in Phase 4.

### Settings UI

Add a complete Agent section to Settings and a compact conversation-specific editor reachable from the chat details surface or the existing per-chat customization area.

The global UI must support:

- enable/disable;
- transport selection;
- registered capability Tool checklist with stable ID, localized label/description, source, and permission summary;
- rounds/calls/timeout controls with ranges and defaults;
- activity visibility;
- user-status visibility;
- editable default Agent and unfinished-run Prompts with preview, reset, and a warning that invalid instructions can prevent completion;
- reset to defaults;
- per-provider native capability override.

The Session UI must support:

- `Use global defaults` versus `Customize this conversation`;
- the same effective behavior controls when custom;
- a clear effective-value preview while inheriting;
- reset/delete override;
- save errors without losing the last good configuration.

Prompt overrides follow the same inheritance rule as the other Session Agent settings. An inheriting Session dynamically uses the current global Prompts; a custom Session may override them and resets by deleting its Agent block.

Show `update_status` and `finish` in the Tool summary as required harness controls, not as checkboxes. `enabled_tools` contains only optional capability Tools.

All labels, descriptions, validation errors, activity text, and transport names require parity across English, Simplified Chinese, Traditional Chinese, and Japanese.

## Structured model and SSE events

### Provider-neutral stream

Replace `ModelProvider::stream_chat() -> String deltas` with the minimum structured event contract needed by the loop:

```rust
pub enum ModelEvent {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallStart { id: String, name: String },
    ToolArgumentsDelta { id: String, chunk: String },
    ToolCallEnd { id: String },
    Usage(Usage),
    Finished(FinishReason),
}
```

Exact variants may change during implementation, but fragmented arguments must not be represented as ordinary text.

Update Echo/test providers to script structured events and cover multi-round runs deterministically.

### Browser SSE

Preserve current `delta`, reasoning rendering, `done`, and `error` behavior for ordinary chat. Add transient events equivalent to:

```text
run_start
round_start
tool_start
tool_result
status
finish
usage
```

Do not expose raw provider payloads. Tool argument/result previews must be bounded and suitable for display.

### Chat UI activity

When `show_activity` is enabled, show a compact, non-permanent activity area near the streaming response:

- current round;
- Tool name;
- running/succeeded/rejected/failed state;
- the latest `update_status` message only when its visibility is `user` and `show_user_status` is enabled;
- elapsed time when known.

It must not permanently reduce the Composer area, must work on mobile, and must disappear or collapse after completion. Phase 4 does not promise trace recovery after reload.

## Task sequence

### Task 1: lock settings and run contracts

Tests first:

- deserialize/default/validate global and Session settings;
- enforce every declared setting/ceiling and reject unknown transport values/Tool IDs;
- resolve inheritance correctly;
- inherit, override, and reset the editable Agent/unfinished Prompts correctly;
- provider namespaces preserve independent native capability values;
- deleting a Session override restores dynamic inheritance;
- existing sessions with no Agent block behave as Agent disabled under compatibility defaults.

Implement typed configuration in core before adding UI controls. Do not let conversation code interpret raw settings JSON.

### Task 2: build the registry and safe built-ins

Tests first:

- malformed/duplicate/unknown/disabled names;
- schema validation and bounded input/output;
- timeout/cancellation behavior;
- `update_status` internal/user visibility and bounded messages;
- `finish` validation, single-call semantics, and immediate run termination;
- deterministic injected RNG tests;
- math grammar and adversarial rejection cases;
- one registry construction path shared by Web/Tauri.

No provider code belongs in Tool handlers.

### Task 3: introduce structured provider events and messages

Tests first:

- OpenAI fragmented multi-call arguments;
- Anthropic fragmented `tool_use` blocks;
- ordered multiple calls;
- native Tool-result round serialization;
- normal text/reasoning behavior unchanged;
- usage and finish reasons preserved;
- malformed/upstream error frames fail clearly.

Refactor provider adapters before conversation loop implementation.

### Task 4: implement XML transport

Tests first:

- tags split at every relevant chunk boundary;
- multiple calls and Unicode JSON;
- malformed/unterminated/oversized tags;
- duplicate IDs;
- no XML protocol leakage into displayed deltas;
- specification/result serialization cannot break the enclosing protocol;
- current user turn remains last in the initial request.

XML is a transport adapter, not a second executor.

### Task 5: replace single-call generation with GenerationRun

Tests first at core/conversation level:

- plain text without `finish` appends the configured unfinished instruction and continues;
- one and several capability Tool rounds followed by a valid `finish` call;
- sequential multiple calls in one round;
- only `finish.response` is processed and persisted;
- successful finish schedules no later round and returns no Tool result to the model;
- duplicate, empty, and oversized finish calls are rejected;
- internal status remains private and user status follows visibility settings;
- Prompt activation is resolved once;
- aggregate usage and per-round budgeting;
- every named round/call/timeout/repetition/size limit and its advertised setting/ceiling;
- Tool correction after a structured rejection;
- Stop during provider streaming, Tool execution, and between rounds;
- send/regenerate share behavior and preserve parent-state semantics;
- provider/storage failure never advances the active leaf incorrectly;
- existing terminal `<state_update>` and regex/Panel processing still occur exactly once.

Extract shared send/regenerate finalization while preserving the existing generation registry's single-live-run guarantee.

### Task 6: expose APIs and transient SSE activity

Add and test:

- registry summary endpoint for UI selectors;
- validated global Agent settings endpoint or typed validation layered over settings;
- GET/PUT/DELETE Session Agent override endpoint;
- SSE run/round/Tool events;
- redacted and bounded Tool previews;
- auth behavior identical to other protected routes.

Do not create a generic HTTP Tool invocation endpoint in Phase 4. Tools are invoked by the generation harness only.

### Task 7: complete global and Session UI

Tests first:

- global controls load/save/reset and enforce ranges;
- editable Agent/unfinished Prompts save, inherit, preview, warn, and reset;
- provider capability remains isolated per source;
- registry Tool list and enabled selection;
- Session inherit/custom/save/reset behavior;
- effective preview updates after global changes;
- current chat captures effective settings at send/regenerate start;
- transient activity/status handles success/error/Stop and does not become a permanent chat row;
- all locale catalogs remain in parity.

Do not add an Advanced mode or separate Agent workspace.

### Task 8: document, clean up, and verify

Update:

- README current capabilities and limitations;
- Architecture GenerationRun, Tool registry, transport, settings inheritance, provider event, and SSE sections;
- current direction and docs index;
- provider documentation with native capability/transport behavior;
- this plan's status and actual verification record.

Remove duplicate single-round paths, provider string-delta assumptions, and unreachable test helpers. Do not remove legacy state behavior.

## Implementation order

Use reviewable commits that keep tests/build green:

1. typed Agent settings, inheritance, validation, and Session atomic storage;
2. Tool contracts, registry, and safe built-ins;
3. provider-neutral structured events/messages;
4. OpenAI and Anthropic native Tool transport;
5. generic XML transport;
6. GenerationRun and bounded loop shared by send/regenerate;
7. APIs and transient SSE activity;
8. global/provider/Session settings UI and locales;
9. documentation, cleanup, and final verification.

Do not combine the provider refactor, loop, and settings UI into one commit.

## Automated verification

Run focused tests after each task, then:

```bash
cargo test --workspace
npm --prefix shirita-ui test
npm --prefix shirita-ui run build
git diff --check
```

Required test categories include:

- core Tool registry and handlers;
- settings validation/inheritance;
- OpenAI and Anthropic serialization/parsing;
- XML streaming parser fuzz-like chunk boundaries;
- conversation multi-round send/regenerate/Stop/error behavior;
- Web API/SSE integration;
- UI global/provider/Session settings and activity;
- locale parity;
- all Phase 1–3 regression suites.

## Manual acceptance scenarios

1. With Agent off, run ordinary send/regenerate and confirm behavior matches Phase 3.
2. With OpenAI native capability enabled, let Prompt instructions cause `random.choose`, return the result, update status, and finish with the intended RP reply.
3. Force XML with a model lacking native Tools and complete the same loop through `finish` without working text or protocol appearing in chat.
4. Use `math.evaluate` during a multi-round response, inspect transient status/activity, and confirm only `finish.response` is persisted.
5. Change global defaults and confirm inheriting conversations update while customized conversations do not.
6. Switch provider sources and confirm native capability settings remain isolated.
7. Let a model output a plausible reply without calling `finish`; confirm the editable unfinished Prompt continues the run rather than ending early.
8. Hit every declared round, call, timeout, malformed-argument, unknown/disabled Tool, size, and repetition limit; recover without navigation.
9. Stop during a provider round and a Tool call; confirm no later round starts.
10. Verify regenerate, swipe, fork, hidden messages, attachments, summaries, Panels, token totals, and terminal state updates.
11. Repeat native/XML settings and a complete loop in self-hosted Web and Tauri/WebKit.

Record environments actually checked. Unperformed manual checks must remain explicitly marked pending.

## Non-binding outlook

The following ideas shape extension boundaries but are not Phase 4 commitments or approved designs.

### Prompt and Agent template convergence

Prompt composition may eventually also describe available Tools, Tool-use guidance, loop/completion policy, and per-session overrides. This could let each conversation combine context and Agent behavior in one composable template. Phase 4 keeps Agent settings separate and does not redesign Prompt nodes.

### Mutable response workspace

Phase 4 establishes only a final response submission through `shirita.run.finish`. A future run may maintain a response document that models and Tools can inspect, patch, replace, validate, or polish before finish. This could support rewriting, numerical substitution, and other user-defined harnesses; it is not implemented now.

### Metadata Tools

Future registered Tools may read or update controlled runtime metadata, such as the currently speaking character's name/avatar, scene background, or CSS/style variables. Such Tools require typed schemas, scopes, permissions, validation, branch/rollback semantics, and clear ownership; Phase 4 does not implement a generic JSON metadata writer.

### External Tool providers

Tool implementations may eventually come from installed plugins, MCP, WASM, or other user-authorized providers. Content may declare requirements, but must not silently install executable code or grant permissions. Phase 4 keeps the registry provider-neutral without choosing or implementing an external plugin format.

These outlook items must not be used to expand Phase 4 implementation scope without a new reviewed plan.

## Completion and handoff

Before marking Phase 4 implemented:

- update this status and current documentation;
- record automated and manual verification honestly;
- confirm native and XML end-to-end acceptance scenarios;
- confirm missing finish calls continue through the editable unfinished Prompt and never commit implicit output;
- confirm editable Agent Prompts accurately describe the enforced visibility channels;
- confirm Agent-off compatibility and terminal legacy state behavior;
- confirm global inheritance, Session override, and provider capability settings all work in UI and runtime;
- confirm no Template/Pack Agent policy, mutable response workspace, metadata Tool, or external plugin loader leaked into scope;
- list known provider/model capability limitations and XML parser constraints.

## Review checklist

- [x] GenerationRun is the shared unit for send and regenerate.
- [x] Initial Prompt activation is resolved once per run.
- [x] Only a valid `shirita.run.finish.response` becomes an assistant message.
- [x] Plain output without finish continues through the configured unfinished Prompt.
- [x] `update_status` and finish are registered control Tools with enforced visibility semantics.
- [x] Registry is immutable, rejects duplicate/unknown names, and is provider-neutral.
- [x] Two run-control and three safe capability Tools are available and bounded.
- [x] OpenAI and Anthropic native Tool calling work end to end.
- [x] Generic XML Tool transport works across stream chunk boundaries without UI leakage.
- [x] Native and XML calls use the same executor.
- [x] Every loop, Tool, size, timeout, repetition, token, and cancellation limit is declared centrally, documented, exposed where configurable, and enforced.
- [x] Global defaults, Session override, provider capability, Agent Prompt, and unfinished Prompt settings are typed, persisted, validated, and fully editable.
- [x] Existing sessions inherit global defaults dynamically.
- [x] Activity is observable but transient and non-intrusive.
- [x] Agent-off chat and all Phase 1–3 behavior remain intact.
- [x] Legacy state behavior remains separate and functional.
- [x] No future outlook item expanded the implementation scope.
- [x] Core/Web/UI/build/locale/diff suites pass.
- [ ] Actual manual verification environments are recorded.

Implementation note (2026-08-02): automated Core and Web workspace tests pass, as do all 482 UI tests and the production UI build. Live-provider and packaged-desktop manual verification has not been performed, so the final manual item remains open.
