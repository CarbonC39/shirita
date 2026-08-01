# Regex Phase 3 — Apply Matrix: read-time Display + outgoing-prompt (Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the full `targets`(ai_output/user_input) × `scope`(display/prompt/both) regex matrix: Display-side computed at read time (so rule edits refresh history instantly), Prompt-side applied to the outgoing request copy. `raw_content` is never mutated.

**Architecture:** A parameterized `apply_regex_rules_for(text, rules, target, phase)`; a shared `effective_regex_rules(storage, session)` (global orphan rules + rules referenced by the session's effective tree) used by both the read path and assembly. Write-time `display_content` keeps only rule-independent transforms (state-tag stripping, HTML-card reconstruction); the `list_messages` handler layers Display-phase regex on top per request. Assembly applies Prompt-phase regex to a copy of the history before sending.

**Tech Stack:** Rust (`shirita-core`, `shirita-web`); `fancy_regex` (Phase 1).

## Global Constraints

- Code comments and git commit messages in English.
- `raw_content` never changes; Prompt-phase transforms are ephemeral (request copy only); Display-phase is computed at read time, not stored.
- World-info trigger scanning keeps using the **original** message text (not the Prompt-regex'd copy).
- Reserved phases/targets: `scope ∈ {display, both, prompt}`, `targets ⊆ {ai_output, user_input}`, empty `targets` = applies to both (back-compat).
- After each task: `cargo test --workspace` green, then commit. Do not push.

---

### Task 1: Parameterized `apply_regex_rules_for`

**Files:**
- Modify: `shirita-core/src/assembly.rs` (`apply_regex_rules` → wrapper + new `apply_regex_rules_for`, enums)
- Modify: `shirita-core/src/lib.rs` (re-export `apply_regex_rules_for`, `RegexTarget`, `RegexPhase`)
- Test: `shirita-core/src/assembly.rs` (tests)

**Interfaces:**
- Produces:
  - `pub enum RegexTarget { AiOutput, UserInput }`
  - `pub enum RegexPhase { Display, Prompt }`
  - `pub fn apply_regex_rules_for(text: &str, rules: &[Definition], target: RegexTarget, phase: RegexPhase) -> Option<String>` — `None` when no applicable rule actually ran; otherwise the transformed text.
  - `pub fn apply_regex_rules(text, rules)` retained as `apply_regex_rules_for(text, rules, AiOutput, Display)`.

- [ ] **Step 1: Write the failing tests**

Add to the assembly tests module (uses the existing `def()` helper):

```rust
#[test]
fn apply_for_filters_by_phase_and_target() {
    let mut ai_disp = def("regex_rule", "ai_disp", "");
    ai_disp.meta = serde_json::json!({ "pattern": "X", "replacement": "", "scope": "display", "targets": ["ai_output"] });
    let mut user_prompt = def("regex_rule", "user_prompt", "");
    user_prompt.meta = serde_json::json!({ "pattern": "Y", "replacement": "", "scope": "prompt", "targets": ["user_input"] });
    let rules = vec![ai_disp, user_prompt];

    // ai_output × Display: only ai_disp applies.
    assert_eq!(apply_regex_rules_for("XY", &rules, RegexTarget::AiOutput, RegexPhase::Display).as_deref(), Some("Y"));
    // user_input × Prompt: only user_prompt applies.
    assert_eq!(apply_regex_rules_for("XY", &rules, RegexTarget::UserInput, RegexPhase::Prompt).as_deref(), Some("X"));
    // user_input × Display: neither applies (none ran).
    assert_eq!(apply_regex_rules_for("XY", &rules, RegexTarget::UserInput, RegexPhase::Display), None);
}

#[test]
fn apply_for_both_scope_covers_display_and_prompt() {
    let mut r = def("regex_rule", "r", "");
    r.meta = serde_json::json!({ "pattern": "Z", "replacement": "", "scope": "both" }); // empty targets = broad
    assert_eq!(apply_regex_rules_for("Z", &[r.clone()], RegexTarget::AiOutput, RegexPhase::Display).as_deref(), Some(""));
    assert_eq!(apply_regex_rules_for("Z", &[r], RegexTarget::UserInput, RegexPhase::Prompt).as_deref(), Some(""));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p shirita-core apply_for_`
Expected: FAIL — `apply_regex_rules_for` / `RegexTarget` / `RegexPhase` undefined.

- [ ] **Step 3: Implement**

In `assembly.rs`, add the enums above `apply_regex_rules`:

```rust
/// regex_rule 作用对象（哪一侧消息）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegexTarget { AiOutput, UserInput }
/// regex_rule 作用阶段（改显示 / 改发给模型的内容）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegexPhase { Display, Prompt }
```

Replace `apply_regex_rules` with the parameterized form + wrapper:

```rust
/// 依挂载顺序对文本应用适用的 regex_rule。按 (target, phase) 过滤：
/// `disabled` 跳过；phase 须匹配 `scope`（display→{display,both}，prompt→{prompt,both}）；
/// target 须在 `targets` 内（空/缺省 = 广义）。返回 None 表示没有任何适用规则真正执行。
/// 运行期宽容：非法 pattern 仅 warn 跳过（校验在创作期做）。
pub fn apply_regex_rules_for(
    text: &str,
    rules: &[Definition],
    target: RegexTarget,
    phase: RegexPhase,
) -> Option<String> {
    let target_key = match target {
        RegexTarget::AiOutput => "ai_output",
        RegexTarget::UserInput => "user_input",
    };
    let mut out = text.to_string();
    let mut ran = false;
    for rule in rules {
        if rule.meta.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let scope = rule.meta.get("scope").and_then(|v| v.as_str()).unwrap_or("display");
        let phase_ok = match phase {
            RegexPhase::Display => scope == "display" || scope == "both",
            RegexPhase::Prompt => scope == "prompt" || scope == "both",
        };
        if !phase_ok {
            continue;
        }
        if let Some(targets) = rule.meta.get("targets").and_then(|v| v.as_array()) {
            if !targets.is_empty() && !targets.iter().any(|t| t.as_str() == Some(target_key)) {
                continue;
            }
        }
        let pattern = rule.meta.get("pattern").and_then(|v| v.as_str());
        let replacement = rule.meta.get("replacement").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(p) = pattern {
            match fancy_regex::Regex::new(p) {
                Ok(re) => {
                    out = re.replace_all(&out, replacement).into_owned();
                    ran = true;
                }
                Err(e) => tracing::warn!(rule = %rule.id, error = %e, "invalid regex_rule pattern, skipping"),
            }
        }
    }
    ran.then_some(out)
}

/// AI 输出、显示侧的便捷封装（沿用旧调用点的语义）。
pub fn apply_regex_rules(text: &str, rules: &[Definition]) -> Option<String> {
    apply_regex_rules_for(text, rules, RegexTarget::AiOutput, RegexPhase::Display)
}
```

- [ ] **Step 4: Re-export**

In `shirita-core/src/lib.rs`, extend the `pub use assembly::{ ... }` list with `apply_regex_rules_for, RegexTarget, RegexPhase`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p shirita-core assembly`
Expected: PASS — new tests pass; existing `regex_rules_clean_text` and the `apply_regex_rules("aaa", ...)` tests still pass (a rule runs → `Some`; empty rules → `None`).

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/assembly.rs shirita-core/src/lib.rs
git commit -m "feat(core): parameterize regex application by target and phase"
```

---

### Task 2: Shared `effective_regex_rules` helper

**Files:**
- Modify: `shirita-core/src/conversation.rs` (add `effective_regex_rules`; use it in `assemble_request`)
- Modify: `shirita-core/src/lib.rs` (re-export `effective_regex_rules`)
- Test: `shirita-core/src/conversation.rs` (tests)

**Interfaces:**
- Consumes: `effective_nodes` (existing, this module), `Storage::referenced_definition_ids`/`list_definitions`/`list_nodes`.
- Produces: `pub async fn effective_regex_rules(storage: &dyn Storage, session: &Session) -> crate::Result<Vec<Definition>>` — global orphan regex rules, then rules referenced by enabled refs in the session's effective tree (disjoint; global first).

- [ ] **Step 1: Write the failing test**

Add to the conversation tests module:

```rust
#[tokio::test]
async fn effective_regex_rules_global_plus_scoped() {
    let storage = Arc::new(temp_storage().await);
    // global orphan rule (referenced by no node)
    let mut g = crate::models::definition::Definition::new("regex_rule", "G", "");
    g.meta = serde_json::json!({ "pattern": "g", "replacement": "" });
    storage.create_definition(&g).await.unwrap();
    // scoped rule referenced by a template the session uses
    let mut s = crate::models::definition::Definition::new("regex_rule", "S", "");
    s.meta = serde_json::json!({ "pattern": "s", "replacement": "" });
    storage.create_definition(&s).await.unwrap();
    let tmpl = crate::models::template::Template::new("rx");
    storage.create_template(&tmpl).await.unwrap();
    storage.create_node(&crate::models::prompt_node::PromptNode::new_ref(
        crate::models::prompt_node::OwnerKind::Template, &tmpl.id, None, 0, &s.id)).await.unwrap();
    let mut session = Session::new("x");
    session.template_id = Some(tmpl.id.clone());
    storage.create_session(&session).await.unwrap();

    let rules = super::effective_regex_rules(storage.as_ref(), &session).await.unwrap();
    let names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["G", "S"], "global orphan first, then scoped");

    // A different session without that template gets only the global rule.
    let other = Session::new("y");
    storage.create_session(&other).await.unwrap();
    let other_rules = super::effective_regex_rules(storage.as_ref(), &other).await.unwrap();
    assert_eq!(other_rules.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["G"]);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p shirita-core effective_regex_rules_global_plus_scoped`
Expected: FAIL — `effective_regex_rules` undefined.

- [ ] **Step 3: Implement the helper**

Add to `conversation.rs` (near `effective_nodes`):

```rust
/// 本会话生效的 regex 规则：全局 orphan 规则（不被任何节点引用，处处生效）+ 本会话
/// effective 树里被启用 ref 引用的 scoped 规则。两集合互斥；global 在前。
pub async fn effective_regex_rules(
    storage: &dyn Storage,
    session: &Session,
) -> crate::Result<Vec<Definition>> {
    let referenced: std::collections::HashSet<String> =
        storage.referenced_definition_ids().await?.into_iter().collect();
    let all = storage.list_definitions().await?;
    let mut rules: Vec<Definition> = all
        .iter()
        .filter(|d| d.def_type == "regex_rule" && !referenced.contains(&d.id))
        .cloned()
        .collect();
    let by_id: std::collections::HashMap<&str, &Definition> =
        all.iter().map(|d| (d.id.as_str(), d)).collect();
    for n in effective_nodes(storage, session).await? {
        if n.kind == crate::models::prompt_node::NodeKind::Ref && n.enabled {
            if let Some(d) = n.definition_id.as_deref().and_then(|id| by_id.get(id)) {
                if d.def_type == "regex_rule" {
                    rules.push((*d).clone());
                }
            }
        }
    }
    Ok(rules)
}
```

- [ ] **Step 4: Use it in `assemble_request`**

In `assemble_request`, replace the "Hybrid regex model" block (the `referenced` + `regex_rules` computation, down to `regex_rules.extend(plan.regex_rules.clone());`) with:

```rust
    // Global orphan rules + this session's tree-scoped rules (see effective_regex_rules).
    let regex_rules = effective_regex_rules(storage, session).await?;
```

- [ ] **Step 5: Re-export and run**

In `shirita-core/src/lib.rs`, add `effective_regex_rules` to the `pub use conversation::{ ... }` list.

Run: `cargo test -p shirita-core conversation`
Expected: PASS — new helper test passes; existing display/scoping tests still pass (assemble_request produces the same rule set as before).

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/conversation.rs shirita-core/src/lib.rs
git commit -m "feat(core): extract effective_regex_rules (global + scoped) helper"
```

---

### Task 3: Move Display-side regex off write-time

**Files:**
- Modify: `shirita-core/src/conversation.rs` (`resolve_display`: drop regex + param; update `send_message`/`regenerate` call sites; remove 3 now-redundant write-time display tests)
- Test: `shirita-core/src/conversation.rs`

**Interfaces:**
- Produces: `fn resolve_display(path: &[&Message], full: &str, cleaned: &str) -> Option<String>` — only rule-independent transforms (HTML reconstruction, else state-stripped text). Regex is no longer applied at write time.

- [ ] **Step 1: Update `resolve_display`**

Replace the body to drop regex:

```rust
/// 写侧 display_content：仅与 regex 规则无关的变换——HTML-card 重建优先，否则
/// state 标签剥离后的文本（与原文不同才存）。Display-side regex 改在读侧即时计算
/// （见 web `list_messages`），故此处不再套规则。
fn resolve_display(path: &[&Message], full: &str, cleaned: &str) -> Option<String> {
    if let Some(html) = crate::html_patch::reconstruct(latest_html_card(path).as_deref(), cleaned) {
        return Some(html);
    }
    (cleaned != full).then(|| cleaned.to_string())
}
```

- [ ] **Step 2: Update the two call sites**

In `send_message`: the assistant block becomes (bind the now-unused rules with `_`):

```rust
        let (req, _regex_rules) = match assemble_request(storage.as_ref(), &session, model, &context, &branch_state, summary_text.clone()).await {
```

(the `let (req, regex_rules)` → `let (req, _regex_rules)`) and:

```rust
        assistant.display_content = resolve_display(&path, &full, &cleaned);
```

Apply the identical two edits in `regenerate` (its `let (req, regex_rules)` → `let (req, _regex_rules)`, and `sibling.display_content = resolve_display(&path, &full, &cleaned);`).

- [ ] **Step 3: Remove the 3 write-time display tests**

Delete these tests (their scoping concern is now covered by `effective_regex_rules_global_plus_scoped` in Task 2, and end-to-end Display by Task 4's web test):
- `regex_rule_sets_display_content`
- `global_regex_rule_applies_without_a_tree`
- `scoped_regex_rule_does_not_leak_to_other_sessions`

- [ ] **Step 4: Run tests**

Run: `cargo test -p shirita-core conversation`
Expected: PASS — remaining tests green (state-tag stripping still sets display_content; HTML patch test unaffected).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/conversation.rs
git commit -m "refactor(core): stop applying regex at write time (moves to read-time display)"
```

---

### Task 4: Read-time Display regex in `list_messages` + `edit_message`

**Files:**
- Modify: `shirita-web/src/routes/sessions.rs` (`list_messages`, ~line 213)
- Modify: `shirita-web/src/routes/messages.rs` (`edit_message`: stop applying regex)
- Test: `shirita-web/tests/` (new integration test file) or an inline `#[cfg(test)]` in `sessions.rs`

**Interfaces:**
- Consumes: `shirita_core::{effective_regex_rules, apply_regex_rules_for, RegexTarget, RegexPhase, Role}`, `shirita_core::html_patch::is_html_document`.

- [ ] **Step 1: Write the failing test**

Create `shirita-web/tests/regex_display_test.rs` (mirror an existing web test's harness for building `AppState`/router; if web tests use a helper module, reuse it). Core assertion:

```rust
// Given a session whose tree references a regex rule that deletes "SECRET"
// from AI output (display scope), GET .../messages returns display_content
// with "SECRET" stripped while raw_content keeps it. Editing the rule and
// re-fetching reflects the change with no message re-write.
#[tokio::test]
async fn list_messages_applies_display_regex_at_read_time() {
    // ... build AppState with in-memory storage (see existing web tests) ...
    // seed: a regex_rule {pattern:"SECRET", replacement:"", scope:"display", targets:["ai_output"]}
    //       referenced by the session's template; one assistant message raw_content="a SECRET b".
    // GET /sessions/{id}/messages -> the assistant message display_content == "a  b", raw_content == "a SECRET b".
}
```

(Use the same construction style as the existing `shirita-web` route tests. If none exist as a harness, build `AppState` directly as `shirita-web`'s unit tests do.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p shirita-web list_messages_applies_display_regex_at_read_time`
Expected: FAIL — display_content still equals raw (no read-time regex yet).

- [ ] **Step 3: Implement read-time Display in `list_messages`**

Replace `list_messages` in `sessions.rs`:

```rust
pub async fn list_messages(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<Message>>, StatusCode> {
    use shirita_core::{apply_regex_rules_for, RegexPhase, RegexTarget, Role};
    let session = state
        .storage
        .get_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut msgs = state
        .storage
        .list_messages(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rules = shirita_core::effective_regex_rules(state.storage.as_ref(), &session)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for m in &mut msgs {
        let base = m.display_content.clone().unwrap_or_else(|| m.raw_content.clone());
        // Skip full HTML-card documents — RP regex is not meant to rewrite the card markup.
        if shirita_core::html_patch::is_html_document(&base) {
            continue;
        }
        let target = match m.role {
            Role::Assistant => RegexTarget::AiOutput,
            Role::User => RegexTarget::UserInput,
            Role::System => continue,
        };
        if let Some(s) = apply_regex_rules_for(&base, &rules, target, RegexPhase::Display) {
            m.display_content = Some(s);
        }
    }
    Ok(Json(msgs))
}
```

- [ ] **Step 4: Simplify `edit_message` (drop regex)**

In `messages.rs::edit_message`, replace the rules-loading + `msg.display_content = apply_regex_rules(...)` block with raw-only update (Display regex is read-time now):

```rust
    if let Some(content) = body.content {
        // Display-side regex is applied at read time (list_messages); store raw only.
        msg.display_content = None;
        msg.raw_content = content;
    }
```

(Remove the now-unused `list_definitions`/`filter`/`apply_regex_rules` lines and any now-unused imports.)

- [ ] **Step 5: Run tests**

Run: `cargo test -p shirita-web`
Expected: PASS — new read-time test passes; existing web tests green.

- [ ] **Step 6: Commit**

```bash
git add shirita-web/src/routes/sessions.rs shirita-web/src/routes/messages.rs shirita-web/tests/regex_display_test.rs
git commit -m "feat(web): apply Display-side regex at read time in list_messages"
```

---

### Task 5: Prompt-side regex in `assemble_request`

**Files:**
- Modify: `shirita-core/src/conversation.rs` (`assemble_request`: transform context copy before `build_chat_messages`)
- Test: `shirita-core/src/conversation.rs`

**Interfaces:**
- Consumes: `apply_regex_rules_for`, `RegexTarget`, `RegexPhase` (Task 1), `regex_rules` (Task 2, already in scope in `assemble_request`).

- [ ] **Step 1: Write the failing test**

Add to the conversation tests module:

```rust
#[tokio::test]
async fn prompt_side_regex_rewrites_outgoing_not_raw() {
    let storage = Arc::new(temp_storage().await);
    // rule: replace "dog"->"cat" on user_input, prompt scope.
    let mut rule = crate::models::definition::Definition::new("regex_rule", "R", "");
    rule.meta = serde_json::json!({ "pattern": "dog", "replacement": "cat", "scope": "prompt", "targets": ["user_input"] });
    storage.create_definition(&rule).await.unwrap();
    let tmpl = crate::models::template::Template::new("rx");
    storage.create_template(&tmpl).await.unwrap();
    storage.create_node(&crate::models::prompt_node::PromptNode::new_ref(
        crate::models::prompt_node::OwnerKind::Template, &tmpl.id, None, 0, &rule.id)).await.unwrap();
    let mut session = Session::new("s");
    session.template_id = Some(tmpl.id.clone());
    storage.create_session(&session).await.unwrap();

    let seen = Arc::new(Mutex::new(None));
    let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
    let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    let s = send_message(storage.clone(), provider, counter, "m".into(), session.id.clone(), "my dog".into(), "".into(), Vec::new());
    futures::pin_mut!(s);
    while s.next().await.is_some() {}

    // Outgoing prompt has "my cat"; stored user raw_content keeps "my dog".
    let req = seen.lock().unwrap().clone().unwrap();
    assert!(req.messages.iter().any(|m| m.role == Role::User && m.content == "my cat"));
    let msgs = storage.list_messages(&session.id).await.unwrap();
    assert!(msgs.iter().any(|m| m.role == Role::User && m.raw_content == "my dog"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p shirita-core prompt_side_regex_rewrites_outgoing_not_raw`
Expected: FAIL — outgoing user message is still "my dog".

- [ ] **Step 3: Implement**

In `assemble_request`, after `regex_rules` is computed (Task 2) and before `let chat_messages = crate::assembly::build_chat_messages(&plan, context, include_history);`, build a Prompt-transformed copy and use it:

```rust
    // Prompt-side regex: rewrite the outgoing copy of each chat message by role
    // (scope ∈ {prompt, both}); raw_content is untouched. World-info scanning above
    // already used the original `context`, so triggers are unaffected.
    let prompt_context: Vec<ChatMessage> = context
        .iter()
        .map(|m| {
            let target = match m.role {
                Role::Assistant => Some(crate::assembly::RegexTarget::AiOutput),
                Role::User => Some(crate::assembly::RegexTarget::UserInput),
                Role::System => None,
            };
            match target {
                Some(t) => {
                    let content = crate::assembly::apply_regex_rules_for(
                        &m.content, &regex_rules, t, crate::assembly::RegexPhase::Prompt,
                    )
                    .unwrap_or_else(|| m.content.clone());
                    ChatMessage { content, ..m.clone() }
                }
                None => m.clone(),
            }
        })
        .collect();
    let chat_messages = crate::assembly::build_chat_messages(&plan, &prompt_context, include_history);
```

(Replace the existing `let chat_messages = ... build_chat_messages(&plan, context, ...)` line.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p shirita-core conversation`
Expected: PASS — prompt-side test passes; others green.

- [ ] **Step 5: Full workspace**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(core): apply Prompt-side regex to outgoing messages in assembly"
```

---

## Self-Review

- **Spec coverage (§2):** matrix function (Task 1) ✓; shared effective rules (Task 2, spec §2.4) ✓; read-time Display incl. user messages + HTML-card skip (Tasks 3-4, spec §2.2) ✓; Prompt-side ephemeral + world-info on original text (Task 5, spec §2.3) ✓. Frontend scope three-way (§2.5) is intentionally in Plan 4 with the rest of the regex UI.
- **Placeholders:** Task 4 Step 1 references "existing web test harness" — the implementer must mirror the project's actual `shirita-web` test setup; the assertion and behavior are fully specified.
- **Type consistency:** `apply_regex_rules_for` / `RegexTarget` / `RegexPhase` defined in Task 1 and consumed in Tasks 4-5; `effective_regex_rules` defined in Task 2, consumed in assembly and `list_messages`; `resolve_display` arity change (Task 3) matches both call sites.
