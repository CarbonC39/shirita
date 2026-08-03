# Phase 5 implementation plan: response-centered RP Agent runtime and MCP Tools

> Status: Phase 5A and 5B implemented
> Date: 2026-08-02
> Scope: complete the Phase 4 Agent foundation around a mutable response workspace, then connect explicitly configured MCP Tools to the same runtime

## Goal

Make Shirita's Agent runtime dependable and useful for role-play text generation.

The center of a run is not an external task. It is a user-visible response being constructed inside the run. The model may create a draft, call built-in or MCP Tools, inspect or revise the draft, and explicitly finish it. Only the finished response becomes an assistant message.

```text
assemble immutable context and freeze available Tools
  -> create an empty response workspace
  -> call the model
  -> execute authorized Tools and return their results
  -> replace or patch the response workspace
  -> repeat within declared limits
  -> call shirita.run.finish
  -> process and persist the workspace once
```

Phase 5 has a mandatory internal gate:

```text
Phase 5A: finish the existing RP Agent runtime
  - response workspace and response Tools
  - native/XML multi-round parity
  - cancellation, errors, usage, activity, and status UI
  - real-provider verification

Phase 5B: add MCP Tool providers
  - stdio and Streamable HTTP
  - discovery, registration, policy, execution, and authorization
  - native/XML and desktop/self-hosted verification
```

Do not begin Phase 5B before the Phase 5A acceptance gate passes.

## Product boundary: RP generation, not a coding Agent

Shirita is not a coding Agent, task automation product, or general autonomous operator. Coding-Agent conventions are not the standard against which this runtime is designed.

Phase 5 must not introduce a privileged workflow such as inspect, plan, execute, verify, and report. It must not add task lists, completion percentages, filesystem assumptions, or a built-in interpretation that a Tool exists for planning, adjudication, editing, inspiration, state, or any other particular RP technique.

Shirita owns only neutral mechanics:

- which capabilities the user made available;
- how calls and results are transported and validated;
- whether a call is authorized;
- how a run is bounded, cancelled, observed, and completed;
- which text is private working output and which response is returned.

Users decide through Prompt composition, settings, and connected capabilities how those mechanics participate in role-play. A Tool may help with prose, randomness, simulation, calculation, retrieval, transformation, or a use the application did not anticipate.

### The response is the run's working object

Traditional task Agents often maintain external task state and finally report an outcome. Shirita instead maintains the RP response that will be returned to the user:

```text
GenerationRun
  immutable initial context
  frozen Tool registry and policies
  private model/Tool conversation
  transient activity and status
  accumulated usage
  response workspace
    text
    revision
```

This does not prescribe a writing workflow. A model may write once, revise repeatedly, call Tools before writing, or finish directly.

## Relationship to other phases

Phases 1–3 repaired chat correctness, removed SillyTavern compatibility, and rebuilt the default UI. Phase 4 introduced `GenerationRun`, the Tool registry, native/XML transports, a bounded loop, settings, `shirita.run.update_status`, and `shirita.run.finish`.

Phase 5 completes that foundation and connects external MCP Tools. Prompt composition usability and observability are Phase 6 and the intended end of this cleanup cycle. Phase 5 must not redesign Prompt trees or merge Prompt and Agent templates.

State Tool migration is separate. Existing `<state_update>`, state snapshots, and manual variable editing remain functional. The terminal state/regex/Panel pipeline applies only to the response committed by `finish`.

## Required invariants

- Agent-disabled send and regenerate retain the existing single-call behavior.
- Agent-enabled send and regenerate use the same `GenerationRun` implementation.
- Every Agent run owns one response workspace with a monotonically increasing revision.
- Every provider round sees exactly one complete canonical snapshot of the workspace as it exists at the start of that round.
- Older workspace snapshots are replaced rather than appended, and completed response-control calls are compacted without losing the current response state.
- Ordinary model text and reasoning remain private and never modify the workspace implicitly.
- Only registered response Tools may change the workspace.
- A successful `finish` persists the workspace as at most one assistant message.
- Failed, stopped, rejected, disconnected, or unfinished runs persist no assistant response or workspace draft.
- Existing terminal regex, state, Panel, snapshot, and display processing runs exactly once after successful finish.
- Native and XML transports produce semantically equivalent multi-round Tool conversations.
- XML works after a capability Tool result with a small RP-oriented model, not only for direct `finish`.
- Tool failures use one provider-neutral result contract and may be returned for bounded correction.
- One model round may emit multiple Tool calls; they execute sequentially in source order and their results become model-visible only in a later provider round.
- The first `finish` call in a round is a terminal fence. Calls and ordinary output after that fence have no effect.
- A recoverable failure before the finish fence prevents that round from committing the workspace.
- Stop is observed during provider streaming, authorization waits, Tool execution, between calls, and between rounds.
- Activity is transient, structured, keyed by `run_id`, and never stored as chat history.
- `update_status` is a transient model-controlled text channel, not a task tracker or factual progress claim.
- Disabling activity hides the whole activity surface, including user-visible status.
- A run freezes its Tools, policies, and initial Prompt context before the first round.
- Built-in and MCP Tools use the same immutable registry, executor, limits, and result types.
- Conversation code never dispatches on whether a Tool is built-in or MCP.
- MCP credentials never enter model context, activity, logs, exports, or ordinary API responses.
- MCP Tools execute only under explicit effective user policy.
- Desktop and self-hosted Web share the same runtime and MCP implementation.
- Every configurable limit has a typed default and documented server ceiling; fixed security limits are named, centralized, and tested.
- No parser, provider, MCP adapter, route, or UI component introduces an undisclosed retry, timeout, byte, round, Tool, or revision limit.
- Branching, regenerate, swipe, hide, fork, summaries, attachments, regex, Panels, state snapshots, token budgeting, and recovery remain functional.

## Explicit non-goals

- Prompt composition or Prompt tree redesign.
- Merging Prompt and Agent templates.
- State Tools or removal of `<state_update>`.
- Persisting drafts, response revision history, Agent traces, or Tool audit history.
- Collaborative editing or multiple response documents in one run.
- Metadata Tools for identity, avatar, background, CSS, or Panel structure.
- MCP Resources, Prompts, Sampling, Elicitation, Tasks, Apps, or experimental extensions.
- An MCP marketplace, installer, package manager, or complete Shirita plugin format.
- Automatic OAuth discovery, browser login, dynamic client registration, or token refresh.
- Allowing Templates, Packs, imports, or model output to install/configure MCP servers.
- Coding-Agent task workflows, filesystem policies, shell policies, or completion scoring.
- Parallel Tool calls, background runs, multi-Agent collaboration, or cross-run jobs.
- Provider expansion unrelated to Tool transport correctness.
- Draft MCP protocol revisions without a separate compatibility decision.

## Phase 5A: response-centered Agent runtime

### Response workspace

Extend the in-memory run with:

```rust
pub struct ResponseWorkspace {
    pub text: String,
    pub revision: u64,
}
```

It starts empty, is bounded by a named response-workspace ceiling, increments revision after each successful mutation, is discarded on failure/Stop, and is committed only through `finish`. Response handlers are required run-control capabilities and cannot be replaced by another provider.

### Response Tools

Register these controls whenever Agent mode is enabled.

#### `shirita.response.replace`

```json
{"text":"New complete draft"}
```

Validate before mutation, increment revision on success, and return the new revision plus a bounded summary. Do not echo the complete draft in the Tool result; the next provider round receives it through the canonical workspace snapshot.

#### `shirita.response.patch`

```json
{
  "revision":2,
  "operations":[
    {
      "search":"She wasn't afraid, but cautious.",
      "replace":"She remained cautious."
    },
    {
      "search":"He doesn't answer.",
      "replace":"He looks away without answering."
    }
  ]
}
```

Rules:

- revision must match;
- operations are applied in source order to an isolated candidate string;
- every search is non-empty and, at its step, matches exactly once in that candidate;
- an empty operation list, no match, multiple matches, stale revision, excessive operation count, or oversized output fails the entire call without mutation;
- all operations succeed or none take effect;
- a successful batch increments revision exactly once, regardless of operation count;
- matching uses documented literal Unicode-string semantics;
- Phase 5 implements one patch language, not offset, diff, regex, and JSON-patch variants.

Stable errors include `stale_revision`, `empty_patch`, `too_many_operations`, `search_not_found`, `search_not_unique`, and `response_too_large`. The result reports the new revision and applied operation count, not the complete response. Existing regex transforms remain separate and do not run during workspace mutation.

“At its step” is deliberately incremental: operation 2 sees the candidate produced by operation 1, not the original workspace. For example:

```text
initial: "red door; red cloak"
op 1: search "red door" -> replace "blue door"
candidate: "blue door; red cloak"
op 2: search "red" -> replace "black"
final: "blue door; black cloak"
```

Operation 2 succeeds because its search is unique after operation 1. Conversely, if operation 1 creates a second occurrence of operation 2's search, operation 2 returns `search_not_unique` and the entire batch rolls back to the original workspace.

### Run-owned mutation controls

Keep Tool handlers stateless and keep the mutable workspace inside `GenerationRun`. Extend the existing Phase 4 control-result pattern rather than teaching the loop to dispatch on Tool names. The contract should be equivalent to:

```rust
pub enum ToolControl {
    None,
    Status { message: String, user_visible: bool },
    ReplaceResponse { text: String },
    PatchResponse { revision: u64, operations: Vec<ResponsePatchOperation> },
    Finish { response: Option<String> },
}
```

The registered handler validates the Tool schema and returns a typed control intent. The Agent loop applies that intent to its own workspace, performs revision/search/size validation requiring current run state, creates the normalized Tool result/receipt, and emits structured activity. Do not let a handler retain an `Arc<Mutex<ResponseWorkspace>>`, mutate persistence directly, or bypass the registry. Conversation logic branches on typed `ToolControl`, never on `shirita.response.*` string names.

#### `shirita.run.finish`

Finish supports both:

```json
{}
```

and:

```json
{"response":"Complete final response"}
```

- With `response`, validate and atomically replace the workspace before commit.
- Without it, commit the current workspace.
- Reject an empty workspace.
- Stop scheduling calls/rounds after success.
- Do not execute calls ordered after finish in the same round.
- Process and persist the final workspace exactly once.

A successful finish produces an internal bounded result equivalent to:

```json
{"committed":true,"revision":3,"bytes":128}
```

Do not include the response text in that result. It is used for the structured Tool/activity boundary and tests; after successful finish it is not sent into another model round. A skipped or invalid finish returns the ordinary structured failure result and does not claim `committed:true`.

One-shot `finish(response)` is a legitimate simple writing path, not a temporary compatibility hack. Prompts describe both paths without forcing needless multi-step editing.

Update the editable default Agent Prompt and generated XML transport instruction to state at least:

- ordinary output is private and does not change the response;
- the current canonical response and revision are supplied automatically every round;
- use `response.replace` to create/replace it and atomic `response.patch` to revise it;
- Tool results become visible only in the next round;
- use `finish()` to commit the current workspace, or `finish(response)` for a one-shot response;
- calls after finish are ignored;
- internal and user-visible status remain separate from the response.

These are channel/mechanics instructions, not a required drafting workflow. Both prompts remain user-editable within their existing declared size bounds.

### Ordered multi-Tool rounds and the finish fence

A provider round may emit more than one normalized Tool call. Preserve source order and execute calls sequentially; Phase 5 does not run calls in parallel.

The model chooses all calls in a round before it can observe any of their results. Results from calls in round N become model-visible only in round N+1. The Agent Prompt and XML transport instructions must state this plainly without prescribing what the Tools are for.

Consequences:

- independent calls may share a round;
- `response.replace` followed by `finish` may create and commit a known draft in one round;
- a call whose arguments or response depend on another Tool result must wait for a later round;
- the runtime does not infer dependencies or reorder calls on the model's behalf.

The first normalized call named `shirita.run.finish` is a terminal fence for that round, whether its arguments ultimately validate or not:

- execute calls before the fence in source order;
- never execute calls after the fence;
- ignore later duplicate finish calls;
- count every model-submitted call, including calls after the fence, against the declared Tool-call limit even when it is not executed;
- ordinary model text remains private regardless of whether it appears before or after the fence.

If every executed call before the fence succeeds and finish validates, commit the resulting workspace. If any preceding call has a recoverable failure, do not execute/commit finish: return the completed results plus a structured indication that finish was skipped, then continue within normal round/call limits. This prevents a failed patch followed by finish from silently committing the old response.

A fatal failure terminates the run. An invalid finish is itself a recoverable Tool failure when limits permit, but its fence still prevents later calls from running. There is no hidden retry or special completion budget.

### Private model text boundary

Do not append ordinary model output to the workspace automatically:

```text
ordinary output  -> private run conversation
response Tools   -> explicit workspace mutation
finish           -> explicit user-visible commit
```

Agent-disabled generation continues through the ordinary streaming path.

### Canonical workspace synchronization

The workspace snapshot is the model's only read channel in Phase 5. Do not register `shirita.response.read`: because Tool results are not observable until the next provider round, such a call would only repeat the snapshot that the next round receives automatically.

Before every provider round, construct its request from the provider-neutral run context with exactly one current workspace snapshot containing:

```text
response workspace
revision: 3
text: <the complete current response, or an explicit empty marker>
```

The representation may be adapted to provider message formats, but its meaning and placement are transport-independent. It must be clearly separated from the user's message, private reasoning, Tool protocol, and final assistant history.

Use this provider-neutral serialized payload, with JSON string escaping plus the existing protocol-breakout escaping for `<`, `>`, and `&`:

```text
SHIRITA_RESPONSE_WORKSPACE
{"revision":0,"state":"empty","text":""}
END_SHIRITA_RESPONSE_WORKSPACE
```

For a non-empty workspace:

```text
SHIRITA_RESPONSE_WORKSPACE
{"revision":2,"state":"present","text":"She remained by the door."}
END_SHIRITA_RESPONSE_WORKSPACE
```

The snapshot is an application-controlled/system message immediately before the message that drives the next model response:

- round 1: immediately before the current user message, so the Phase 1 current-user-last invariant remains true;
- round 2 and later: after all retained previous-round Tool results and immediately before the configured unfinished instruction, which is the final message for that continuation round.

There is no unfinished instruction in round 1. Do not combine the snapshot with the user message, Tool result, Agent Prompt, or unfinished instruction. Provider adapters may map system content into a provider-level system field, but must preserve this logical ordering and must not merge the snapshot into user-authored content.

The logical round-1 transcript is fixed as:

```text
native:
  assembled system/summary/history messages
  system(editable Agent Prompt)
  system(canonical workspace revision 0, empty)
  user(current interaction)

XML:
  assembled system/summary/history messages
  system(editable Agent Prompt + generated XML Tool protocol)
  system(canonical workspace revision 0, empty)
  user(current interaction)
```

After a capability-only first round, the logical continuation is:

```text
native round 2 tail:
  assistant(private working text, tool_calls=[capability call])
  tool_result(capability result)
  system(canonical workspace at its current revision)
  system(editable unfinished instruction)

XML round 2 tail:
  assistant(private working text and retained capability XML call)
  user(normalized XML capability result)
  system(canonical workspace at its current revision)
  system(editable unfinished instruction)
```

The provider adapter may encode native Tool results using its required role rather than the provider-neutral labels above. It may not move the snapshot after the unfinished instruction or duplicate either message.

When the workspace changes, replace the previous snapshot in the next request; never append another full historical version. The request therefore contains one complete authoritative response, not a chain of drafts the model must reconstruct.

Compact completed response-control calls before constructing the very next provider request. Remove the response-control call and its Tool result as a pair, then add bounded mutation receipts such as:

```text
response replaced -> revision 1
response patched (2 operations) -> revision 2
```

Compaction removes obsolete full replace arguments, patch bodies, and duplicate response text. The canonical snapshot plus receipt is the complete model-visible result of a successful response mutation; native providers do not additionally receive the original response-control `assistant.tool_calls`/Tool-result pair.

If one native assistant turn contains both response-control and ordinary capability calls, rebuild its retained provider-neutral history with only the non-response calls and their matching results. Every retained native Tool call must still have exactly one matching result, and every retained result must still have a call. Receipts are appended only after all retained results for that round; do not insert a system receipt inside a provider-required call/result sequence.

XML follows the same normalized history: remove response-control XML blocks/results, retain semantically necessary capability/MCP call results, then append the same receipts. Do not make XML reconstruct response history from old protocol blocks.

Examples follow. `C` is a capability Tool and `R` is `shirita.response.replace`.

```text
round 1 native model output: assistant(tool_calls=[C, R])
round 1 execution:           C -> result c1; R -> revision 1

round 2 normalized tail:
  assistant(tool_calls=[C])
  tool_result(c1)
  system("response replaced -> revision 1")
  system(canonical workspace revision 1)
  system(unfinished instruction)
```

```text
round 1 XML model output: <tool_call C>...</tool_call><tool_call R>...</tool_call>
round 1 execution:        C -> result c1; R -> revision 1

round 2 normalized tail:
  retained capability call/result c1 in the XML conversation representation
  system("response replaced -> revision 1")
  system(canonical workspace revision 1)
  system(unfinished instruction)
```

Capability and MCP Tool results are not discarded merely because response-control history is compacted; they may still be semantically necessary for later writing.

This design bounds context growth but does not pretend that repeated provider rounds are free: the current full response is input to each round and must remain visible in accumulated usage. Phase 6 may expose that cost more clearly. Phase 5 does not add full/delta/on-demand synchronization modes or depend on provider prompt caching.

### Streaming boundaries

Phase 5 distinguishes four independent forms of streaming:

1. **Provider stream.** Model text, reasoning, native Tool fragments, and usage may arrive incrementally. Stop must remain effective while consuming them.
2. **Tool-call assembly.** Native arguments and XML blocks may span chunks, but a Tool executes only after its complete call has been assembled, bounded, parsed, and validated.
3. **Response visibility.** Workspace drafts and patches are not streamed into the ordinary assistant message. The UI receives only bounded activity summaries until finish. After finish, the complete committed response may be transported in one or more SSE `delta` events, but those chunks represent an already finalized response rather than a visible draft.
4. **MCP transport.** Streamable HTTP may deliver protocol/progress messages incrementally, but Shirita gives the model a normalized Tool result only after the call reaches a complete result or failure. Progress may become bounded activity; it never mutates the workspace implicitly.

Once a complete finish fence can be recognized, the runtime may cancel or stop consuming unnecessary trailing provider output when the provider adapter can do so safely. Semantic correctness must not depend on that optimization: any trailing text/calls are ignored even if the underlying stream had to be consumed to completion. Provider usage is reported as actually supplied, without inventing tokens for ignored output.

### XML/native parity

Equivalent transcripts must convey call ID/name/arguments, exact result association/status, workspace revision where relevant, and that the run remains open until finish.

Remove empty or misleading XML transcript messages created by Tool-only rounds. Define result and unfinished-instruction ordering so small RP models can continue after a capability result.

Truncated XML must distinguish an incomplete plausible call, malformed complete protocol, private text without a call, and invalid arguments. Never execute a partial call or leak it. Continuation consumes ordinary declared rounds; do not add hidden retries or model-name branches.

### Unified failures and lifecycle

Normalize recoverable failures across transports/providers:

```text
unknown_tool, disabled_tool, authorization_denied, invalid_arguments,
arguments_too_large, timeout, cancelled, result_too_large,
provider_unavailable, protocol_error, execution_failed
```

Workspace errors extend this vocabulary. Recoverable results return to the model within ordinary configured call/round limits; there is no hidden correction counter.

Check Stop before/during provider calls, before each Tool, during authorization and Tool execution, after results, before later same-round calls, and before another round. Stop emits `stopped`, discards the workspace, and starts no more work.

Document and test explicit Stop versus a dropped SSE consumer. The policy must match desktop and self-hosted modes, and frontend abort behavior must not make persistence ambiguous.

### Usage, activity, and `update_status`

Accumulate usage across rounds and distinguish provider-reported, labeled estimate, and unavailable states. Missing usage is not exact zero.

Replace the frontend's activity/status strings with a run-scoped transient model:

```ts
interface AgentRunView {
  runId: string
  phase: 'running' | 'awaiting_authorization' | 'finished' | 'stopped' | 'failed'
  round: number
  responseRevision: number
  events: AgentActivityEvent[]
  usage: GenerationUsage
}
```

Use structured SSE for run/round start, Tool start/result, workspace mutation, status, authorization, usage, finish, Stop, and failure. Do not reconstruct state from localized strings.

UI rules:

- `show_activity=false` hides the whole surface;
- `show_user_status=false` hides user status while Tool activity may remain;
- a new status may replace the prominent line but not erase Tool history;
- activity is compact and may collapse older events;
- arguments, drafts, results, and credentials are hidden by default;
- workspace events show neutral summaries and revision only;
- success clears deterministically; Stop/failure remains understandable until recovery;
- stale events from an old run cannot overwrite the active run;
- nothing is persisted or restored after reload.

Any event-list memory bound must be named, exposed, and tested.

### Phase 5A acceptance gate

Before MCP implementation:

- scripted native/XML tests prove equivalent capability-result-edit-finish flows;
- ordered multi-call tests prove next-round result visibility, finish fencing, duplicate finish handling, call counting, and pre-finish failure blocking;
- replace/atomic-batch-patch/revision/size/Stop/failure tests pass;
- every round contains one current canonical snapshot, no historical full snapshot, and no obsolete full response-control payload;
- native/XML transcript fixtures lock snapshot serialization, round-1/current-user placement, continuation placement, and mixed response/capability compaction;
- response handlers remain stateless and all workspace mutation occurs through typed run-owned `ToolControl` application;
- finish success/failure payloads and the updated editable Agent/XML instructions match the declared contracts;
- only the finished workspace persists and terminal processing occurs once;
- Stop covers provider, Tool, inter-call, and inter-round boundaries;
- activity/status tests prove run isolation and setting gates;
- `gemma-4-e4b` at `localhost:8080` completes short native and XML capability-result-response-finish flows;
- one-shot native/XML `finish(response)` still works;
- Agent-off and all Phase 1–4 regressions pass;
- actual environments and unperformed checks are recorded honestly.

## Phase 5B: MCP Tools

### Protocol scope

Target stable MCP `2025-11-25`, Tools only, over:

- stdio;
- Streamable HTTP.

Minimum lifecycle:

```text
connect/start -> initialize/negotiate -> initialized -> tools/list
  -> authorized tools/call -> cancellation/timeout -> graceful shutdown
```

Handle pagination and `tools.listChanged` for later runs; never mutate an active run's frozen registry. Wrap any Rust MCP SDK locally so SDK JSON-RPC, transport, process, HTTP-session, and version types do not leak into conversation/domain contracts.

### Provider-neutral registry integration

Implement `McpToolProvider` behind the existing registry boundary. MCP Tools normalize to the same `ToolSpec`, `ToolCall`, `ToolResult`, limits, policies, cancellation, and activity events as built-ins. Conversation logic must not branch on MCP source.

Each server has a stable ID. Expose names deterministically, for example:

```text
mcp.<server_id>.<encoded_tool_name>
```

Retain the original name for `tools/call`. Document encoding and reject malformed schemas, duplicate IDs/names, normalization collisions, and collisions with built-in or run/response controls. Never overwrite a handler.

Map MCP descriptions and `inputSchema` into `ToolSpec`. Treat server annotations as metadata, not authorization. Normalize text, `structuredContent`, and server-declared errors into bounded JSON. Unsupported binary/resource content must produce a safe description or stable error, not silent loss or automatic fetching.

### stdio transport

- Launch an explicitly configured executable with separate arguments and no shell.
- Do not interpolate, expand, pipe, redirect, or substitute commands.
- Pass only explicit environment entries/references.
- Reserve stdout for JSON-RPC and bound/redact stderr diagnostics.
- Apply declared framing, timeout, cancellation, and shutdown limits.
- Close input, wait a declared grace period, then terminate/kill if needed.
- Clean child processes on shutdown where possible.

Only a user/admin can register servers. Templates, Packs, imports, and models cannot launch processes. In self-hosted mode, commands execute on the Shirita server/container, not the browser device; say so in UI/docs.

### Streamable HTTP transport

Support target-version initialization, JSON/SSE responses, protocol/session headers, cancellation, declared timeouts, graceful shutdown, bounded redirects, and redacted diagnostics.

- HTTPS is normal.
- Plain HTTP requires explicit acceptance; loopback is the normal development case.
- Reject embedded URL credentials.
- Never forward Shirita auth, provider keys, cookies, or unrelated headers.
- Validate configured headers against a named routing/hop-by-hop denylist.
- Support explicit static secret headers or server-side environment references.
- Defer automatic OAuth.

### Typed storage and policy

Use a typed MCP server model, preferably a dedicated table, for stable ID/display name, enabled state, transport configuration, timeout settings, secret presence, and timestamps. API responses never return stored secret values. Runtime client construction revalidates configuration instead of trusting the save route.

Policy inheritance:

```text
global server registration and default per-Tool policy
  -> optional conversation policy override
  -> effective policy frozen at run start
```

Connection details/secrets remain server-owned and are not copied into conversations.

External Tools default to `disabled`:

```text
disabled  absent from the model-visible registry
allow     callable within run limits
ask       visible, but this call waits for user authorization
```

These are capability choices for RP generation, not a coding-Agent risk taxonomy.

For `ask`, bind the request to the authenticated session plus frozen run/call/server/Tool/arguments; show only a bounded redacted preview; accept a decision once; reject stale/mismatched decisions; execute only frozen arguments; return `authorization_denied` on denial when continuation is possible; make Stop resolve the wait; and never turn a one-time decision into a stored policy. Ownership today is the authenticated single-tenant data domain (see the API section note); per-conversation ownership must be added before multi-tenancy.

### MCP UI and API

Provide simple server CRUD, transport fields, redacted secrets, connection/discovery test, discovered Tools, global policies, and conversation policy overrides. Chat adds only authorization prompts and compact Tool status; it is not a protocol inspector or trace viewer.

Typed API equivalents:

```text
GET/POST       /api/mcp/servers
GET/PUT/DELETE /api/mcp/servers/{id}
POST           /api/mcp/servers/{id}/test
POST           /api/mcp/servers/{id}/refresh-tools
GET/PUT        /api/agent-settings
GET/PUT        /api/sessions/{id}/agent-settings
POST           /api/agent-runs/{run_id}/calls/{call_id}/approve
POST           /api/agent-runs/{run_id}/calls/{call_id}/deny
```

The generic settings endpoint is not the sole validation boundary. Run/call IDs do not authorize access without conversation ownership. **Current boundary:** `chat_sessions` carry no `user_id`, so conversations form an authenticated single-tenant data domain; the authorize/pending routes sit behind the shared auth gate, which is the ownership boundary today. If multi-tenancy is introduced later, conversations must first gain an owner and these routes must re-check it before resolving decisions.

## Declared limits

Extend centralized limits for:

- workspace snapshot, search/replacement total bytes, atomic patch operations, and, if separate, revisions per run;
- activity events and authorization previews;
- configured servers and effective MCP Tools;
- Tool names, descriptions, schemas, discovery pages, and results;
- connection, initialization, request, authorization-wait, and shutdown timeouts;
- stdio messages/stderr and HTTP responses/SSE frames/redirects.

Limits protect memory and availability. They must not encode an opinion about how an RP response is written or how many conceptual steps it takes.

## Implementation sequence

### Phase 5A

1. Add failing workspace and native/XML transcript tests.
2. Add workspace state plus replace and atomic batch-patch controls.
3. Make finish commit the workspace while retaining optional one-shot response.
4. Add canonical snapshot replacement, response-control history compaction, and native/XML transcript/truncation parity.
5. Normalize Tool errors and correction flow.
6. Complete Stop/lifecycle handling.
7. Add structured events and rebuild the frontend run activity UI.
8. Pass automated and real `gemma-4-e4b` gate checks and record them.

### Phase 5B

9. Select/wrap MCP support and add deterministic mock stdio/HTTP servers.
10. Implement both transports, lifecycle, pagination, calls, cancellation, and shutdown.
11. Add typed/redacted storage and CRUD/test/discovery APIs.
12. Adapt MCP Tools into the immutable registry with stable names/collisions.
13. Add global/conversation policies and runtime revalidation.
14. Execute `allow` Tools through native and XML.
15. Add `ask` state, APIs/events, cancellation, and UI.
16. Add settings UI and locale parity.
17. Complete desktop/self-hosted, security, regression, and documentation checks.

Keep tasks independently reviewable. Do not combine response semantics, transports, storage/API, registry, authorization, and UI in one commit.

## Verification

After focused tests:

```bash
cargo test --workspace
npm --prefix shirita-ui test
npm --prefix shirita-ui run build
git diff --check
```

Required categories include workspace atomicity/revisions and incremental batch ordering; stateless response handlers and typed run-owned control application; finish payloads, modes, and terminal fencing; exact round-1/continuation snapshot placement and serialization; native/XML mixed-call compaction fixtures; ordered multi-calls and next-round result visibility; pre-finish failure blocking; default Agent/XML instruction contracts; provider/Tool/final-response streaming boundaries; native/XML malformed/truncated streams; Stop boundaries; structured activity/usage; MCP negotiation/pagination/list changes/calls/cancellation/shutdown; stdio process cleanup; HTTP JSON/SSE/session behavior; names/collisions/schemas/results/limits; secret redaction; policy inheritance and authorization races; shared native/XML MCP handlers; prior chat/state/Panel/regex/token regressions; locale parity; and production UI build.

MCP tests use deterministic fixtures and never download or execute arbitrary third-party packages.

## Manual acceptance

1. Agent-off ordinary send/regenerate with no activity.
2. Native and XML `gemma-4-e4b`: short capability-result-response-edit-finish.
3. Direct native/XML one-shot finish.
4. Emit multiple independent Tools in one round and confirm source-order execution with results visible only in the next round.
5. Place calls after finish and a failed patch before finish; confirm the fence and blocked commit semantics.
6. Confirm workspace edits remain private until finish while provider/final SSE streaming and Stop behave as documented.
7. Internal/user status under all activity/status settings.
8. Stop during provider, response Tool, authorization, MCP HTTP, and MCP stdio work.
9. Register/call one minimal stdio Tool through native and XML.
10. Repeat with one minimal Streamable HTTP Tool.
11. Exercise disabled, allow, approve, deny, stale approval, and inheritance.
12. Refresh discovery during a run; only the next run changes.
13. Recover from protocol/schema/timeout/oversize/server errors in place.
14. Confirm no draft, private output, protocol, credential, or sensitive call data becomes a message.
15. Confirm only the finished workspace persists and failed/stopped runs leave branches unchanged.
16. Repeat representative flows in self-hosted Web and packaged Tauri/WebKit.

Keep live prompts and results short for low-memory hardware. Record exact environments and unperformed checks honestly.

## Completion and review checklist

Before completion, update README/current direction/architecture/module docs; document response visibility, MCP revision/transports, stdio execution location, HTTP auth limitations, secrets, policies, and limits; record live Web/Tauri checks; and leave state Tools, response persistence, metadata Tools, broader plugins, and MCP non-Tool capabilities as future work.

- [x] Shirita is treated as an RP platform with a neutral text-generation harness, not a coding Agent.
- [x] The response workspace is the run's explicit working object.
- [x] Every provider round receives exactly one complete current workspace snapshot.
- [x] No model-facing response read Tool or accumulated historical snapshot duplicates the canonical text.
- [x] Snapshot format, placement, and native/XML transcript compaction are locked by exact fixtures.
- [x] Ordinary output never mutates the response implicitly.
- [x] Replace and batch-patch mutations are revisioned, bounded, atomic, and recoverable.
- [x] Response handlers are stateless; only the run applies typed workspace controls.
- [x] Finish supports workspace commit and simple one-shot submission.
- [x] Phase 5A passes before MCP work begins (automated gate green; live `gemma-4-e4b` gate passed on 2026-08-02; packaged-desktop check still pending).
- [x] Native/XML capability-result-edit-finish works with the selected small model.
- [x] Multi-Tool rounds are ordered, results appear only in later rounds, and finish is a tested terminal fence.
- [x] Recoverable failures before finish cannot commit a stale or partially edited workspace.
- [x] Provider, Tool-call, final-response, and MCP streaming boundaries are explicit and tested (MCP part deferred to 5B).
- [x] Stop and finish prevent all later calls/rounds.
- [x] Activity/status is structured, transient, compact, run-scoped, and gated.
- [x] MCP supports stdio and Streamable HTTP for Tools only.
- [x] MCP Tools share the immutable registry/executor with built-ins.
- [x] Tool identities cannot overwrite handlers.
- [x] Configuration/secrets are typed, validated, redacted, and revalidated.
- [x] Disabled/allow/ask policies do not prescribe Tool purpose.
- [x] Limits are centralized with no hidden retries or constraints.
- [x] Agent-off and prior chat/runtime behavior remain intact.
- [ ] Desktop and self-hosted modes share behavior (packaged-desktop check pending).
- [x] Verification is recorded honestly.
- [x] Prompt composition remains Phase 6 and the cleanup endpoint.

Implementation note (2026-08-02): Phase 5A automated verification passes — the full Rust workspace suite (including a pre-existing, unrelated parallel-env race in two `config` tests that passes single-threaded), all UI tests, and the production UI build. The response workspace, replace/patch controls, finish commit + one-shot path, canonical snapshot overlay, response-control compaction, unified failures, Stop boundaries, and the run-scoped activity/status UI are implemented and committed.

Live acceptance gate passed against the real model on 2026-08-02. Environment: `llama-swap` (llama.cpp) OpenAI-compatible server at `http://localhost:8080/v1`, model `gemma-4-e4b` (reported `gemma-4-E4B-it-Q4_K_M.gguf`), driven by `cargo run --release --example live_gate` (kept in `shirita-core/examples/` for reproducibility). All five scenarios reached a valid finish: native capability(math)→edit(replace)→finish; native one-shot `finish(response)`; XML capability(math)→edit(replace)→finish; native multi-tool single round (math + random.choose) with next-round results; and XML one-shot `finish(response)`.

The live gate caught one real defect: with the XML transport, a retained capability call was serialized back into the next round as a native `tool_calls` field rather than the `<tool_call>` XML the model wrote, so the small model stopped continuing after a capability result (round-limit failure). Fixed by rendering retained calls into `<tool_call>` blocks in the assistant transcript for XML transport (`xml_tools::render_xml_tool_call` + `append_round`), with a round-trip test. After the fix, the XML capability-result-edit-finish flow completes.

Packaged Tauri/WebKit checks remain open.

Post-review fixes (2026-08-02, after the live gate): a review round found and fixed gaps that the existing tests did not cover — finish is now a terminal fence by normalized call name even when its arguments are invalid; finish emits exactly one ToolFinished after commit validation with a `{committed,revision,bytes}` result; one-shot `finish(response)` obeys the response-workspace ceiling; round-control system messages (snapshot, receipts, unfinished instruction) stay in-sequence for Anthropic via a `control` flag instead of being promoted into the top-level `system` field; `response.patch` enforces a replacement-bytes ceiling; the activity SSE carries the round number structurally and the frontend renders from the run-scoped `AgentRunView` with deterministic clearing on success. All five live gate scenarios still pass after the fixes.

Phase 5B implementation note (2026-08-02): MCP support is implemented and committed. A minimal Tools-only MCP client targets stable `2025-11-25` over stdio (explicit command/args/env, no shell, stdout reserved for JSON-RPC, bounded stderr diagnostics, child cleanup) and Streamable HTTP (JSON/SSE, session header, timeouts, bounded redirects; rejects embedded URL credentials and plain HTTP to remote hosts). Storage is typed/redacted with an `mcp_servers` table and a CRUD/test/refresh-tools API; secrets never leave the backend (`has_secret` + blank header/env values + secret-preserving merge). MCP Tools join the immutable registry with stable `mcp.<server>.<encoded>` names and collision rejection, gated by a global `mcp.policy` setting plus an optional per-conversation override frozen at run start (default disabled; `allow`/`ask`). `ask` Tools wait for a one-time approve/deny decision bound to the run/call/session with frozen arguments and a bounded redacted preview; stale decisions are rejected; Stop/expiry resolves the wait; decisions are never stored as policy. The chat UI polls pending authorizations and shows an approve/deny prompt; a settings UI covers server CRUD and per-tool policy with full locale parity. Deterministic mock servers (in-process HTTP responder + the `mcp_mock_stdio` binary) cover pagination, calls, `isError`, spawn failure, redaction, secret merge, policy gating, allow execution, and the ask wait/approve/deny/stale flows. Automated verification passes: the full Rust workspace suite, all UI tests, and the production UI build. Live-provider execution of an MCP Tool through the real `gemma-4-e4b` loop, and packaged Tauri/WebKit checks, remain open (recorded honestly).

Phase 5B review fixes (2026-08-02): a review round found gaps the original tests did not cover, all now fixed and tested. (1) `ask` authorization is no longer truncated by the ordinary Tool-call timeout: ask-policy Tools carry `requires_authorization`, the loop bounds their execution by the authorization-wait timeout, and the broker removes pending entries on approve/deny, internal timeout, Stop, or future drop via an RAII guard — no zombie requests. (2) the pending preview is now redacted (sensitive values masked recursively) and truncated on a character boundary, so Chinese/Japanese arguments cannot panic and secrets never reach the UI. (3) named, centralized, exposed MCP limits bound the HTTP body, stdio line, discovery pages, tool count, tool name/description/schema, result text, and config item counts/sizes; a duplicate-cursor check stops `nextCursor` loops. (4) configured headers are validated against a routing/hop-by-hop denylist and redirects stay same-origin so custom secret headers cannot be forwarded cross-host. (5) non-text Tool content is surfaced as a bounded safe description instead of silently dropped. (6) authorization ownership: `chat_sessions` carry no `user_id`, so conversations are an existing single-tenant data domain; the approve/deny/pending routes sit behind the authenticated gate and this is documented as the existing single-tenant constraint rather than inventing per-conversation ownership the model does not have.
