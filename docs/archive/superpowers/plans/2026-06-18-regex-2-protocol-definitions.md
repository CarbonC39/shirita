# Regex Phase 2 — Protocol Definitions (#3 + HTML-patch migration) (Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Represent the `<state_update>` protocol instruction (and the existing HTML-card patch instruction) as seeded builtin `protocol` Definitions, and have the engine auto-inject them — appending the live variable list for the state_update protocol.

**Architecture:** A new reserved def_type `protocol`. Two builtin definitions (fixed ids) are seeded at startup, holding the static protocol texts. In `assemble_request`, every `protocol` definition is injected as an AfterHistory system segment when its `meta.kind` trigger holds: `state_update` when the session declares a non-system variable (and then appends the current variable list), `html_patch` when the conversation contains a card.

**Tech Stack:** Rust (`shirita-core`, `shirita-web`).

## Global Constraints

- Code comments and git commit messages in English.
- Protocol text is model-facing → write it in English.
- The state_update trigger is "has a **non-system** variable": `resolve_schema` always includes the 3 `$`-prefixed system vars, so a plain non-empty check is wrong.
- After each task: `cargo test -p shirita-core` (and `-p shirita-web` where noted) green, then commit. Do not push.
- Depends on Phase 1 only for being on the same branch; no code dependency.

---

### Task 1: Add `protocol` to the reserved def types

**Files:**
- Modify: `shirita-core/src/models/def_type.rs` (`RESERVED`, ~line 6)
- Test: `shirita-core/src/models/def_type.rs` (tests module)

**Interfaces:**
- Produces: `is_reserved("protocol") == true`.

- [ ] **Step 1: Write the failing test**

Add to the tests module:

```rust
#[test]
fn protocol_is_reserved() {
    assert!(is_reserved("protocol"));
    assert!(!is_prompt("protocol"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core protocol_is_reserved`
Expected: FAIL — `is_reserved("protocol")` is false.

- [ ] **Step 3: Add the type**

Replace the `RESERVED` constant and its doc comment:

```rust
/// 保留类型（代码常量，永不入 def_types 表，不进节点树容器）。
pub const RESERVED: [&str; 5] = ["prompt", "regex_rule", "tool", "first_message", "protocol"];
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p shirita-core protocol_is_reserved`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/models/def_type.rs
git commit -m "feat(core): add reserved def_type 'protocol'"
```

---

### Task 2: Build the live variable-list block

**Files:**
- Modify: `shirita-core/src/state.rs` (add `variables_block`)
- Modify: `shirita-core/src/lib.rs` (re-export, ~line 50 `pub use state::{...}`)
- Test: `shirita-core/src/state.rs` (tests module)

**Interfaces:**
- Produces: `pub fn variables_block(schema: &[VarDecl], state: &Value) -> Option<String>` — `None` when there is no non-system variable (this doubles as the state_update trigger).

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `state.rs`:

```rust
#[test]
fn variables_block_lists_only_user_vars() {
    let schema = vec![
        VarDecl { name: "$avatar".into(), var_type: VarType::String, initial: Value::String("".into()), scope: Some("system".into()) },
        VarDecl { name: "hp".into(), var_type: VarType::Number, initial: serde_json::json!(100), scope: Some("template".into()) },
    ];
    let state = serde_json::json!({ "hp": 80 });
    let block = variables_block(&schema, &state).unwrap();
    assert!(block.contains("- hp (number) = 80"));
    assert!(!block.contains("$avatar")); // system vars excluded
}

#[test]
fn variables_block_is_none_without_user_vars() {
    let schema = system_variables(); // only $-vars
    assert_eq!(variables_block(&schema, &serde_json::json!({})), None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core variables_block`
Expected: FAIL — `variables_block` not defined.

- [ ] **Step 3: Implement**

Add to `state.rs` (near `schema_initials`):

```rust
/// 当前变量清单文本（仅非系统变量，取当前值否则初值）。无用户变量时返回 None，
/// 这同时作为 state_update 协议的注入触发条件。
pub fn variables_block(schema: &[VarDecl], state: &Value) -> Option<String> {
    let lines: Vec<String> = schema
        .iter()
        .filter(|d| d.scope.as_deref() != Some("system"))
        .map(|d| {
            let val = state.get(&d.name).cloned().unwrap_or_else(|| d.initial.clone());
            let ty = match d.var_type {
                VarType::Number => "number",
                VarType::Bool => "bool",
                VarType::String => "string",
                VarType::List => "list",
            };
            format!("- {} ({}) = {}", d.name, ty, val)
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(format!("Current variables:\n{}", lines.join("\n")))
}
```

- [ ] **Step 4: Re-export**

In `shirita-core/src/lib.rs`, add `variables_block` to the `pub use state::{ ... }` list.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p shirita-core variables_block`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/state.rs shirita-core/src/lib.rs
git commit -m "feat(core): variables_block — live variable list for protocol injection"
```

---

### Task 3: Seed the builtin protocol definitions

**Files:**
- Modify: `shirita-core/src/seed.rs` (add constants + `ensure_builtin_definitions`)
- Modify: `shirita-core/src/lib.rs` (re-export, ~line 49 `pub use seed::ensure_default_template;`)
- Modify: `shirita-web/src/main.rs` (call after `ensure_default_template`, ~line 19)
- Test: `shirita-core/src/seed.rs` (tests module)

**Interfaces:**
- Consumes: reserved type `protocol` (Task 1), `crate::html_patch::INSTRUCTION` (existing const).
- Produces: `pub async fn ensure_builtin_definitions<S: Storage + ?Sized>(storage: &S) -> Result<()>`; two definitions with ids `builtin-protocol-state-update` and `builtin-protocol-html-patch`, def_type `protocol`, `meta.kind` `state_update`/`html_patch`.

- [ ] **Step 1: Write the failing test**

Add to the tests module in `seed.rs` (it already has a `mem_storage()` helper):

```rust
#[tokio::test]
async fn seeds_protocol_definitions_idempotently() {
    let storage = mem_storage().await;
    ensure_builtin_definitions(&storage).await.unwrap();
    ensure_builtin_definitions(&storage).await.unwrap(); // idempotent

    let protos: Vec<_> = storage
        .list_definitions().await.unwrap()
        .into_iter().filter(|d| d.def_type == "protocol").collect();
    assert_eq!(protos.len(), 2, "exactly two builtin protocols, no duplicates");
    let su = protos.iter().find(|d| d.id == "builtin-protocol-state-update").unwrap();
    assert_eq!(su.meta["kind"], "state_update");
    assert!(su.content.contains("<state_update"));
    let hp = protos.iter().find(|d| d.id == "builtin-protocol-html-patch").unwrap();
    assert_eq!(hp.meta["kind"], "html_patch");
    assert!(hp.content.contains("<<<<<<< SEARCH"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core seeds_protocol_definitions_idempotently`
Expected: FAIL — `ensure_builtin_definitions` not defined.

- [ ] **Step 3: Implement**

At the top of `seed.rs`, add the import and constants:

```rust
use crate::models::definition::Definition;

/// 状态变量更新协议说明（注入给模型；内容随 builtin 定义可被用户编辑）。
const STATE_PROTOCOL_TEXT: &str = "\
You can update tracked story variables by emitting self-closing <state_update> \
tags inline in your reply. They are folded into the running state and stripped \
from what the reader sees.

Syntax: <state_update action=\"ACTION\" key=\"VAR\" value=\"VALUE\"/>
Actions:
- SET — set VAR to VALUE
- ADD — add numeric VALUE to VAR
- SUB — subtract numeric VALUE from VAR
- TOGGLE — flip a boolean VAR (omit value)
- APPEND — append VALUE to a list/string VAR
- REMOVE — remove VALUE from a list VAR
Only emit updates for variables that actually change; keep narrative prose separate from the tags.";

/// (id, name, content, kind) for each seeded builtin `protocol` definition.
const BUILTIN_PROTOCOLS: [(&str, &str, &str, &str); 2] = [
    ("builtin-protocol-state-update", "Variable Update Protocol", STATE_PROTOCOL_TEXT, "state_update"),
    ("builtin-protocol-html-patch", "HTML Card Patch Protocol", crate::html_patch::INSTRUCTION, "html_patch"),
];
```

Add the function (after `ensure_default_template`):

```rust
/// Seed the builtin `protocol` definitions (fixed ids, create-if-absent so it is
/// idempotent and self-heals if one was deleted). Their content is the static
/// protocol text the engine injects (see conversation::assemble_request).
pub async fn ensure_builtin_definitions<S: Storage + ?Sized>(storage: &S) -> Result<()> {
    for (id, name, content, kind) in BUILTIN_PROTOCOLS {
        if storage.get_definition(id).await?.is_none() {
            let mut d = Definition::new("protocol", name, content);
            d.id = id.to_string();
            d.meta = serde_json::json!({ "kind": kind });
            storage.create_definition(&d).await?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Re-export and wire startup**

In `shirita-core/src/lib.rs`, next to `pub use seed::ensure_default_template;` add:

```rust
pub use seed::{ensure_builtin_definitions, ensure_default_template};
```

(remove the old single-item `pub use seed::ensure_default_template;` line to avoid a duplicate import.)

In `shirita-web/src/main.rs`, after the `ensure_default_template` call (~line 19) add:

```rust
    shirita_core::ensure_builtin_definitions(&storage).await?;
```

- [ ] **Step 5: Run tests + build**

Run: `cargo test -p shirita-core seeds_protocol_definitions_idempotently && cargo build -p shirita-web`
Expected: PASS + builds.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/seed.rs shirita-core/src/lib.rs shirita-web/src/main.rs
git commit -m "feat(core,web): seed builtin protocol definitions at startup"
```

---

### Task 4: Inject protocol definitions in `assemble_request`

**Files:**
- Modify: `shirita-core/src/conversation.rs` (`assemble_request`: replace the HTML-patch push block, ~lines 130-145)
- Test: `shirita-core/src/conversation.rs` (update `html_card_patch_reconstructs_display_and_injects_instruction`; add a state_update test)

**Interfaces:**
- Consumes: `variables_block` (Task 2), seeded protocol defs (Task 3), `session_schema` (existing private fn in this module), `crate::html_patch::{is_html_document, has_patch_blocks}` (existing).
- Produces: AfterHistory system segments `source = "protocol:<kind>"`.

- [ ] **Step 1: Replace the injection block**

In `assemble_request`, replace the current HTML-patch block:

```rust
    let has_card = context
        .iter()
        .any(|m| crate::html_patch::is_html_document(&m.content) || crate::html_patch::has_patch_blocks(&m.content));
    if has_card {
        plan.segments.push(crate::assembly::PromptSegment {
            placement: crate::assembly::Placement::AfterHistory,
            content: crate::html_patch::INSTRUCTION.to_string(),
            source: "html_patch".into(),
        });
    }
```

with the unified protocol loop:

```rust
    // Auto-inject protocol instructions. Their text lives in builtin `protocol`
    // definitions (spec §4); each is injected after history when its meta.kind
    // trigger holds. state_update fires when the session declares a non-system
    // variable (and appends the live variable list); html_patch fires when the
    // conversation already holds a card. Both may coexist; the provider adapter
    // merges adjacent System segments.
    let has_card = context.iter().any(|m| {
        crate::html_patch::is_html_document(&m.content) || crate::html_patch::has_patch_blocks(&m.content)
    });
    let schema = session_schema(storage, session).await;
    let protocols = storage.list_definitions().await?;
    for pdef in protocols.iter().filter(|d| d.def_type == "protocol") {
        let kind = pdef.meta.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let content = match kind {
            "state_update" => match crate::state::variables_block(&schema, state) {
                Some(block) => format!("{}\n\n{}", pdef.content, block),
                None => continue,
            },
            "html_patch" => {
                if !has_card {
                    continue;
                }
                pdef.content.clone()
            }
            _ => continue,
        };
        plan.segments.push(crate::assembly::PromptSegment {
            placement: crate::assembly::Placement::AfterHistory,
            content,
            source: format!("protocol:{kind}"),
        });
    }
```

- [ ] **Step 2: Update the existing HTML-card test to seed builtins**

In `html_card_patch_reconstructs_display_and_injects_instruction`, right after `storage.create_session(&session).await.unwrap();`, add:

```rust
        crate::seed::ensure_builtin_definitions(storage.as_ref()).await.unwrap();
```

(The seeded `html_patch` protocol content is `html_patch::INSTRUCTION`, which contains `<<<<<<< SEARCH`, so the existing assertions still hold.)

- [ ] **Step 3: Add a state_update injection test**

Add to the tests module:

```rust
#[tokio::test]
async fn state_protocol_injected_only_when_user_vars_declared() {
    let storage = Arc::new(temp_storage().await);
    let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

    // Template declaring a user variable `hp`.
    let mut t = crate::models::template::Template::new("T");
    t.meta = serde_json::json!({ "variables": [ {"name":"hp","type":"number","initial":100} ] });
    storage.create_template(&t).await.unwrap();
    let mut session = Session::new("s");
    session.template_id = Some(t.id.clone());
    storage.create_session(&session).await.unwrap();
    crate::seed::ensure_builtin_definitions(storage.as_ref()).await.unwrap();

    let seen = Arc::new(Mutex::new(None));
    let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
    let s = send_message(storage.clone(), provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
    futures::pin_mut!(s);
    while s.next().await.is_some() {}

    let req = seen.lock().unwrap().clone().unwrap();
    let sys = req.messages.iter().filter(|m| m.role == Role::System).map(|m| m.content.clone()).collect::<Vec<_>>().join("\n");
    assert!(sys.contains("<state_update"), "protocol text injected");
    assert!(sys.contains("- hp (number) = 100"), "live variable list appended");
}

#[tokio::test]
async fn state_protocol_absent_without_user_vars() {
    let storage = Arc::new(temp_storage().await);
    let session = Session::new("s"); // no template → only system vars
    storage.create_session(&session).await.unwrap();
    crate::seed::ensure_builtin_definitions(storage.as_ref()).await.unwrap();
    let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

    let seen = Arc::new(Mutex::new(None));
    let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
    let s = send_message(storage.clone(), provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
    futures::pin_mut!(s);
    while s.next().await.is_some() {}

    let req = seen.lock().unwrap().clone().unwrap();
    assert!(!req.messages.iter().any(|m| m.content.contains("<state_update")), "no state protocol without user vars");
}
```

- [ ] **Step 4: Run the conversation tests**

Run: `cargo test -p shirita-core conversation`
Expected: PASS — new state_update tests pass; the updated HTML-card test passes; all others green.

- [ ] **Step 5: Run the full workspace**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(core): inject protocol definitions (state_update + html_patch) in assembly"
```

---

## Self-Review

- **Spec coverage (§4):** reserved `protocol` type (Task 1) ✓; seeded builtin defs for both protocols (Task 3) ✓; engine injection by `meta.kind` with state_update variable-list append + html_patch card trigger (Task 4) ✓; HTML-patch migrated off the hardcoded direct push, `INSTRUCTION` retained only as seed content (Tasks 3-4) ✓; state_update trigger = non-system variable (Task 2) ✓.
- **Placeholders:** none — protocol text is concrete.
- **Type consistency:** `variables_block(&[VarDecl], &Value) -> Option<String>` defined in Task 2 and consumed in Task 4; `ensure_builtin_definitions` defined in Task 3, used in Task 4 tests and `main.rs`; def ids/kinds match between Task 3 (seed) and Task 4 (consume).
