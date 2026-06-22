# ST Status-Bar → Native Panel Conversion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a SillyTavern character card's status bar follows the common single-regex/`$N`-template pattern, generate an equivalent native `pack.meta.panel` at import time (alongside the existing, unmodified regex_rule/`HtmlCardFrame` compatibility layer) and keep its variables updated every turn.

**Architecture:** Detection + conversion happens once, at import time, in `shirita-core/src/adapters/charcard.rs` (pure functions, no I/O). A new `capture_vars` field on the triggering `regex_rule` Definition's `meta` records which capture group maps to which variable name. At generation time, `shirita-core/src/assembly.rs` gets a new pure function that re-runs that same pattern (read-only, via `.captures()`) against each AI turn's raw text and turns matched groups into `Update`s, which `shirita-core/src/conversation.rs` merges into the existing `apply_updates` call already used for `<state_update>` tags. No new tables, no new endpoints, no new frontend components — `PackEditor.vue`/`PanelView.vue` already render whatever is in `pack.meta.panel`.

**Tech Stack:** Rust (`shirita-core`, `shirita-web`), `fancy_regex` (lookaround/backreference-capable regex engine already a workspace dependency), `regex` (already a workspace dependency, used for the simple non-backreference tag/placeholder scans), Vue 3 + `vue-i18n` (`shirita-ui`).

## Global Constraints

- Source spec: `docs/superpowers/specs/2026-06-22-st-status-panel-conversion-design.md` (read it before starting — this plan implements it section by section).
- Only convert when **exactly one** candidate `regex_scripts` entry qualifies (§3, "Decision rule"). Zero or ≥2 candidates ⇒ skip conversion entirely, no panic, no partial panel.
- A `$N` token is only a capture reference when `1 <= N <= group_count` (the compiled `findRegex`'s actual capture-group count via `captures_len() - 1`). Out-of-range `$N` is left as literal text, never generates a variable, never counts toward candidate detection.
- `<style>`/`<script>` extraction must use case-insensitive, dot-matches-newline regex flags (`(?is)`).
- Generated variable names are exactly `field{N}` (the original 1-based capture-group number, no renumbering) — never invent semantic names.
- All generated variables are `VarType::String`.
- The compatibility layer (`regex_rule.meta.pattern`/`.replacement`, `assembly::apply_regex_rules_for`, `HtmlCardFrame.vue`) must keep working byte-for-byte identically whether or not conversion happens — this feature only *adds* a `capture_vars` key and a `panel` meta entry, never removes or rewrites existing fields.
- Write code comments and commit messages in English.

**Implementation note vs. the spec:** the spec's §3 mentions skipping conversion "if a pack already has a manually-authored panel" on re-import. Tracing the actual import code path (`shirita-web/src/routes/import_export.rs::persist_loreset_as_pack`) shows every charcard import always builds a brand-new `Pack` via `loreset_to_pack` (fresh `Pack::new(...)`, default empty `meta`) — there is no code path where `charcard_to_loreset`'s output pack already has a panel. That guard is therefore not implemented (nothing to guard against); this note exists so a reviewer comparing this plan to the spec doesn't think it was missed.

---

## Task 1: `$N` capture-reference validation and substitution helpers

**Files:**
- Modify: `shirita-core/src/assembly.rs` (change `normalize_js_regex_literal` visibility)
- Modify: `shirita-core/src/adapters/charcard.rs` (new private helpers + tests)

**Interfaces:**
- Consumes: `crate::assembly::normalize_js_regex_literal(pattern: &str) -> String` (already implemented in the JS-literal-regex bugfix; this task only changes its visibility).
- Produces (for Task 3): `fn dollar_refs_in(replace_string: &str, group_count: usize) -> Vec<usize>` and `fn substitute_dollar_refs(replace_string: &str, valid_ns: &[usize]) -> String`, both private to `charcard.rs`.

- [ ] **Step 1: Change `normalize_js_regex_literal` to crate-visible**

In `shirita-core/src/assembly.rs`, find:
```rust
fn normalize_js_regex_literal(pattern: &str) -> String {
```
Change to:
```rust
pub(crate) fn normalize_js_regex_literal(pattern: &str) -> String {
```
(Task 3 in `charcard.rs` needs to call this; it's currently private to `assembly.rs`.)

- [ ] **Step 2: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `shirita-core/src/adapters/charcard.rs` (near the top of the block, alongside the existing `ty` helper):

```rust
    #[test]
    fn dollar_refs_in_keeps_only_in_range_groups() {
        // pattern has 3 capture groups; $10 is not a reference (out of range),
        // $1 and $3 are.
        assert_eq!(dollar_refs_in("$1 $3 $10", 3), vec![1, 3]);
    }

    #[test]
    fn dollar_refs_in_dedupes_and_sorts() {
        assert_eq!(dollar_refs_in("$3 $1 $3", 3), vec![1, 3]);
    }

    #[test]
    fn dollar_refs_in_ignores_dollar_dollar_and_dollar_amp() {
        // `$$` and `$&` never match `\$(\d+)` — no digits follow the `$`.
        assert_eq!(dollar_refs_in("$$ $& $1", 1), vec![1]);
    }

    #[test]
    fn substitute_dollar_refs_replaces_only_valid_refs() {
        let out = substitute_dollar_refs("a:$1 b:$10 c:$3", &[1, 3]);
        assert_eq!(out, "a:{{field1}} b:$10 c:{{field3}}");
    }
```

- [ ] **Step 2b: Run tests to verify they fail**

Run: `cargo test -p shirita-core --lib dollar_refs`
Expected: compile error (`cannot find function dollar_refs_in`/`substitute_dollar_refs` in this scope) — that's the correct RED for code that doesn't exist yet.

- [ ] **Step 3: Implement the helpers**

Add above the `#[cfg(test)]` block in `shirita-core/src/adapters/charcard.rs` (after `tavern_helper_vardecls`):

```rust
/// Capture-group numbers (1-based) referenced by valid `$N` tokens in
/// `replace_string`, deduped and sorted ascending. A `$N` only counts when
/// `1 <= N <= group_count` — an out-of-range `$N` (e.g. `$10` against a
/// 3-group pattern) is not a capture reference at all and must be ignored
/// here (see `substitute_dollar_refs`, which leaves it untouched in the
/// output).
fn dollar_refs_in(replace_string: &str, group_count: usize) -> Vec<usize> {
    static DOLLAR_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\$(\d+)").unwrap());
    let mut ns: Vec<usize> = DOLLAR_RE
        .captures_iter(replace_string)
        .filter_map(|c| c[1].parse::<usize>().ok())
        .filter(|&n| n >= 1 && n <= group_count)
        .collect();
    ns.sort_unstable();
    ns.dedup();
    ns
}

/// Replace every `$N` in `replace_string` where `N` is in `valid_ns` with
/// Panel's `{{fieldN}}` interpolation syntax; any other `$N` (out of range,
/// already excluded from `valid_ns` by `dollar_refs_in`) is left exactly
/// as-is.
fn substitute_dollar_refs(replace_string: &str, valid_ns: &[usize]) -> String {
    static DOLLAR_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\$(\d+)").unwrap());
    DOLLAR_RE
        .replace_all(replace_string, |caps: &regex::Captures| {
            let n: usize = caps[1].parse().unwrap();
            if valid_ns.contains(&n) {
                let mut s = String::from("{{field");
                s.push_str(&n.to_string());
                s.push_str("}}");
                s
            } else {
                caps[0].to_string()
            }
        })
        .into_owned()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core --lib dollar_refs substitute_dollar_refs`
Expected: 4 tests pass (`dollar_refs_in_keeps_only_in_range_groups`, `dollar_refs_in_dedupes_and_sorts`, `dollar_refs_in_ignores_dollar_dollar_and_dollar_amp`, `substitute_dollar_refs_replaces_only_valid_refs`).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/assembly.rs shirita-core/src/adapters/charcard.rs
git commit -m "feat(charcard): add \$N capture-reference validation/substitution helpers"
```

---

## Task 2: `<style>`/`<script>` block extraction helper

**Files:**
- Modify: `shirita-core/src/adapters/charcard.rs`

**Interfaces:**
- Produces (for Task 3): `fn extract_tag_blocks(html: &str, tag: &str) -> (String, Vec<String>)` — returns `(html_with_blocks_removed, vec_of_each_blocks_inner_content)`.

- [ ] **Step 1: Write the failing tests**

Add to the same test module:

```rust
    #[test]
    fn extract_tag_blocks_removes_style_and_returns_contents() {
        let html = "<div>x</div><style>.a{color:red}</style><p>y</p>";
        let (remaining, blocks) = extract_tag_blocks(html, "style");
        assert_eq!(remaining, "<div>x</div><p>y</p>");
        assert_eq!(blocks, vec![".a{color:red}".to_string()]);
    }

    #[test]
    fn extract_tag_blocks_is_case_insensitive_and_dotall() {
        let html = "<Style>\n.a{color:red}\n</STYLE>";
        let (remaining, blocks) = extract_tag_blocks(html, "style");
        assert_eq!(remaining, "");
        assert_eq!(blocks, vec!["\n.a{color:red}\n".to_string()]);
    }

    #[test]
    fn extract_tag_blocks_handles_multiple_blocks() {
        let html = "<script>a()</script>mid<script>b()</script>";
        let (remaining, blocks) = extract_tag_blocks(html, "script");
        assert_eq!(remaining, "mid");
        assert_eq!(blocks, vec!["a()".to_string(), "b()".to_string()]);
    }

    #[test]
    fn extract_tag_blocks_no_match_returns_input_unchanged() {
        let html = "<div>no blocks here</div>";
        let (remaining, blocks) = extract_tag_blocks(html, "script");
        assert_eq!(remaining, html);
        assert!(blocks.is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shirita-core --lib extract_tag_blocks`
Expected: compile error, `cannot find function extract_tag_blocks`.

- [ ] **Step 3: Implement the helper**

Add next to the helpers from Task 1:

```rust
/// Remove every `<tag ...>...</tag>` block from `html` (case-insensitive,
/// content may span multiple lines), returning the html with those blocks
/// cut out plus each block's inner content in order of appearance. Used to
/// pull a card's `<style>` into `panel.css` and discard its `<script>`
/// (Panel forbids `<script>` at render time regardless — this just does it
/// explicitly, earlier).
fn extract_tag_blocks(html: &str, tag: &str) -> (String, Vec<String>) {
    let pattern = format!(r"(?is)<{tag}\b[^>]*>(.*?)</{tag}\s*>");
    let re = regex::Regex::new(&pattern).expect("tag is always a static literal (\"style\"/\"script\")");
    let mut blocks = Vec::new();
    let mut out = String::new();
    let mut last = 0;
    for caps in re.captures_iter(html) {
        let m = caps.get(0).unwrap();
        out.push_str(&html[last..m.start()]);
        blocks.push(caps.get(1).map(|g| g.as_str().to_string()).unwrap_or_default());
        last = m.end();
    }
    out.push_str(&html[last..]);
    (out, blocks)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core --lib extract_tag_blocks`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/adapters/charcard.rs
git commit -m "feat(charcard): add case-insensitive style/script block extraction"
```

---

## Task 3: `try_convert_status_panel` detection + conversion

**Files:**
- Modify: `shirita-core/src/adapters/charcard.rs`

**Interfaces:**
- Consumes: `dollar_refs_in`, `substitute_dollar_refs` (Task 1), `extract_tag_blocks` (Task 2), `crate::assembly::normalize_js_regex_literal` (Task 1 step 1), `crate::state::{VarDecl, VarType}` (already imported at the top of this file).
- Produces (for Task 4): 
  ```rust
  struct PanelConversion {
      source_index: usize,
      html: String,
      css: String,
      var_decls: Vec<VarDecl>,
      capture_vars: Vec<Option<String>>,
  }
  fn try_convert_status_panel(scripts: &[serde_json::Value]) -> Option<PanelConversion>
  ```
  `capture_vars[i]` corresponds to capture group `i+1`; `None` means that group has no associated variable (not referenced by a valid `$N`).

- [ ] **Step 1: Write the failing tests**

```rust
    fn script(find_regex: &str, replace_string: &str) -> serde_json::Value {
        serde_json::json!({ "findRegex": find_regex, "replaceString": replace_string, "disabled": false })
    }

    #[test]
    fn try_convert_status_panel_converts_the_only_candidate() {
        let scripts = vec![script(
            r"<update>(.*?)<hp>(\d+)</hp></update>",
            "<div>$1</div><div>HP: $2</div><style>.x{color:red}</style><script>alert(1)</script>",
        )];
        let conv = try_convert_status_panel(&scripts).expect("exactly one candidate must convert");
        assert_eq!(conv.source_index, 0);
        assert_eq!(conv.html, "<div>{{field1}}</div><div>HP: {{field2}}</div>");
        assert_eq!(conv.css, ".x{color:red}");
        assert_eq!(conv.capture_vars, vec![Some("field1".to_string()), Some("field2".to_string())]);
        let names: Vec<&str> = conv.var_decls.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["field1", "field2"]);
        assert!(conv.var_decls.iter().all(|v| v.var_type == crate::state::VarType::String));
    }

    #[test]
    fn try_convert_status_panel_skips_when_no_candidates() {
        // no `$N` anywhere in replaceString -> not a status-bar template.
        let scripts = vec![script(r"<a>(.*)</a>", "plain text, no placeholders")];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_skips_when_ambiguous() {
        // two scripts both qualify -> skip rather than guess.
        let scripts = vec![
            script(r"<a>(.*)</a>", "$1"),
            script(r"<b>(.*)</b>", "$1"),
        ];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_skips_disabled_scripts() {
        let mut s = script(r"<a>(.*)</a>", "$1");
        s["disabled"] = serde_json::json!(true);
        assert!(try_convert_status_panel(&[s]).is_none());
    }

    #[test]
    fn try_convert_status_panel_skips_prompt_only_scripts() {
        // promptOnly (no markdownOnly) -> scope is "prompt", never applies to display.
        let mut s = script(r"<a>(.*)</a>", "$1");
        s["promptOnly"] = serde_json::json!(true);
        assert!(try_convert_status_panel(&[s]).is_none());
    }

    #[test]
    fn try_convert_status_panel_handles_js_literal_find_regex() {
        // `/pattern/flags` form must compile via normalize_js_regex_literal,
        // same as the display-time regex_rule pipeline.
        let scripts = vec![script(r"/<hp>(\d+)<\/hp>/gsi", "hp=$1")];
        let conv = try_convert_status_panel(&scripts).expect("js-literal pattern must still convert");
        assert_eq!(conv.html, "hp={{field1}}");
    }

    #[test]
    fn try_convert_status_panel_out_of_range_dollar_is_not_a_candidate_signal() {
        // 1 capture group, but replaceString only references $10 (out of range) ->
        // not a valid candidate (no in-range $N at all).
        let scripts = vec![script(r"<a>(.*)</a>", "$10 literally")];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_ignores_uncompilable_findregex() {
        // An unbalanced paren never compiles -> excluded from candidates
        // entirely (not an error, not a panic).
        let scripts = vec![script(r"<a>(.*", "$1")];
        assert!(try_convert_status_panel(&scripts).is_none());
    }

    #[test]
    fn try_convert_status_panel_repeated_dollar_n_yields_one_variable() {
        let scripts = vec![script(r"<hp>(\d+)</hp>", "now: $1, again: $1")];
        let conv = try_convert_status_panel(&scripts).unwrap();
        assert_eq!(conv.html, "now: {{field1}}, again: {{field1}}");
        assert_eq!(conv.var_decls.len(), 1);
        assert_eq!(conv.var_decls[0].name, "field1");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shirita-core --lib try_convert_status_panel`
Expected: compile error, `cannot find function try_convert_status_panel` / `struct PanelConversion`.

- [ ] **Step 3: Implement `PanelConversion` and `try_convert_status_panel`**

Add next to the helpers from Tasks 1-2:

```rust
/// Result of recognizing exactly one ST `regex_scripts` entry as the common
/// single-regex/`$N`-template status-bar pattern and converting it into
/// native Panel content. See the 2026-06-22 design spec, §3.
struct PanelConversion {
    source_index: usize,
    html: String,
    css: String,
    var_decls: Vec<VarDecl>,
    capture_vars: Vec<Option<String>>,
}

/// Detect and convert the card's status-bar `regex_scripts` entry, if there
/// is exactly one unambiguous candidate. Returns `None` when zero or
/// multiple scripts qualify — ambiguous detections are skipped rather than
/// guessed.
fn try_convert_status_panel(scripts: &[serde_json::Value]) -> Option<PanelConversion> {
    struct Candidate {
        index: usize,
        replace_string: String,
        valid_ns: Vec<usize>,
    }

    let mut candidates = Vec::new();
    for (index, s) in scripts.iter().enumerate() {
        if s.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        // Same scope derivation as `regex_rule_def`: (markdownOnly, promptOnly)
        // -> "display" | "prompt" | "both". Conversion only applies to scripts
        // that affect display ("display" or "both").
        let markdown_only = s.get("markdownOnly").and_then(|v| v.as_bool()).unwrap_or(false);
        let prompt_only = s.get("promptOnly").and_then(|v| v.as_bool()).unwrap_or(false);
        let display_scope = !(prompt_only && !markdown_only);
        if !display_scope {
            continue;
        }
        let Some(find_regex) = s.get("findRegex").and_then(|v| v.as_str()) else { continue };
        let Some(replace_string) = s.get("replaceString").and_then(|v| v.as_str()) else { continue };
        let normalized = crate::assembly::normalize_js_regex_literal(find_regex);
        let Ok(re) = fancy_regex::Regex::new(&normalized) else { continue };
        let group_count = re.captures_len().saturating_sub(1);
        if group_count == 0 {
            continue;
        }
        let valid_ns = dollar_refs_in(replace_string, group_count);
        if valid_ns.is_empty() {
            continue;
        }
        candidates.push(Candidate { index, replace_string: replace_string.to_string(), valid_ns });
    }

    if candidates.len() != 1 {
        return None;
    }
    let c = candidates.into_iter().next().unwrap();

    let substituted = substitute_dollar_refs(&c.replace_string, &c.valid_ns);
    let (no_style, style_blocks) = extract_tag_blocks(&substituted, "style");
    let (html, _script_blocks) = extract_tag_blocks(&no_style, "script"); // script content is dropped, never preserved

    let max_n = *c.valid_ns.iter().max().unwrap();
    let mut capture_vars: Vec<Option<String>> = vec![None; max_n];
    let mut var_decls = Vec::new();
    for &n in &c.valid_ns {
        let name = format!("field{n}");
        capture_vars[n - 1] = Some(name.clone());
        var_decls.push(VarDecl {
            name,
            var_type: VarType::String,
            initial: serde_json::Value::String(String::new()),
            scope: None,
        });
    }

    Some(PanelConversion { source_index: c.index, html, css: style_blocks.join("\n"), var_decls, capture_vars })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core --lib try_convert_status_panel`
Expected: all 9 tests pass.

- [ ] **Step 5: Run the full adapter test file to check for regressions**

Run: `cargo test -p shirita-core --lib charcard::`
Expected: all tests in this module (old + new) pass.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/adapters/charcard.rs
git commit -m "feat(charcard): detect and convert single-script \$N status-bar templates"
```

---

## Task 4: Wire conversion into `charcard_to_loreset`

**Files:**
- Modify: `shirita-core/src/adapters/charcard.rs:91-239` (the `charcard_to_loreset` function)

**Interfaces:**
- Consumes: `try_convert_status_panel` (Task 3), `regex_rule_def` (existing), `tavern_helper_vardecls` (existing).
- Produces: `charcard_to_loreset` now sets `tmpl.meta.panel.{html,css}` and merges converted `VarDecl`s into `tmpl.meta.variables` when conversion succeeds; the triggering `regex_rule` Definition gains `meta.capture_vars`. No change to its public signature (`fn charcard_to_loreset(card: &serde_json::Value) -> LoreSet`).

- [ ] **Step 1: Write the failing tests**

Add to the test module:

```rust
    #[test]
    fn charcard_to_loreset_populates_panel_for_unambiguous_status_bar() {
        let card = serde_json::json!({
            "data": {
                "name": "Neo", "description": "desc",
                "extensions": { "regex_scripts": [
                    { "scriptName": "status", "findRegex": "<hp>(\\d+)</hp>",
                      "replaceString": "HP: $1", "disabled": false, "markdownOnly": true }
                ] }
            }
        });
        let ls = charcard_to_loreset(&card);
        assert_eq!(ls.template.meta["panel"]["html"], "HP: {{field1}}");
        let vars = ls.template.meta["variables"].as_array().unwrap();
        assert!(vars.iter().any(|v| v["name"] == "field1" && v["type"] == "string"));

        let rule = ls.definitions.iter().find(|d| d.def_type == "regex_rule").unwrap();
        assert_eq!(rule.meta["capture_vars"], serde_json::json!(["field1"]));
        // the compatibility-layer fields are untouched.
        assert_eq!(rule.meta["pattern"], "<hp>(\\d+)</hp>");
        assert_eq!(rule.meta["replacement"], "HP: $1");
    }

    #[test]
    fn charcard_to_loreset_omits_panel_when_no_status_bar_detected() {
        let card = serde_json::json!({
            "data": {
                "name": "Neo", "description": "desc",
                "extensions": { "regex_scripts": [
                    { "scriptName": "r1", "findRegex": "a", "replaceString": "b", "disabled": false }
                ] }
            }
        });
        let ls = charcard_to_loreset(&card);
        assert!(ls.template.meta.get("panel").is_none());
        let rule = ls.definitions.iter().find(|d| d.def_type == "regex_rule").unwrap();
        assert!(rule.meta.get("capture_vars").is_none());
    }

    #[test]
    fn charcard_to_loreset_merges_converted_vars_with_tavern_helper_vars_without_duplicates() {
        let card = serde_json::json!({
            "data": {
                "name": "Neo", "description": "desc",
                "extensions": {
                    "regex_scripts": [
                        { "scriptName": "status", "findRegex": "<hp>(\\d+)</hp>",
                          "replaceString": "HP: $1", "disabled": false, "markdownOnly": true }
                    ],
                    "tavern_helper": { "variables": { "field1": "already declared", "mood": "calm" } }
                }
            }
        });
        let ls = charcard_to_loreset(&card);
        let vars = ls.template.meta["variables"].as_array().unwrap();
        let field1_count = vars.iter().filter(|v| v["name"] == "field1").count();
        assert_eq!(field1_count, 1, "must not declare field1 twice");
        // the tavern_helper declaration (a string type) wins since it was added first and
        // the converter skips names that already exist.
        assert_eq!(vars.iter().find(|v| v["name"] == "field1").unwrap()["initial"], "already declared");
        assert!(vars.iter().any(|v| v["name"] == "mood"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shirita-core --lib charcard_to_loreset_populates_panel charcard_to_loreset_omits_panel charcard_to_loreset_merges_converted_vars`
Expected: FAIL — `ls.template.meta["panel"]` is `Value::Null` (no panel ever written today), assertions fail.

- [ ] **Step 3: Implement the wiring**

In `shirita-core/src/adapters/charcard.rs`, replace the existing regex_scripts loop:

```rust
    // --- non-rendering root refs: regex_scripts + first_message ---
    if let Some(scripts) = data
        .get("extensions")
        .and_then(|e| e.get("regex_scripts"))
        .and_then(|v| v.as_array())
    {
        for s in scripts {
            let d = regex_rule_def(s);
            nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
            defs.push(d);
        }
    }
```

with:

```rust
    // --- non-rendering root refs: regex_scripts + first_message ---
    let empty_scripts: Vec<serde_json::Value> = Vec::new();
    let scripts: &[serde_json::Value] = data
        .get("extensions")
        .and_then(|e| e.get("regex_scripts"))
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_scripts);
    // Detect the common single-regex/$N-template status-bar pattern (see the
    // 2026-06-22 design spec). Ambiguous or absent -> None, and nothing about
    // the regex_rule Definitions below changes.
    let panel_conversion = try_convert_status_panel(scripts);
    for (index, s) in scripts.iter().enumerate() {
        let mut d = regex_rule_def(s);
        if let Some(conv) = &panel_conversion {
            if conv.source_index == index {
                if let Some(obj) = d.meta.as_object_mut() {
                    obj.insert(
                        "capture_vars".to_string(),
                        serde_json::to_value(&conv.capture_vars).unwrap(),
                    );
                }
            }
        }
        nodes.push(PromptNode::new_ref(OwnerKind::Template, &tmpl.id, None, next(&mut sort), &d.id));
        defs.push(d);
    }
```

Then replace the existing variable-registration block:

```rust
    // --- register the card's default chat variables, if any ---
    let vardecls = tavern_helper_vardecls(data);
    if !vardecls.is_empty() {
        tmpl.meta = serde_json::json!({ "variables": vardecls });
    }
```

with:

```rust
    // --- register the card's default chat variables, plus any converted
    // status-bar fields, plus the converted panel itself ---
    let mut vardecls = tavern_helper_vardecls(data);
    if let Some(conv) = &panel_conversion {
        for vd in &conv.var_decls {
            if !vardecls.iter().any(|existing| existing.name == vd.name) {
                vardecls.push(vd.clone());
            }
        }
    }
    let mut meta = serde_json::Map::new();
    if !vardecls.is_empty() {
        meta.insert("variables".to_string(), serde_json::to_value(&vardecls).unwrap());
    }
    if let Some(conv) = &panel_conversion {
        meta.insert("panel".to_string(), serde_json::json!({ "html": conv.html, "css": conv.css }));
    }
    if !meta.is_empty() {
        tmpl.meta = serde_json::Value::Object(meta);
    }
```

Note the `capture_vars` value is serialized from `Vec<Option<String>>` — `serde_json::to_value` turns `None` entries into JSON `null`, matching the design spec's `[Option<String>]` shape (e.g. `["field1", null, "field3"]`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core --lib charcard::`
Expected: all tests in the `charcard` module pass, including the 3 new ones and every pre-existing one (in particular `decomposes_every_nonempty_field` and `imports_tavern_helper_variables_into_template_meta`, which exercise `tmpl.meta` shape and must still pass unchanged).

- [ ] **Step 5: Run the full core test suite**

Run: `cargo test -p shirita-core`
Expected: all tests pass (229+ tests, exact count will have grown by the new tests added in Tasks 1-4).

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/adapters/charcard.rs
git commit -m "feat(charcard): wire status-bar conversion into charcard_to_loreset"
```

---

## Task 5: `capture_panel_updates` in `assembly.rs`

**Files:**
- Modify: `shirita-core/src/assembly.rs`

**Interfaces:**
- Consumes: `crate::state::{Action, Update}` (existing), `crate::models::definition::Definition` (existing), `normalize_js_regex_literal` (Task 1, now `pub(crate)`).
- Produces (for Task 6): `pub fn capture_panel_updates(text: &str, rules: &[Definition]) -> Vec<Update>`.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `shirita-core/src/assembly.rs`, near the other `regex_rule`-related tests:

```rust
    #[test]
    fn capture_panel_updates_extracts_named_groups_into_set_updates() {
        let mut r = def("regex_rule", "status", "");
        r.meta = json!({
            "pattern": "<hp>(\\d+)</hp> <mood>(\\w+)</mood>",
            "replacement": "HP: $1 ($2)",
            "capture_vars": ["hp", "mood"]
        });
        let updates = capture_panel_updates("text <hp>42</hp> <mood>calm</mood> more", &[r]);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0], Update { action: Action::Set, key: "hp".to_string(), value: Some("42".to_string()) });
        assert_eq!(updates[1], Update { action: Action::Set, key: "mood".to_string(), value: Some("calm".to_string()) });
    }

    #[test]
    fn capture_panel_updates_skips_rules_without_capture_vars() {
        let mut r = def("regex_rule", "plain", "");
        r.meta = json!({ "pattern": "x", "replacement": "y" }); // no capture_vars -> not a panel-sync rule
        assert_eq!(capture_panel_updates("x", &[r]), Vec::new());
    }

    #[test]
    fn capture_panel_updates_no_match_yields_no_updates() {
        let mut r = def("regex_rule", "status", "");
        r.meta = json!({ "pattern": "<hp>(\\d+)</hp>", "replacement": "$1", "capture_vars": ["hp"] });
        assert_eq!(capture_panel_updates("no tags here", &[r]), Vec::new());
    }

    #[test]
    fn capture_panel_updates_honors_null_slots_in_capture_vars() {
        // group 1 has no associated variable (out-of-range $N case from Task 3) -> skipped.
        let mut r = def("regex_rule", "status", "");
        r.meta = json!({
            "pattern": "<a>(.)</a><b>(.)</b>",
            "replacement": "$2",
            "capture_vars": [null, "field2"]
        });
        let updates = capture_panel_updates("<a>X</a><b>Y</b>", &[r]);
        assert_eq!(updates, vec![Update { action: Action::Set, key: "field2".to_string(), value: Some("Y".to_string()) }]);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p shirita-core --lib capture_panel_updates`
Expected: compile error, `cannot find function capture_panel_updates`.

- [ ] **Step 3: Implement `capture_panel_updates`**

First add this import near the top of `shirita-core/src/assembly.rs`, alongside the existing `use` lines (e.g. right after `use crate::models::prompt_node::{NodeKind, PromptNode};`):
```rust
use crate::state::{Action, Update};
```
(This is a module-level `use`, so the test submodule's existing `use super::*;` picks it up too — the new tests in Step 1 reference `Update`/`Action` unqualified.)

Then add this function to `shirita-core/src/assembly.rs`, after `apply_regex_rules`:

```rust
/// Pull values for a converted status-bar panel's variables out of one AI
/// turn's raw text, using the same `pattern` that also drives the
/// compatibility-layer display replace in `apply_regex_rules_for` — but
/// read-only (`captures`, never `replace_all`): this never touches what's
/// shown to the user, only extracts values to fold into the session's
/// persistent variable state via the same `apply_updates` call already used
/// for `<state_update>` tags. Only rules carrying `meta.capture_vars`
/// (written by `adapters::charcard::try_convert_status_panel`) participate;
/// every other regex_rule is untouched and contributes nothing here.
pub fn capture_panel_updates(text: &str, rules: &[Definition]) -> Vec<Update> {
    let mut out = Vec::new();
    for rule in rules {
        let Some(capture_vars) = rule.meta.get("capture_vars").and_then(|v| v.as_array()) else {
            continue;
        };
        let Some(pattern) = rule.meta.get("pattern").and_then(|v| v.as_str()) else { continue };
        let Ok(re) = fancy_regex::Regex::new(&normalize_js_regex_literal(pattern)) else { continue };
        let Ok(Some(caps)) = re.captures(text) else { continue };
        for (i, name) in capture_vars.iter().enumerate() {
            let Some(name) = name.as_str() else { continue }; // null slot -> no variable for this group
            if let Some(m) = caps.get(i + 1) {
                out.push(Update { action: Action::Set, key: name.to_string(), value: Some(m.as_str().to_string()) });
            }
        }
    }
    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p shirita-core --lib capture_panel_updates`
Expected: all 4 tests pass.

- [ ] **Step 5: Run the full assembly test module**

Run: `cargo test -p shirita-core --lib assembly::`
Expected: all tests pass (existing + new).

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/assembly.rs
git commit -m "feat(assembly): add capture_panel_updates for converted status-bar panels"
```

---

## Task 6: Wire per-turn sync into `conversation.rs`

**Files:**
- Modify: `shirita-core/src/conversation.rs:371-406` (`send_message`)
- Modify: `shirita-core/src/conversation.rs:462-490` (`regenerate`)

**Interfaces:**
- Consumes: `crate::assembly::capture_panel_updates` (Task 5).
- Produces: no new public interface — both functions' existing signatures/behavior are unchanged except that `assistant.snapshot_state`/`sibling.snapshot_state` now also reflect any converted-panel variables when the mounted regex_rules include a converted one.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `shirita-core/src/conversation.rs`, next to `state_update_folds_into_snapshot_and_strips_display`:

```rust
    #[tokio::test]
    async fn converted_panel_capture_folds_into_snapshot_alongside_state_update_tags() {
        let storage = Arc::new(temp_storage().await);
        let mut t = crate::models::template::Template::new("T");
        t.meta = serde_json::json!({ "variables": [
            { "name": "hp", "type": "number", "initial": 100 },
            { "name": "field1", "type": "string", "initial": "" }
        ] });
        storage.create_template(&t).await.unwrap();

        // An orphan (unreferenced) regex_rule with capture_vars — same shape
        // `try_convert_status_panel` produces — is globally effective per
        // `effective_regex_rules`.
        let mut rule = Definition::new("regex_rule", "status", "");
        rule.meta = serde_json::json!({
            "pattern": "<mood>(\\w+)</mood>",
            "replacement": "$1",
            "capture_vars": ["field1"]
        });
        storage.create_definition(&rule).await.unwrap();

        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        session.current_state = serde_json::json!({ "hp": 100, "field1": "" });
        storage.create_session(&session).await.unwrap();

        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider {
            seen: Arc::new(Mutex::new(None)),
            reply: "<mood>calm</mood> You take a hit. <state_update action=\"SUB\" key=\"hp\" value=\"5\"/>".into(),
        });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());
        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hi".into(), "".into(), Vec::new());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let msgs = storage.list_messages(&session.id).await.unwrap();
        let assistant = msgs.iter().find(|m| m.role == Role::Assistant).unwrap();
        assert_eq!(assistant.snapshot_state["hp"], 95); // <state_update> tag still folds in
        assert_eq!(assistant.snapshot_state["field1"], "calm"); // regex capture folds in too
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shirita-core --lib converted_panel_capture_folds_into_snapshot`
Expected: FAIL — `assistant.snapshot_state["field1"]` is `""` (the initial value), not `"calm"`, because nothing extracts it yet.

- [ ] **Step 3: Implement the wiring**

In `shirita-core/src/conversation.rs`, in `send_message`, change:
```rust
        let (req, _regex_rules) = match assemble_request(storage.as_ref(), &session, model, &context, &branch_state, summary_text.clone()).await {
            Ok(r) => r,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
```
to:
```rust
        let (req, regex_rules) = match assemble_request(storage.as_ref(), &session, model, &context, &branch_state, summary_text.clone()).await {
            Ok(r) => r,
            Err(e) => { yield SendEvent::Error(e.to_string()); return; }
        };
```
and change:
```rust
        // 4) 折叠 <state_update> 进快照、剥离展示文本，落库 assistant 消息，再 yield Done。
        let updates = parse_state_updates(&full);
        let new_snapshot = apply_updates(&branch_state, &schema, &updates);
```
to:
```rust
        // 4) 折叠正则捕获的状态栏变量（在前）+ <state_update> 标签（在后，冲突时标签优先）
        //    进快照、剥离展示文本，落库 assistant 消息，再 yield Done。
        let mut updates = crate::assembly::capture_panel_updates(&full, &regex_rules);
        updates.extend(parse_state_updates(&full));
        let new_snapshot = apply_updates(&branch_state, &schema, &updates);
```

Apply the identical pair of changes in `regenerate`: rename its `_regex_rules` to `regex_rules` and replace its
```rust
        let updates = parse_state_updates(&full);
        let new_snapshot = apply_updates(&branch_state, &schema, &updates);
```
with
```rust
        let mut updates = crate::assembly::capture_panel_updates(&full, &regex_rules);
        updates.extend(parse_state_updates(&full));
        let new_snapshot = apply_updates(&branch_state, &schema, &updates);
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shirita-core --lib converted_panel_capture_folds_into_snapshot`
Expected: PASS.

- [ ] **Step 5: Run the full conversation test module and full core suite**

Run: `cargo test -p shirita-core --lib conversation::`
Expected: all tests pass, including the pre-existing `state_update_folds_into_snapshot_and_strips_display` and the regenerate-path equivalent — unchanged behavior when no `capture_vars`-bearing rule is mounted.

Run: `cargo test -p shirita-core`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(conversation): fold converted-panel regex captures into snapshot state"
```

---

## Task 7: Surface panel conversion in the import summary (backend)

**Files:**
- Modify: `shirita-web/src/routes/import_export.rs:358-391` (`persist_loreset_as_pack`)
- Test: `shirita-web/tests/import_charcard_test.rs`

**Interfaces:**
- Consumes: `pack.meta.get("panel")` (set by Task 4's `charcard_to_loreset` → `loreset_to_pack` → `pack.meta = template.meta`).
- Produces: when a converted panel exists, `persist_loreset_as_pack` pushes one extra `ImportItem { kind: "panel", .. }` into `summary.created` (existing `ImportSummary`/`ImportItem` types, no schema change).

- [ ] **Step 1: Write the failing test**

`shirita-web/tests/import_charcard_test.rs` already has `test_state()` (builds an `AppState` with a temp sqlite db) and `send(&state, method, uri, body)` (drives the router via `tower::ServiceExt::oneshot`, returns `(StatusCode, String)`) — reuse both as-is. Add this test function to the file:

```rust
#[tokio::test]
async fn import_charcard_with_status_bar_reports_a_panel_item() {
    let state = test_state().await;
    let card = r#"{"data":{"name":"Neo","description":"d","extensions":{"regex_scripts":[
        {"scriptName":"status","findRegex":"<hp>(\\d+)</hp>","replaceString":"HP: $1","disabled":false,"markdownOnly":true}
    ]}}}"#;
    let (st, body) = send(&state, "POST", "/api/import/charcard", Some(card)).await;
    assert_eq!(st, StatusCode::OK);
    let summary: Value = serde_json::from_str(&body).unwrap();
    let created = summary["created"].as_array().unwrap();
    assert!(created.iter().any(|c| c["kind"] == "panel"), "expected a panel item in created: {created:?}");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shirita-web --test import_charcard_test import_charcard_with_status_bar_reports_a_panel_item`
Expected: FAIL — no `"panel"` kind in `created`.

- [ ] **Step 3: Implement the summary item**

In `shirita-web/src/routes/import_export.rs`, in `persist_loreset_as_pack`, change:
```rust
    let (pack, defs, mut nodes) = loreset_to_pack(ls, avatar);

    for d in &defs {
        summary.created.push(item("definition", &d.id, &d.name));
    }
```
to:
```rust
    let (pack, defs, mut nodes) = loreset_to_pack(ls, avatar);

    for d in &defs {
        summary.created.push(item("definition", &d.id, &d.name));
    }
    if pack.meta.get("panel").is_some() {
        summary.created.push(item("panel", &pack.id, &pack.name));
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shirita-web --test import_charcard_test import_charcard_with_status_bar_reports_a_panel_item`
Expected: PASS.

- [ ] **Step 5: Run the full shirita-web test suite**

Run: `cargo test -p shirita-web`
Expected: all tests pass (the `"panel"` item is additive — it doesn't change `created.len()` semantics any existing test was asserting on; if any existing test asserts an exact `created.len()` for a card import that also happens to have a convertible status bar, update that assertion's expected count by +1 and note why in a one-line comment).

- [ ] **Step 6: Commit**

```bash
git add shirita-web/src/routes/import_export.rs shirita-web/tests/import_charcard_test.rs
git commit -m "feat(import): report a panel item in the summary when status-bar conversion happens"
```

---

## Task 8: Surface the panel hint in the import UI (frontend)

**Files:**
- Modify: `shirita-ui/src/locales/en.ts`, `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts` (new key `book.importPanelDetected`)
- Modify: `shirita-ui/src/views/BookView.vue` (template, near the existing import-summary paragraph)
- Test: `shirita-ui/src/views/BookView.test.ts`

**Interfaces:**
- Consumes: `importSummary.value.created` (existing `ImportSummary` shape, now may contain a `{ kind: "panel", ... }` item per Task 7).

- [ ] **Step 1: Write the failing test**

`shirita-ui/src/views/BookView.test.ts` already has a working pattern for this exact flow (mock `api.importFile`, mount, set a file on the hidden `<input type="file">`, trigger `change`, `flushPromises()`) in its `'selects the newly imported pack immediately...'` test. Add a new test right after it, inside the same `describe` block:

```ts
  it('shows a hint when the import summary includes a converted panel', async () => {
    ;(api.importFile as any).mockResolvedValue({
      created: [{ kind: 'pack', id: 'p1', name: 'Neo' }, { kind: 'panel', id: 'p1', name: 'Neo' }],
      skipped: [],
      overwritten: [],
    })
    const w = mount(BookView)
    await flushPromises()
    const input = w.find('input[type="file"]').element as HTMLInputElement
    Object.defineProperty(input, 'files', { value: [new File(['x'], 'card.png')], configurable: true })
    await w.find('input[type="file"]').trigger('change')
    await flushPromises()
    expect(w.text()).toContain('Detected a status bar')
  })
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npx vitest run src/views/BookView.test.ts -t "converted panel"`
Expected: FAIL — no such text rendered yet.

- [ ] **Step 3: Add the locale key**

In `shirita-ui/src/locales/en.ts`, near the existing `importSummary` key:
```ts
    importPanelDetected: 'Detected a status bar — generated a native panel preview.',
```
In `shirita-ui/src/locales/zh-Hans.ts`:
```ts
    importPanelDetected: '检测到状态栏，已生成原生面板预览。',
```
In `shirita-ui/src/locales/zh-Hant.ts`:
```ts
    importPanelDetected: '偵測到狀態列，已產生原生面板預覽。',
```
In `shirita-ui/src/locales/ja.ts`:
```ts
    importPanelDetected: 'ステータスバーを検出し、ネイティブパネルのプレビューを生成しました。',
```
(Insert each into the same `book: { ... }` object as the existing `importSummary` key, in all 4 files — `src/locales/parity.test.ts` asserts every locale has the same key set, so missing any one of the four fails that test.)

- [ ] **Step 4: Add the template hint in `BookView.vue`**

In `shirita-ui/src/views/BookView.vue`, find this block (around line 994):

```html
            <p v-if="importSummary" data-test="import-summary" class="text-[12px] text-muted mt-2">
                {{
                    $t("book.importSummary", {
                        created: importSummary.created.length,
                        skipped: importSummary.skipped.length,
                        overwritten: importSummary.overwritten.length,
                    })
                }}
            </p>
```

and add a sibling paragraph right after its closing `</p>`:

```html
            <p
                v-if="importSummary && importSummary.created.some((c) => c.kind === 'panel')"
                data-test="import-panel-hint"
                class="text-[12px] text-muted mt-1"
            >
                {{ $t("book.importPanelDetected") }}
            </p>
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `npx vitest run src/views/BookView.test.ts`
Expected: all tests in the file pass, including the new one.

- [ ] **Step 6: Run the locale parity test and the full frontend suite**

Run: `npx vitest run src/locales/parity.test.ts`
Expected: PASS (all 4 locales still have matching key sets).

Run: `npx vitest run`
Expected: all tests pass.

Run: `npx vue-tsc --noEmit`
Expected: no type errors.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts shirita-ui/src/views/BookView.vue shirita-ui/src/views/BookView.test.ts
git commit -m "feat(import-ui): hint when an imported card's status bar was converted to a native panel"
```

---

## Final verification

- [ ] Run `cargo test -p shirita-core -p shirita-web` from the repo root — all pass.
- [ ] Run `npx vitest run` from `shirita-ui/` — all pass.
- [ ] Run `npx vue-tsc --noEmit` from `shirita-ui/` — no errors.
- [ ] Manually import one of the example cards (`examples/怪谈社.json` has 5 ambiguous scripts — expect conversion to be **skipped**; construct or find a single-script example to see conversion actually fire) via the running app's Book import UI and confirm: the hint line appears, `PackEditor`'s Panel section is pre-populated, and sending a message that includes a matching tag updates the panel live.
