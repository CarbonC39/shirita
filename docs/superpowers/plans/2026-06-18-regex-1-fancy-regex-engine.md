# Regex Phase 1 — fancy-regex Engine Swap (Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Swap the `regex_rule` compile/validate/apply path from the `regex` crate to `fancy-regex` so SillyTavern patterns using lookaround and backreferences work.

**Architecture:** `fancy-regex` wraps `regex` and adds backtracking only for lookaround/backreferences; otherwise it uses the fast linear engine. Only the user-facing `regex_rule` functions (`is_valid_regex`, `apply_regex_rules`) switch. Engine-internal regexes (`<state_update>` parsing, `{{var}}` rendering) keep the `regex` crate — they need no fancy features.

**Tech Stack:** Rust, `fancy-regex = "0.13"` (already in `Cargo.lock` transitively).

## Global Constraints

- Code comments and git commit messages in English.
- `raw_content` is never mutated by regex; this phase only changes the engine, not what is stored.
- After each task: `cargo test -p shirita-core` green, then commit. Do not push.
- `fancy_regex::Regex::new` returns `Result`; `replace_all(text, rep) -> Cow<str>` mirrors the `regex` crate.

---

### Task 1: Add the fancy-regex dependency

**Files:**
- Modify: `shirita-core/Cargo.toml` (`[dependencies]`)

**Interfaces:**
- Produces: `fancy_regex::Regex` available to `shirita-core`.

- [ ] **Step 1: Add the dependency**

In `shirita-core/Cargo.toml`, under `[dependencies]`, add directly after the `regex = "1"` line:

```toml
regex = "1"
fancy-regex = "0.13"
```

- [ ] **Step 2: Verify it resolves and compiles**

Run: `cargo build -p shirita-core`
Expected: builds with no new version resolution (0.13.0 already in `Cargo.lock`).

- [ ] **Step 3: Commit**

```bash
git add shirita-core/Cargo.toml Cargo.lock
git commit -m "build(core): add fancy-regex dependency"
```

---

### Task 2: Switch `is_valid_regex` to fancy-regex

**Files:**
- Modify: `shirita-core/src/assembly.rs` (`is_valid_regex`, ~line 159)
- Test: `shirita-core/src/assembly.rs` (tests module)

**Interfaces:**
- Consumes: `fancy_regex::Regex` (Task 1).
- Produces: `pub fn is_valid_regex(pattern: &str) -> bool` (unchanged signature; now accepts lookaround/backrefs).

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `shirita-core/src/assembly.rs`:

```rust
#[test]
fn is_valid_regex_accepts_lookaround() {
    // Plain `regex` rejects lookahead; fancy-regex accepts it.
    assert!(is_valid_regex(r"foo(?=bar)"));
    assert!(is_valid_regex(r"(?<=\d)px"));
    assert!(is_valid_regex(r"(\w+)\s+\1")); // backreference
    assert!(!is_valid_regex(r"foo(")); // still invalid: unbalanced paren
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core is_valid_regex_accepts_lookaround`
Expected: FAIL — lookahead/backref assertions fail under the `regex` crate.

- [ ] **Step 3: Switch the implementation**

Replace the body of `is_valid_regex`:

```rust
/// 校验一条 regex_rule 的 pattern 能否编译（创作期使用；空 pattern 视为合法/无操作）。
/// 用 fancy-regex 引擎（支持 lookaround / 反向引用，吃下 ST 兼容）。
pub fn is_valid_regex(pattern: &str) -> bool {
    fancy_regex::Regex::new(pattern).is_ok()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p shirita-core is_valid_regex_accepts_lookaround`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/assembly.rs
git commit -m "feat(core): validate regex_rule patterns with fancy-regex (lookaround/backref)"
```

---

### Task 3: Switch `apply_regex_rules` to fancy-regex

**Files:**
- Modify: `shirita-core/src/assembly.rs` (`apply_regex_rules`, ~lines 165-205)
- Test: `shirita-core/src/assembly.rs` (tests module)

**Interfaces:**
- Consumes: `fancy_regex::Regex` (Task 1).
- Produces: `pub fn apply_regex_rules(text: &str, rules: &[Definition]) -> Option<String>` (unchanged signature; lookaround patterns now match).

- [ ] **Step 1: Write the failing test**

Add to the tests module (reuse the existing `def()` helper that builds a `Definition`):

```rust
#[test]
fn apply_regex_rules_supports_lookaround() {
    // Strip a trailing "px" only when preceded by digits (lookbehind).
    let mut r = def("regex_rule", "r", "");
    r.meta = serde_json::json!({ "pattern": r"(?<=\d)px", "replacement": "" });
    assert_eq!(apply_regex_rules("12px and apx", &[r]).as_deref(), Some("12 and apx"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p shirita-core apply_regex_rules_supports_lookaround`
Expected: FAIL — `regex::Regex::new` errors on the lookbehind, so the rule is skipped and text is unchanged (`"12px and apx"`).

- [ ] **Step 3: Switch the implementation**

In `apply_regex_rules`, change the compile + replace lines. Replace:

```rust
        if let Some(p) = pattern {
            match regex::Regex::new(p) {
                Ok(re) => out = re.replace_all(&out, replacement).into_owned(),
                Err(e) => tracing::warn!(rule = %rule.id, error = %e, "invalid regex_rule pattern, skipping"),
            }
        }
```

with:

```rust
        if let Some(p) = pattern {
            match fancy_regex::Regex::new(p) {
                Ok(re) => out = re.replace_all(&out, replacement).into_owned(),
                Err(e) => tracing::warn!(rule = %rule.id, error = %e, "invalid regex_rule pattern, skipping"),
            }
        }
```

- [ ] **Step 4: Run the full assembly tests**

Run: `cargo test -p shirita-core`
Expected: PASS — `apply_regex_rules_supports_lookaround` passes and all existing regex tests (`regex_rules_clean_text`, etc.) still pass (`$1`-style replacements behave identically under fancy-regex).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/assembly.rs
git commit -m "feat(core): apply regex_rule replacements with fancy-regex"
```

---

## Self-Review

- **Spec coverage (§1):** engine swap for application (Task 3) + validation (Task 2) ✓; internal `regex` usages (`state.rs`, `render_vars`) intentionally untouched ✓; `pattern_error` surfacing is Phase 4 (§3.3), not here.
- **Placeholders:** none.
- **Type consistency:** signatures of `is_valid_regex` / `apply_regex_rules` unchanged; only the engine type (`fancy_regex::Regex`) differs internally.
