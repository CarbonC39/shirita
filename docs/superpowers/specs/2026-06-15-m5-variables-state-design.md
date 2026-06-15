# M5 — Dynamic Variables & State Sandbox (Design Spec)

> Milestone M5 of the Shirita roadmap (`2026-06-12-shirita-roadmap-design.md`). Builds on
> M2 (variable *rendering*: `render_vars` substitutes `{{var}}` from `current_state`) and
> M4 (message tree + per-message `snapshot_state` + `active_leaf_id`). This milestone adds
> the **write side**: the model mutates conversation state through a sandboxed instruction
> set, state is snapshotted per message and isolated per branch, and a read-only variable
> panel plus system-bound variables (`$avatar`/`$background`) surface it.

## Goals / Done criteria

- The model emits `<state_update action="SUB" key="hp" value="5"/>` (and friends); the engine
  applies it to the conversation's state, strips the tag from the displayed text, and records
  the result as the message's snapshot.
- State is **per-message and per-branch**: swiping to a sibling, regenerating, or forking shows
  that branch's values; the global library is untouched.
- Variables are **pre-declared and typed**; the sandbox only mutates declared keys, type-checked.
- **System variables** `$avatar` and `$background` are built in and drive the rendered avatar /
  chat background reactively, per branch.
- A collapsible read-only **Variables** panel shows the active branch's values (System / Custom),
  and the Book page lets you declare variables (template-level + per-chat), reusing the M4 split.
- Adding a variable to the schema later does not break existing branches (stale snapshots
  backfill to the declared initial).

## Non-goals (deferred)

- Native provider `tool_calls` channel — designed-for but not implemented (see "The seam").
- Recomputing snapshots when a message is edited or hidden (history is immutable in M5).
- Manual variable editing from the chat panel.
- Rich list/string operations beyond the basic instruction set below.

---

## 1. Variable model

Two classes of variables, both stored as plain keys in the conversation's state JSON.

### System variables (built-in registry)
A fixed registry shipped by the engine, reserved `$`-namespace, no declaration. Each is bound to
a render behavior:

| Key | Type | Binding |
|-----|------|---------|
| `$avatar` | asset path (string) | the avatar shown in the chat (falls back to `session.avatar`) |
| `$background` | asset path (string) | the chat/app background (falls back to none / app default) |

Extensible later (`$name`, `$time`, …). The model reads/writes them through the *same*
`<state_update>` mechanism; the frontend reflects the active-branch values reactively.

### Custom variables (declared, typed)
Declared as `{ name, type, initial }` where `type ∈ { number, bool, string, list }`. They live in
two places, mirroring the M4 copy-on-write split:

- **Template blueprint:** `template.meta.variables: [{name, type, initial}]` — the default schema
  for any conversation created from that template.
- **Per-chat local:** `session.override_config.local_variables: [{name, type, initial}]` — variables
  a single conversation adds on top.

Definitions carry **no** variable schema — they are constant content. The schema belongs to the
conversation (via its template blueprint and its own local additions).

### Effective schema
For a session, the effective schema is the union, in precedence order (later wins on name clash):

```
system registry  <  template.meta.variables  <  session.override_config.local_variables
```

---

## 2. State storage & the effective-state merge

Three JSON layers already exist (no migration needed):

- `session.current_state` — the **seed**: declared initials captured at creation. Persisted.
- `message.snapshot_state` — the **cumulative state at that message** (column exists, unused until M5).
- the schema (above) — supplies initials for backfill.

**Reads never trust a snapshot blindly.** The single source of truth for "what are the current
values on this branch" is:

```
effective_state(schema, seed, leafSnapshot) =
    { ...schemaInitials(schema), ...seed, ...leafSnapshot }   // later layer wins per key
```

- `schemaInitials` — initials of the *current* effective schema, so a newly-declared variable
  always resolves to its initial even on a branch whose snapshots predate it.
- `seed` — `session.current_state` (values fixed at creation; survive schema-initial edits).
- `leafSnapshot` — the active branch's evolved values, winning for keys it actually holds.

This makes existing snapshots **immune to schema growth** (new keys backfill) and **type-tolerant**
(unknown/stale keys are ignored by `apply_updates`). The snapshot is a cache of evolution, not the
canonical state.

`effective_state` is **one function used in three places**: assembly's `{{var}}` rendering, the
variable panel API, and — critically — the **fold base at apply-time** (§4), so writes get the same
backfill as reads.

### Branch isolation
- Each assistant turn stores its post-update effective state on the new message's `snapshot_state`
  (§4). The **active leaf's** snapshot is the branch's state.
- User messages carry the parent's effective state forward (cosmetic; the active leaf after a
  completed turn is always the assistant).
- Fork copies messages including `snapshot_state` (M4 already deep-copies the path) and the seed, so
  a fork keeps its variable state. Swipe/regenerate read the chosen branch's leaf.
- **Immutable history (MVP):** editing a message's text does not recompute downstream snapshots;
  hiding a message does not un-apply state already folded into descendants.

---

## 3. The sandbox (pure Rust, no eval)

New core module `state` (e.g. `shirita-core/src/state.rs`). Pure, deterministic, no I/O:

- `parse_state_updates(text) -> Vec<Update>` — scan for self-closing tags
  `<state_update action="…" key="…" value="…"/>` (whitespace-tolerant; multiple per message;
  applied in document order). `Update { action, key, value }`.
- `strip_state_tags(text) -> String` — remove all matched tags (for the displayed text), like the
  existing regex-rule cleaning of `display_content`.
- `apply_updates(state, schema, updates) -> state` — type-aware fold. For each update, look up the
  key in the effective schema; **ignore if undeclared or type-mismatched**:

| action | applies to | semantics |
|--------|-----------|-----------|
| `SET` | any | set the value, coerced to the declared type |
| `ADD` / `SUB` | number | add / subtract the numeric value |
| `TOGGLE` | bool | flip (no `value` needed) |
| `APPEND` / `REMOVE` | list | add element / remove first matching element |

Coercion failures (e.g. `ADD` with a non-numeric value) are ignored. The sandbox never executes
model-supplied code; it is a fixed match over actions.

### The seam (native tool calls, later)
`apply_updates` consumes `Vec<Update>`. A future native-tool-calling adapter would translate
provider `delta.tool_calls` into the same `Vec<Update>` and call the same `apply_updates` — no
change to the sandbox or storage. M5 only wires the tag path (works offline, testable with
`EchoProvider`).

---

## 4. Streaming integration

In `conversation.rs`, both `send_message` and `regenerate` change at the persist step (after the
full reply is streamed):

1. Resolve the effective **schema** for the session (system ∪ template ∪ local).
2. Compute the parent branch state: `parent = effective_state(schema, session.current_state,
   activePathLeafSnapshot)`. (First turn: leaf snapshot is empty → `schemaInitials ∪ seed`.)
3. **Assembly reads `parent`** (replacing the current `&session.current_state` argument to
   `assemble_from_nodes`), so `{{hp}}` etc. reflect the branch.
4. After the stream completes:
   - `updates = parse_state_updates(full)`; `newSnapshot = apply_updates(parent, schema, updates)`.
   - assistant `raw_content` keeps the tags (faithful record, fed back as history);
     `display_content = strip_state_tags(apply_regex_rules(full))`.
   - store `newSnapshot` on the assistant message's `snapshot_state`; M4's
     `set_session_active_leaf` already advances the leaf.

The user message created at the top of the turn carries `parent` as its `snapshot_state`.

---

## 5. Web API

- `GET /api/sessions/{id}/state` → the **effective state + schema** for the active branch
  (computed server-side with the §2 merge so the merge logic stays single-sourced in Rust).
  Shape (illustrative): `{ schema: [{name,type,initial,scope}], values: { "$avatar": "...", "hp": 95, ... } }`.
  The frontend re-fetches after load / send / swipe.
- `PUT /api/sessions/{id}/local-variables` → replace `override_config.local_variables`
  (mirrors M4's local-definitions endpoint).
- Template variable editing uses a template-meta update path (extend `PUT /api/templates/{id}` to
  accept `meta`, or a dedicated `…/variables` route — finalized in the plan).

No new migration: `template.meta`, `session.current_state`, `session.override_config`, and
`message.snapshot_state` already exist.

---

## 6. Frontend

- **Chat — Variables panel:** a collapsible, read-only panel (single-column minimalist layout)
  with two groups, **System** and **Custom**, bound to `GET …/state` for the active branch. Refreshes
  on load, after a send/regenerate completes, and after a swipe.
- **`$avatar` / `$background` binding:** `ChatView` reads the effective state; a set `$avatar`
  overrides the displayed avatar and `$background` sets the chat background, per branch (resolved via
  `/assets/{path}`).
- **Book — Variables declaration:** a Variables section in the editor. Template-level declarations
  (全局) edit `template.meta.variables`; per-chat declarations (局部) edit
  `override_config.local_variables`, reusing the M4 局部/全局 Book layout. Each row: name / type /
  initial.

---

## 7. Scope & plans

Two TDD plans, executed 1a → 1b:

- **Plan 1a — backend:** `state` module (`parse_state_updates` / `strip_state_tags` / `apply_updates`),
  schema resolution + `effective_state`, seeding at session creation, snapshot folding in
  `send_message` + `regenerate`, assembly reading the branch state, `GET …/state` and the
  local-variable / template-meta endpoints.
- **Plan 1b — frontend:** the Variables panel + state client fn, `$avatar`/`$background` binding in
  the chat view, and the Book Variables declaration UI (template + per-chat).

### Self-bounded units
- `state` module: pure functions, fully unit-testable, depend on nothing but the schema + state JSON.
- `effective_state`: one function, three call sites; changing the merge can't desync read vs. write.
- The sandbox is decoupled from the provider via `Vec<Update>` (the native-tool seam).
