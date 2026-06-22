# ST Preset Import Fidelity v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite `stpreset_to_loreset` so an imported ST preset keeps every authored prompt by enabled/disabled status, recognizes `setvar`/`getvar` as template variables, gives sections `wrap_in_tag` structure, and bundles cross-node XML spans into folders.

**Architecture:** All logic lives in the core adapter `shirita-core/src/adapters/stpreset.rs` plus a one-line guard in the shared `sanitize_tag` (`assembly.rs`). The web `persist_preset` path is unchanged (it already stores `template.meta` and creates folders before their child Refs). Pure helper functions (variable extraction, tag scanning, span detection) are built and unit-tested first, then wired into the rewritten adapter.

**Tech Stack:** Rust, `serde_json`, `regex` (inline `Regex::new(...).unwrap()`, the project idiom), `crate::state::{VarDecl, VarType}`.

## Global Constraints

- Code comments and commit messages in **English**; end every commit with `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- No new dependencies (`regex`, `uuid`, `serde_json` already present).
- Spec: `docs/superpowers/specs/2026-06-21-st-preset-import-fidelity-v2-design.md`. Markers behavior unchanged from v1 (first char/world marker → one Content mount; `chatHistory` → History; append History if none).
- Active order = group `character_id == 100000`. "Authored" = `marker != true`.
- Variable type via `value.parse::<f64>()` → Number, else `parse::<bool>()` → Bool, else String. No hand-rolled numeric regex.
- Folder/wrap tags must be valid XML names (Task 1).

---

## File Structure

- `shirita-core/src/assembly.rs` — **modify** `sanitize_tag` (XML-name-start guard) + add one test.
- `shirita-core/src/adapters/stpreset.rs` — **modify**: add pure helpers (Tasks 2–3), rewrite `stpreset_to_loreset` (Task 4), update/add unit tests.
- `shirita-web/tests/import_preset_v2_test.rs` — **create** (Task 5): integration test against the real example file.

---

## Task 1: `sanitize_tag` guarantees a valid XML name start

**Files:**
- Modify: `shirita-core/src/assembly.rs` (`sanitize_tag`, ~line 372)

**Interfaces:**
- Produces: `pub fn sanitize_tag(name: &str) -> String` — same signature; now a non-empty result whose first char is not `is_alphabetic()`/`_` is prefixed with `tag_`. Empty stays empty (callers fall back).

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `assembly.rs` (next to the existing `sanitize_tag_*` tests):

```rust
    #[test]
    fn sanitize_tag_prefixes_invalid_xml_start() {
        assert_eq!(sanitize_tag("123 核心"), "tag_123_核心"); // digit start -> prefixed
        assert_eq!(sanitize_tag("Alice Smith"), "Alice_Smith"); // letter start -> unchanged
        assert_eq!(sanitize_tag("主角·凛"), "主角·凛"); // CJK letter start -> unchanged
        assert_eq!(sanitize_tag("<>&\"'/"), ""); // all stripped -> empty (caller falls back)
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shirita-core sanitize_tag_prefixes_invalid_xml_start`
Expected: FAIL — `assertion failed: left: "123_核心", right: "tag_123_核心"`.

- [ ] **Step 3: Add the guard**

In `sanitize_tag`, replace the final `out` return:

```rust
    out
```

with:

```rust
    // Guarantee a valid XML name start: a non-empty name that starts with a
    // digit/punctuation is prefixed; empty stays empty (callers fall back to def_type).
    match out.chars().next() {
        Some(c) if c.is_alphabetic() || c == '_' => out,
        Some(_) => format!("tag_{out}"),
        None => out,
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p shirita-core sanitize_tag`
Expected: PASS — the new test plus the existing `sanitize_tag_folds_spaces_and_strips_fatal` / `sanitize_tag_empty_when_all_stripped`.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/assembly.rs
git commit -m "$(cat <<'EOF'
feat(core): sanitize_tag guarantees a valid XML name start

Prefix tag_ when the sanitized name starts with a digit/punctuation so
wrap_in_tag and folder tags never emit invalid markup like <123…>. Empty
results are unchanged (callers fall back to def_type).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: variable extraction helpers (`extract_variables`, `infer_var`)

**Files:**
- Modify: `shirita-core/src/adapters/stpreset.rs`

**Interfaces:**
- Consumes: `crate::state::{VarDecl, VarType}`.
- Produces:
  - `fn extract_variables(content: &str) -> (String, Vec<VarDecl>)` — strips `{{setvar::name::value}}` (returns its `VarDecl`s in first-seen order, **no** dedup — caller dedups globally), rewrites `{{getvar::name}}` → `{{name}}`, leaves other `{{…}}` literal.
  - `fn infer_var(value: &str) -> (VarType, serde_json::Value)`.

- [ ] **Step 1: Write the failing tests**

Add to `stpreset.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn extract_variables_handles_setvar_getvar_and_literals() {
        let (clean, decls) = extract_variables(
            "a{{setvar::hp::100}}b{{setvar::ok::true}}c{{setvar::note:: }}{{getvar::hp}}{{trim}}{{// c}}",
        );
        // setvar macros stripped; getvar rewritten; other macros kept literal
        assert_eq!(clean, "abc{{hp}}{{trim}}{{// c}}");
        assert_eq!(decls.len(), 3);
        let by = |n: &str| decls.iter().find(|d| d.name == n).unwrap();
        assert_eq!(by("hp").var_type, VarType::Number);
        assert_eq!(by("hp").initial, serde_json::json!(100.0));
        assert_eq!(by("ok").var_type, VarType::Bool);
        assert_eq!(by("ok").initial, serde_json::json!(true));
        assert_eq!(by("note").var_type, VarType::String);
        assert_eq!(by("note").initial, serde_json::json!(" "));
    }

    #[test]
    fn extract_variables_all_setvar_yields_empty_content() {
        let (clean, decls) = extract_variables("{{setvar::a:: }}{{setvar::b::继续}}");
        assert!(clean.trim().is_empty());
        assert_eq!(decls.len(), 2);
    }

    #[test]
    fn extract_variables_keeps_unterminated_and_malformed_literal() {
        assert_eq!(extract_variables("x{{setvar::y").0, "x{{setvar::y");
        assert_eq!(extract_variables("x{{setvar::nosep}}y").0, "x{{setvar::nosep}}y");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p shirita-core extract_variables`
Expected: FAIL — `cannot find function extract_variables`.

- [ ] **Step 3: Implement the helpers**

Add near the top of `stpreset.rs` (after the `use` lines add `use crate::state::{VarDecl, VarType};`), above `stpreset_to_loreset`:

```rust
/// Type-infer an ST setvar value: f64-parseable -> Number, bool-parseable ->
/// Bool, else String. Uses the standard parses (no hand-rolled numeric regex).
fn infer_var(value: &str) -> (VarType, serde_json::Value) {
    if let Ok(n) = value.parse::<f64>() {
        (VarType::Number, serde_json::json!(n))
    } else if let Ok(b) = value.parse::<bool>() {
        (VarType::Bool, serde_json::json!(b))
    } else {
        (VarType::String, serde_json::json!(value))
    }
}

/// Strip `{{setvar::name::value}}` macros (collecting them as VarDecls in
/// first-seen order) and rewrite `{{getvar::name}}` -> `{{name}}`. Any other
/// `{{...}}` (e.g. `{{trim}}`, `{{// comment}}`, `{{user}}`) is kept literal.
/// Linear scan; nested macros inside a value are not parsed (first `}}` wins).
fn extract_variables(content: &str) -> (String, Vec<VarDecl>) {
    let mut out = String::with_capacity(content.len());
    let mut decls: Vec<VarDecl> = Vec::new();
    let mut rest = content;
    while let Some(pos) = rest.find("{{") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[pos..]); // unterminated -> keep literal, stop
            return (out, decls);
        };
        let inner = &after[..end];
        let literal = &rest[pos..pos + 2 + end + 2]; // the whole {{...}}
        if let Some(body) = inner.strip_prefix("setvar::") {
            if let Some((name, value)) = body.split_once("::") {
                let name = name.trim();
                if !name.is_empty() {
                    let (var_type, initial) = infer_var(value);
                    decls.push(VarDecl { name: name.to_string(), var_type, initial, scope: None });
                }
                // strip entirely (emit nothing)
            } else {
                out.push_str(literal); // malformed setvar -> keep literal
            }
        } else if let Some(name) = inner.strip_prefix("getvar::") {
            out.push_str("{{");
            out.push_str(name.trim());
            out.push_str("}}");
        } else {
            out.push_str(literal); // not a var macro -> keep literal
        }
        rest = &rest[pos + 2 + end + 2..];
    }
    out.push_str(rest);
    (out, decls)
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p shirita-core extract_variables`
Expected: PASS — all three tests.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/adapters/stpreset.rs
git commit -m "$(cat <<'EOF'
feat(core): stpreset variable extraction (setvar/getvar -> VarDecl)

extract_variables strips {{setvar::n::v}} into VarDecls and rewrites
{{getvar::n}} to {{n}}; other macros kept literal. Type via parse::<f64>/bool.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: tag-scanning + span helpers

**Files:**
- Modify: `shirita-core/src/adapters/stpreset.rs`

**Interfaces:**
- Produces:
  - `fn scan_tags(s: &str) -> Vec<(bool, String)>` — `(is_close, name)` per XML-ish tag; attributes ignored; self-closing skipped.
  - `fn tag_balance(s: &str) -> std::collections::HashMap<String, i32>` — net open count per tag (0 entries removed).
  - `fn is_balanced(s: &str) -> bool` — `tag_balance` empty.
  - `fn find_first_span(contents: &[String]) -> Option<(usize, usize, String)>` — first `(opener_idx, closer_idx, tag)` in a contiguous slice.
  - `fn strip_open_tag(content: &str, tag: &str) -> String` / `fn strip_close_tag(content: &str, tag: &str) -> String` — remove the first `<tag …>` / `</tag>` and trim.

- [ ] **Step 1: Write the failing tests**

Add to `stpreset.rs` tests:

```rust
    #[test]
    fn scan_tags_ignores_attributes_and_self_closing() {
        assert_eq!(
            scan_tags("<Rule depth=\"0\">x</Rule><br/><最新互动>"),
            vec![(false, "Rule".to_string()), (true, "Rule".to_string()), (false, "最新互动".to_string())]
        );
        assert!(is_balanced("<a>x</a> plain"));
        assert!(!is_balanced("<a>x"));
        assert!(is_balanced("no tags here"));
    }

    #[test]
    fn find_first_span_pairs_open_and_close() {
        let c = vec!["<rules>foo".to_string(), "mid".to_string(), "bar</rules>".to_string()];
        assert_eq!(find_first_span(&c), Some((0, 2, "rules".to_string())));
        let none = vec!["<x>unclosed".to_string(), "plain".to_string()];
        assert_eq!(find_first_span(&none), None);
    }

    #[test]
    fn strip_tag_helpers_remove_first_occurrence() {
        assert_eq!(strip_open_tag("<Rule depth=\"0\">body", "Rule"), "body");
        assert_eq!(strip_close_tag("body</rules>", "rules"), "body");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p shirita-core -- scan_tags find_first_span strip_tag`
Expected: FAIL — functions not found.

- [ ] **Step 3: Implement the helpers**

Add to `stpreset.rs` (above `stpreset_to_loreset`):

```rust
/// (is_close, name) for each XML-ish tag. The name is the first token after
/// `<`/`</`; attributes are ignored; self-closing `<x/>` is skipped.
fn scan_tags(s: &str) -> Vec<(bool, String)> {
    let re = regex::Regex::new(r"<\s*(/?)\s*([^\s<>/]+)[^<>]*?(/?)\s*>").unwrap();
    re.captures_iter(s)
        .filter(|c| &c[3] != "/") // skip self-closing
        .map(|c| (&c[1] == "/", c[2].to_string()))
        .collect()
}

/// Net open count per tag name (open +1, close -1); balanced entries removed.
fn tag_balance(s: &str) -> std::collections::HashMap<String, i32> {
    let mut bal: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    for (is_close, name) in scan_tags(s) {
        *bal.entry(name).or_insert(0) += if is_close { -1 } else { 1 };
    }
    bal.retain(|_, v| *v != 0);
    bal
}

fn is_balanced(s: &str) -> bool {
    tag_balance(s).is_empty()
}

/// First cross-node span in a contiguous slice: the earliest element that
/// leaves some tag T net-open, paired with the earliest later element that
/// leaves T net-closed. Returns (opener_idx, closer_idx, tag).
fn find_first_span(contents: &[String]) -> Option<(usize, usize, String)> {
    for (i, c) in contents.iter().enumerate() {
        let Some(tag) = tag_balance(c).into_iter().find(|(_, v)| *v > 0).map(|(t, _)| t) else {
            continue;
        };
        for (j, c2) in contents.iter().enumerate().skip(i + 1) {
            if tag_balance(c2).get(&tag).copied().unwrap_or(0) < 0 {
                return Some((i, j, tag));
            }
        }
    }
    None
}

/// Remove the first `<tag …>` (any attributes) and trim.
fn strip_open_tag(content: &str, tag: &str) -> String {
    let re = regex::Regex::new(&format!(r"<\s*{}(?:\s[^<>]*)?\s*>", regex::escape(tag))).unwrap();
    re.replacen(content, 1, "").trim().to_string()
}

/// Remove the first `</tag>` and trim.
fn strip_close_tag(content: &str, tag: &str) -> String {
    let re = regex::Regex::new(&format!(r"</\s*{}\s*>", regex::escape(tag))).unwrap();
    re.replacen(content, 1, "").trim().to_string()
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p shirita-core -- scan_tags find_first_span strip_tag`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/adapters/stpreset.rs
git commit -m "$(cat <<'EOF'
feat(core): stpreset tag-scan + cross-node span helpers

scan_tags/tag_balance/is_balanced (attribute- and self-closing-aware) and
find_first_span + strip_open_tag/strip_close_tag for bundling spans into folders.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: rewrite `stpreset_to_loreset` (wire A + B + C)

**Files:**
- Modify: `shirita-core/src/adapters/stpreset.rs`

**Interfaces:**
- Consumes: helpers from Tasks 2–3; `Definition`, `PromptNode` (`new_folder`/`new_ref`), `NodeKind`, `OwnerKind`, `Template`, `LoreSet`, `crate::assembly::sanitize_tag`.
- Produces: same `pub fn stpreset_to_loreset(preset: &serde_json::Value, name: &str) -> LoreSet`, now per the v2 spec.

- [ ] **Step 1: Update the two existing tests whose behavior changed**

In `stpreset.rs` tests, **replace** `skips_disabled_entries_and_unknown_identifiers` with:

```rust
    #[test]
    fn imports_disabled_in_order_as_disabled_ref_skips_unknown() {
        let preset = json!({
            "prompts": [
                { "identifier": "main", "name": "Main", "content": "hi" },
                { "identifier": "off", "name": "Off", "content": "no" }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "main", "enabled": true },
                { "identifier": "off", "enabled": false },
                { "identifier": "ghost", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        // both authored prompts imported; ghost (unknown id) skipped
        assert_eq!(prompt_defs(&ls).len(), 2);
        let def_of = |nm: &str| ls.definitions.iter().find(|d| d.name == nm).unwrap();
        let ref_of = |nm: &str| {
            let id = &def_of(nm).id;
            ls.nodes.iter().find(|n| n.definition_id.as_deref() == Some(id.as_str())).unwrap()
        };
        assert!(ref_of("Main").enabled);
        assert!(!ref_of("Off").enabled, "disabled-in-order imported as a disabled Ref");
    }
```

And **replace** `reads_only_default_group_100000` with:

```rust
    #[test]
    fn reads_default_group_others_go_inactive() {
        let preset = json!({
            "prompts": [
                { "identifier": "main", "name": "Main", "content": "default" },
                { "identifier": "other", "name": "Other", "content": "char-specific" }
            ],
            "prompt_order": [
                { "character_id": 100001, "order": [ { "identifier": "other", "enabled": true } ] },
                { "character_id": 100000, "order": [ { "identifier": "main", "enabled": true } ] }
            ]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        // "main" is the active (enabled, root) prompt
        let main = ls.definitions.iter().find(|d| d.name == "Main").unwrap();
        let main_ref = ls.nodes.iter().find(|n| n.definition_id.as_deref() == Some(main.id.as_str())).unwrap();
        assert!(main_ref.enabled && main_ref.parent_id.is_none());
        // "other" (not in group 100000) -> disabled Ref under the inactive folder
        let inactive = ls.nodes.iter().find(|n| n.tag.as_deref() == Some("inactive")).expect("inactive folder");
        assert!(!inactive.enabled);
        let other = ls.definitions.iter().find(|d| d.name == "Other").unwrap();
        let other_ref = ls.nodes.iter().find(|n| n.definition_id.as_deref() == Some(other.id.as_str())).unwrap();
        assert_eq!(other_ref.parent_id.as_deref(), Some(inactive.id.as_str()));
        assert!(!other_ref.enabled);
    }
```

- [ ] **Step 2: Add the new behavior tests**

Append to `stpreset.rs` tests:

```rust
    #[test]
    fn setvar_registers_variables_and_emits_no_node_when_emptied() {
        let preset = json!({
            "prompts": [
                { "identifier": "vars", "name": "Vars", "content": "{{setvar::hp::100}}{{setvar::tone::soft}}" },
                { "identifier": "main", "name": "Main", "content": "use {{getvar::hp}}" }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "vars", "enabled": true },
                { "identifier": "main", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        // "vars" emptied by stripping -> no def; its variables registered on the template
        assert!(ls.definitions.iter().all(|d| d.name != "Vars"));
        let vars = ls.template.meta["variables"].as_array().unwrap();
        assert!(vars.iter().any(|v| v["name"] == "hp" && v["type"] == "number"));
        assert!(vars.iter().any(|v| v["name"] == "tone" && v["type"] == "string"));
        // getvar rewritten to {{hp}} in main's content
        let main = ls.definitions.iter().find(|d| d.name == "Main").unwrap();
        assert_eq!(main.content, "use {{hp}}");
    }

    #[test]
    fn balanced_prompt_wraps_stray_tag_stays_raw() {
        let preset = json!({
            "prompts": [
                { "identifier": "a", "name": "Clean", "content": "just text" },
                { "identifier": "b", "name": "Stray", "content": "see <最新互动> here" }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "a", "enabled": true },
                { "identifier": "b", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        let wrap = |nm: &str| {
            ls.definitions.iter().find(|d| d.name == nm).unwrap()
                .meta.get("wrap_in_tag").and_then(|v| v.as_bool()).unwrap_or(false)
        };
        assert!(wrap("Clean"), "balanced/no-tag prompt is wrapped");
        assert!(!wrap("Stray"), "stray unclosed tag -> left raw");
    }

    #[test]
    fn cross_node_span_becomes_a_folder() {
        let preset = json!({
            "prompts": [
                { "identifier": "open", "name": "Open", "content": "<Rule depth=\"0\">first" },
                { "identifier": "close", "name": "Close", "content": "second</Rule>" }
            ],
            "prompt_order": [ { "character_id": 100000, "order": [
                { "identifier": "open", "enabled": true },
                { "identifier": "close", "enabled": true }
            ]}]
        });
        let ls = stpreset_to_loreset(&preset, "P");
        let folder = ls.nodes.iter().find(|n| n.kind == NodeKind::Folder && n.tag.as_deref() == Some("Rule")).expect("span folder");
        // both prompts are children of the folder, literal tags stripped, not individually wrapped
        let children: Vec<_> = ls.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(folder.id.as_str())).collect();
        assert_eq!(children.len(), 2);
        let open = ls.definitions.iter().find(|d| d.name == "Open").unwrap();
        let close = ls.definitions.iter().find(|d| d.name == "Close").unwrap();
        assert_eq!(open.content, "first");
        assert_eq!(close.content, "second");
        assert!(open.meta.get("wrap_in_tag").is_none(), "span children not individually wrapped");
    }
```

- [ ] **Step 3: Run to verify the updated/new tests fail**

Run: `cargo test -p shirita-core --lib stpreset`
Expected: FAIL — the new/updated tests reference v2 behavior the old function doesn't produce (e.g. no `inactive` folder, no `wrap_in_tag`, no span folder, variables not registered).

- [ ] **Step 4: Rewrite `stpreset_to_loreset`**

Replace the entire `pub fn stpreset_to_loreset(...) { ... }` body (lines 15–120) with:

```rust
pub fn stpreset_to_loreset(preset: &serde_json::Value, name: &str) -> LoreSet {
    use std::collections::{HashMap, HashSet};

    let tname = if name.trim().is_empty() {
        format!("Imported preset ({})", &uuid::Uuid::new_v4().to_string()[..4])
    } else {
        name.trim().to_string()
    };
    let mut tmpl = Template::new(tname);
    let mut defs: Vec<Definition> = Vec::new();
    let mut nodes: Vec<PromptNode> = Vec::new();

    // Index prompts by identifier, and keep their array order for the library tail.
    let prompts: HashMap<String, &serde_json::Value> = preset
        .get("prompts")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| p.get("identifier").and_then(|i| i.as_str()).map(|id| (id.to_string(), p)))
                .collect()
        })
        .unwrap_or_default();
    let all_ids: Vec<String> = preset
        .get("prompts")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|p| p.get("identifier").and_then(|i| i.as_str()).map(String::from)).collect())
        .unwrap_or_default();

    // Active order (group 100000): (identifier, enabled) in list order.
    let order: Vec<(String, bool)> = preset
        .get("prompt_order")
        .and_then(|v| v.as_array())
        .and_then(|gs| gs.iter().find(|g| g.get("character_id").and_then(|c| c.as_i64()) == Some(100000)))
        .and_then(|g| g.get("order"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let id = e.get("identifier")?.as_str()?.to_string();
                    let en = e.get("enabled").and_then(|b| b.as_bool()).unwrap_or(false);
                    Some((id, en))
                })
                .collect()
        })
        .unwrap_or_default();
    let order_ids: HashSet<&str> = order.iter().map(|(id, _)| id.as_str()).collect();

    let is_marker = |id: &str| {
        prompts.get(id).and_then(|p| p.get("marker")).and_then(|m| m.as_bool()) == Some(true)
    };
    let name_of = |id: &str| {
        prompts
            .get(id)
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(id)
            .to_string()
    };

    // --- Part C: variables (global, first-wins) + cleaned-content cache. ---
    // Walk active order first, then library ids (array order), so order values win.
    let lib_ids: Vec<String> =
        all_ids.iter().filter(|id| !order_ids.contains(id.as_str())).cloned().collect();
    let mut vars: Vec<VarDecl> = Vec::new();
    let mut cleaned: HashMap<String, String> = HashMap::new();
    for id in order.iter().map(|(id, _)| id.clone()).chain(lib_ids.iter().cloned()) {
        if cleaned.contains_key(&id) || !prompts.contains_key(&id) || is_marker(&id) {
            continue;
        }
        let raw = prompts.get(&id).and_then(|p| p.get("content")).and_then(|v| v.as_str()).unwrap_or("");
        let (clean, decls) = extract_variables(raw);
        for d in decls {
            if !vars.iter().any(|e| e.name == d.name) {
                vars.push(d);
            }
        }
        cleaned.insert(id, clean);
    }
    if !vars.is_empty() {
        tmpl.meta = serde_json::json!({ "variables": vars });
    }

    // --- Phase 1: build ordered elements for the active order. ---
    struct Authored {
        name: String,
        content: String,
        enabled: bool,
    }
    enum Elem {
        Content,
        History,
        Prompt(usize),
    }
    let mut authored: Vec<Authored> = Vec::new();
    let mut elems: Vec<Elem> = Vec::new();
    let mut emitted_content = false;
    let mut has_history = false;
    for (id, enabled) in &order {
        if !prompts.contains_key(id) {
            tracing::warn!(identifier = %id, "st preset import: identifier missing from prompts, skipping");
            continue;
        }
        if is_marker(id) {
            if id == "chatHistory" {
                elems.push(Elem::History);
                has_history = true;
            } else if !emitted_content {
                elems.push(Elem::Content);
                emitted_content = true;
            }
            continue;
        }
        let content = cleaned.get(id).cloned().unwrap_or_default();
        if content.trim().is_empty() {
            continue; // originally empty or emptied by setvar-stripping
        }
        authored.push(Authored { name: name_of(id), content, enabled: *enabled });
        elems.push(Elem::Prompt(authored.len() - 1));
    }

    // --- Phase 2: detect cross-node spans within maximal runs of consecutive
    //     ENABLED authored elements (a marker or disabled prompt breaks a run). ---
    let mut spans: Vec<(usize, usize, String)> = Vec::new(); // (start_elem, end_elem, tag)
    let mut e = 0;
    while e < elems.len() {
        let run_enabled = matches!(elems[e], Elem::Prompt(k) if authored[k].enabled);
        if !run_enabled {
            e += 1;
            continue;
        }
        let start = e;
        while e + 1 < elems.len() && matches!(elems[e + 1], Elem::Prompt(k) if authored[k].enabled) {
            e += 1;
        }
        let end = e; // run is elems[start..=end]
        let contents: Vec<String> = (start..=end)
            .map(|x| match elems[x] {
                Elem::Prompt(k) => authored[k].content.clone(),
                _ => unreachable!(),
            })
            .collect();
        if let Some((ri, rj, tag)) = find_first_span(&contents) {
            spans.push((start + ri, start + rj, tag));
        }
        e += 1;
    }

    // --- Phase 3: emit nodes from elements, applying spans + wrap_in_tag. ---
    let folder_tag = |tag: &str| {
        let s = crate::assembly::sanitize_tag(tag);
        if s.is_empty() {
            "prompt".to_string()
        } else {
            s
        }
    };
    let mut root_sort: i64 = 0;
    let mut x = 0;
    while x < elems.len() {
        if let Some((s, t, tag)) = spans.iter().find(|(s, _, _)| *s == x).cloned() {
            let folder = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, root_sort, folder_tag(&tag));
            root_sort += 1;
            let fid = folder.id.clone();
            nodes.push(folder);
            let mut child_sort: i64 = 0;
            for ei in s..=t {
                let k = match elems[ei] {
                    Elem::Prompt(k) => k,
                    _ => unreachable!(),
                };
                let mut content = authored[k].content.clone();
                if ei == s {
                    content = strip_open_tag(&content, &tag);
                }
                if ei == t {
                    content = strip_close_tag(&content, &tag);
                }
                let d = Definition::new("prompt", &authored[k].name, content);
                // children are not individually wrapped — the folder emits <tag>…</tag>
                nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, Some(fid.clone()), child_sort, &d.id));
                child_sort += 1;
                defs.push(d);
            }
            x = t + 1;
            continue;
        }
        match &elems[x] {
            Elem::Content => {
                let mut c = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, root_sort, "content");
                c.kind = NodeKind::Content;
                c.tag = None;
                nodes.push(c);
                root_sort += 1;
            }
            Elem::History => {
                let mut h = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, root_sort, "history");
                h.kind = NodeKind::History;
                h.tag = None;
                nodes.push(h);
                root_sort += 1;
            }
            Elem::Prompt(k) => {
                let a = &authored[*k];
                let mut d = Definition::new("prompt", &a.name, &a.content);
                if is_balanced(&a.content) {
                    d.meta = serde_json::json!({ "wrap_in_tag": true });
                }
                let mut r = PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, root_sort, &d.id);
                r.enabled = a.enabled;
                nodes.push(r);
                defs.push(d);
                root_sort += 1;
            }
        }
        x += 1;
    }

    // A template needs a history mount.
    if !has_history {
        let mut hist = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, root_sort, "history");
        hist.kind = NodeKind::History;
        hist.tag = None;
        nodes.push(hist);
        root_sort += 1;
    }

    // --- Part A tail: not-in-order authored prompts -> one disabled `inactive` folder. ---
    let inactive: Vec<(String, String)> = lib_ids
        .iter()
        .filter(|id| !is_marker(id))
        .filter_map(|id| {
            let c = cleaned.get(id).cloned().unwrap_or_default();
            if c.trim().is_empty() {
                None
            } else {
                Some((name_of(id), c))
            }
        })
        .collect();
    if !inactive.is_empty() {
        let mut folder = PromptNode::new_folder(OwnerKind::Template, &tmpl.id, None, root_sort, "inactive");
        folder.enabled = false;
        let fid = folder.id.clone();
        nodes.push(folder);
        let mut cs: i64 = 0;
        for (nm, content) in inactive {
            let mut d = Definition::new("prompt", &nm, &content);
            if is_balanced(&content) {
                d.meta = serde_json::json!({ "wrap_in_tag": true });
            }
            let mut r = PromptNode::new_ref(OwnerKind::Template, &tmpl.id, Some(fid.clone()), cs, &d.id);
            r.enabled = false;
            cs += 1;
            nodes.push(r);
            defs.push(d);
        }
    }

    LoreSet { template: tmpl, definitions: defs, nodes }
}
```

- [ ] **Step 5: Run to verify all stpreset unit tests pass**

Run: `cargo test -p shirita-core --lib stpreset`
Expected: PASS — the updated, new, and unchanged-v1 tests (`maps_authored_prompts_and_history_in_order`, `first_marker_becomes_one_content_node`, `appends_history_when_enabled_order_has_none`, `skips_authored_prompt_with_empty_content`, `empty_name_yields_unique_fallback`) all green.

- [ ] **Step 6: Confirm no core regressions**

Run: `cargo test -p shirita-core`
Expected: PASS — including the `assembly` tests (Task 1's `sanitize_tag` change).

- [ ] **Step 7: Commit**

```bash
git add shirita-core/src/adapters/stpreset.rs
git commit -m "$(cat <<'EOF'
feat(core): preset import fidelity v2 — status import, tags, variables

Rewrite stpreset_to_loreset: import every authored prompt by enabled/disabled
status (not-in-order ones under one disabled `inactive` folder), auto
wrap_in_tag for balanced sections, bundle cross-node XML spans into folders,
and register setvar/getvar as template variables (emptied prompts become
variables only, no node). Markers/history unchanged.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: web integration test against the real example

**Files:**
- Create: `shirita-web/tests/import_preset_v2_test.rs`

**Interfaces:**
- Consumes: `stpreset_to_loreset` via the `/api/import` route (unchanged `persist_preset`).

- [ ] **Step 1: Write the failing test**

Create `shirita-web/tests/import_preset_v2_test.rs`:

```rust
//! POST /api/import — fidelity v2 against the real example preset.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use shirita_core::{
    Config, EchoProvider, ModelProvider, SqliteStorage, Storage, TiktokenCounter, TokenCounter,
};
use shirita_web::{app, AppState};

async fn test_state(dir: &std::path::Path) -> AppState {
    let storage = SqliteStorage::connect(dir.join("p.db").to_str().unwrap()).await.unwrap();
    storage.run_migrations().await.unwrap();
    let storage: Arc<dyn Storage> = Arc::new(storage);
    let config = Arc::new(Config::new("ignored", dir.join("assets").to_str().unwrap(), "secret-token").unwrap());
    let provider: Arc<dyn ModelProvider> = Arc::new(EchoProvider);
    let token_counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
    AppState {
        storage,
        config,
        provider,
        token_counter,
        model: "m".into(),
        generations: Arc::new(shirita_web::Generations::new()),
        http_client: shirita_web::new_http_client(),
    }
}

async fn import_named(state: &AppState, filename: &str, data: &[u8]) -> (StatusCode, Value) {
    let boundary = "BND";
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method("POST")
        .uri("/api/import")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let res = app(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn imports_real_preset_with_variables_and_inactive_folder() {
    let dir = tempfile::tempdir().unwrap();
    let state = test_state(dir.path()).await;
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/示例预设.json")).unwrap();
    let (st, summary) = import_named(&state, "示例预设.json", &data).await;
    assert_eq!(st, StatusCode::OK);
    let tmpl_id = summary["created"].as_array().unwrap().iter()
        .find(|c| c["kind"] == "template").expect("template created")["id"].as_str().unwrap().to_string();

    // Variables from the jailbreak setvar block landed on the template.
    let tmpl = state.storage.get_template(&tmpl_id).await.unwrap().unwrap();
    let vars = tmpl.meta["variables"].as_array().expect("variables registered");
    let has = |n: &str| vars.iter().any(|v| v["name"] == n);
    assert!(has("wordsCloud") && has("JailbreakPrompt"), "setvar variables registered");

    // The all-setvar jailbreak prompt produced no def (its display name is "🛡️ 变量（别动）").
    let defs = state.storage.list_definitions().await.unwrap();
    assert!(defs.iter().all(|d| d.name != "🛡️ 变量（别动）"), "emptied prompt yields no def");

    // An inactive folder exists for the out-of-order library prompts.
    let nodes = state.storage.list_nodes(&shirita_core::OwnerKind::Template, &tmpl_id).await.unwrap();
    assert!(nodes.iter().any(|n| n.tag.as_deref() == Some("inactive") && !n.enabled), "inactive folder");
}
```

- [ ] **Step 2: Run to verify it fails (then passes after Task 4 is in)**

Run: `cargo test -p shirita-web --test import_preset_v2_test`
Expected after Tasks 1–4: PASS. (If run before Task 4, it fails — no variables/inactive folder.)

Both accessors are confirmed on the `Storage` trait: `get_template(&self, id: &str) -> Result<Option<Template>>` (`storage/mod.rs:57`) and `list_nodes(&self, owner_kind: &OwnerKind, owner_id: &str)` (`storage/mod.rs:68`).

- [ ] **Step 3: Run the v1 preset tests too (no regression in persist_preset)**

Run: `cargo test -p shirita-web --test import_preset_test`
Expected: PASS — the v1 collision-independence / empty-order-400 tests still hold.

- [ ] **Step 4: Commit**

```bash
git add shirita-web/tests/import_preset_v2_test.rs
git commit -m "$(cat <<'EOF'
test(web): preset import v2 integration against the real example

Asserts setvar variables land on the template, the all-setvar jailbreak prompt
yields no def, and an inactive folder holds the out-of-order library prompts.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review notes

- **Spec coverage:** Part A (status import + `inactive` folder) → Task 4 Phase 1/3 + tail, tests in Steps 1–2. Part B (`wrap_in_tag` balanced-only + cross-node span folders + valid tags) → Tasks 1, 3, 4. Part C (`setvar`/`getvar` → variables, emptied→no node) → Task 2 + Task 4 Part C. Markers/history unchanged → preserved v1 tests. Real-file behavior → Task 5.
- **Placeholder scan:** none — every code/test block is complete.
- **Type consistency:** helper names (`extract_variables`, `infer_var`, `scan_tags`, `tag_balance`, `is_balanced`, `find_first_span`, `strip_open_tag`, `strip_close_tag`) and signatures match between their defining task and Task 4's usage. `sanitize_tag` reused via `crate::assembly::sanitize_tag`.
- **Verification note (Task 5):** `get_template` / `list_nodes` accessor names are flagged to confirm against `Storage` before relying on them.

## Out of scope

Samplers; `injection_position`/depth; per-prompt roles; macros beyond `setvar`/`getvar`; nested/interleaved/mid-content cross-node tags; alternate `prompt_order` groups. (Per spec §5.)
