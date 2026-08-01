# ST Status-Bar → Native Panel Conversion — Design

> Follow-on to `2026-06-20-native-card-panels-design.md`'s "v2 (ST card → native conversion)", which that spec deliberately left out of scope. This is v2, scoped narrowly.

## 1. Background & motivation

`HtmlCardFrame.vue` renders ST "frontend cards" as-is in a sandboxed iframe (`sandbox="allow-scripts"`, no app bridge). Investigating real cards (`examples/怪谈社.json`, `examples/密教模拟器.json`) showed the actual SillyTavern mechanism behind most status bars is **not** card-side JS reading runtime state — it's server-side (ST-side) regex templating: the LLM emits a structured `<update>...</update>` tag, a `regex_scripts` entry's `findRegex` captures fields out of it with capture groups, and `replaceString` is an HTML template with positional `$N` placeholders that get the captured values spliced in. Shirita already imports `regex_scripts` as `regex_rule` Definitions and applies them at display time (`assembly::apply_regex_rules_for`); a separate bug in that pipeline (JS-literal `/pattern/flags` parsing) was fixed already and is not part of this spec.

That existing regex-rule/`HtmlCardFrame` path is a faithful **compatibility layer**: it reproduces ST's behavior but the captured values never persist into Shirita's variable system, so the resulting status bar can't participate in the rest of the app (Panel's declarative bindings, the variables sidebar, future automation). This spec adds a **native conversion path**: when a character card's status bar follows the common single-regex/`$N`-template pattern, generate an equivalent `pack.meta.panel` (the existing native Panel feature from the v1 spec) alongside the untouched compatibility layer, and keep the captured values flowing into real session variables every turn.

Both paths are kept side by side — the compatibility layer keeps working unmodified (covers complex/multi-script/JS-dependent cards this conversion intentionally won't touch), and the converted Panel is an additional, native, opt-in-by-editing artifact the user can refine or delete in `PackEditor`.

## 2. Scope

**In scope:** the common pattern — exactly one enabled, display-scoped `regex_scripts` entry whose `replaceString` contains `$N` placeholders fed by `findRegex` capture groups.

**Out of scope (stays on the compatibility layer only):**
- Cards with zero or with multiple ambiguous candidate scripts (see §3).
- Cards whose status bar depends on `<script>` logic beyond simple display (tabs, drag, `localStorage`, computed values) — Panel is deliberately zero-JS; these cards cannot be represented natively without rewriting their logic, which is a manual authoring task, not an automatic conversion.
- Inferring semantic variable names from surrounding HTML/labels — generated variables are named generically (`field1`, `field2`, ... by capture-group number) and left for the user to rename.

## 3. Detection & conversion (import time)

Location: `shirita-core/src/adapters/charcard.rs`, as an added step in `charcard_to_loreset`, after the existing `regex_rule_def` loop produces the card's `regex_rule` Definitions.

**Candidate filter** — from `data.extensions.regex_scripts`, keep entries where:
- `disabled` is `false`.
- Display scope applies: using the same scope derivation as `regex_rule_def` (`markdownOnly`/`promptOnly` → `"display"`/`"prompt"`/`"both"`), the derived scope is `"display"` or `"both"` — matching `apply_regex_rules_for`'s own `phase_ok` rule for `RegexPhase::Display`.
- `findRegex`, after `assembly::normalize_js_regex_literal`, compiles with `fancy_regex`, and the compiled pattern's actual capture-group count (`Regex::captures_len() - 1`, since `captures_len()` includes the whole-match group 0) is at least 1.
- `replaceString` contains at least one `$N` token (regex `\$(\d+)`, excluding `$$`/`$&`) **where `N` is within that pattern's actual capture-group count** — see validation below. A `$N` whose `N` exceeds the group count is not a capture reference at all (it's either a typo in the card or literal text that happens to look like one) and must not be counted toward "this script has placeholders", nor toward the candidate/ambiguity decision.

**`$N` validation (applies both to candidate filtering and to the substitution in step 1-2 below):** for a script's compiled `findRegex`, let `group_count = captures_len() - 1`. For each `$N` token found in `replaceString`, it is a valid capture reference only if `1 <= N <= group_count`. Any `$N` failing this check is left untouched in the output (not replaced, no `field{N}` generated, not counted as evidence the script is a status-bar template). This matters most for cards whose `findRegex` has e.g. 3 groups — a `$10` in `replaceString` is then just two literal characters `$10`, not a reference to a (nonexistent) 10th group, and must survive the conversion unchanged.

**Decision rule:** if the candidate count (scripts with ≥1 *valid* `$N` reference, per the check above) is exactly 1, convert it. If 0 or ≥2, skip conversion entirely (no panel is written; the regex_rule Definition is still created as today, so the compatibility layer is unaffected either way).

**Conversion steps** (only when exactly one candidate):
1. Collect all *valid* `$N` tokens appearing in `replaceString` (per the validation above — invalid ones are excluded here too), dedup, keep original capture-group numbers (no renumbering) → variable names `field{N}`.
2. Replace every valid `$N` in `replaceString` with `{{field{N}}}` (Panel's existing `{{var}}` interpolation syntax); any `$N` that failed validation is left exactly as-is in the output text.
3. Extract any top-level `<style>...</style>` block(s) from the result into `panel.css`; remove them from the HTML.
4. Remove any `<script>...</script>` block(s) entirely (dropped, not preserved anywhere) — Panel forbids `<script>` at render time (`sanitizePanelHtml`) regardless, so this is just doing the same thing earlier and explicitly.

   Both extractions use a case-insensitive, dot-matches-newline `fancy_regex` pattern — real card templates write `<Style>`/`<SCRIPT>` inconsistently and the block content is always multi-line, so a plain `Regex::new` without flags would miss most real instances. Use inline flags, e.g.:
   ```rust
   fancy_regex::Regex::new(r"(?is)<style\b[^>]*>(.*?)</style\s*>")
   fancy_regex::Regex::new(r"(?is)<script\b[^>]*>(.*?)</script\s*>")
   ```
   (`i` = case-insensitive, `s` = `.` matches newline; the non-greedy `.*?` keeps multiple separate blocks from being merged into one match.)
5. The remaining markup becomes `panel.html`.
6. For each `field{N}`, append a `VarDecl { name: "field{N}", var_type: String, initial: "" }` to the pack's variable declarations (merged with `tavern_helper_vardecls`'s output; skip if a declaration with that name already exists).
7. On the triggering regex_rule Definition's `meta`, add `capture_vars: [Option<String>; max_group]` — index `i` (0-based, capture group `i+1`) holds `Some("field{i+1}")` if that group's `$N` appeared in the template, else `null`. The existing `pattern`/`replacement` keys are untouched — the compatibility layer's display-time regex replace keeps working exactly as before.
8. Write the generated html/css into `pack.meta.panel` (the same field `PackEditor.vue` already edits).

If no panel existed before (the normal case for a fresh import), this is the first time `pack.meta.panel` is populated. If a pack is re-imported over an existing one with a manually-authored panel, conversion is skipped if a panel already exists (never clobber manual edits) — checked by the caller before invoking the converter, not inside it.

## 4. Per-turn variable sync (generation time)

Location: `shirita-core/src/assembly.rs` (new function) + `shirita-core/src/conversation.rs` (call site, both the normal-generation and regenerate paths).

**New function** `capture_panel_updates(text: &str, rules: &[Definition]) -> Vec<Update>`:
- Filter `rules` to those with a non-empty `meta.capture_vars`.
- Compile each rule's `pattern` (via `normalize_js_regex_literal`, same as the display path) and run `.captures(text)` (read-only — does not call `replace_all`, does not touch the text used for display; the compatibility layer's own `apply_regex_rules_for` call is separate and unaffected).
- For each non-`null` entry in `capture_vars` at index `i`, if capture group `i+1` matched, emit `Update { action: Set, key: <name>, value: <captured text> }`.
- No match on the whole pattern → contributes zero updates (existing "no update declared ⇒ value holds" semantics apply; no special-casing needed).

**Call sites** (`conversation.rs`): both places that currently do
```rust
let updates = parse_state_updates(&full);
let new_snapshot = apply_updates(&branch_state, &schema, &updates);
```
change to compute `capture_panel_updates(&full, &mounted_regex_rules)` first, then concatenate `[capture_updates, parse_state_updates(&full)].concat()` before the single `apply_updates` call. Order matters: regex-captured updates are applied first, `<state_update>` tag updates second, so if both happen to target the same key the explicit native tag wins (it's a deliberate author signal; the regex capture is a best-effort inference).

This requires `mounted_regex_rules` (already-resolved `regex_rule` Definitions for the session's mounted packs) to be available at both call sites — both already resolve a definitions/rules list nearby for the existing `apply_regex_rules_for` display step, so this reuses that resolution rather than adding a new fetch.

## 5. Data model changes

- `Definition.meta` (type `regex_rule`): new optional key `capture_vars: Vec<Option<String>>`. Absent/empty ⇒ no behavior change (every existing hand-authored or previously-imported regex_rule is unaffected).
- `Pack.meta.panel.{html,css}`: no schema change; conversion is just another writer of the same field `PackEditor.vue` already reads/writes.
- Pack `VarDecl` list: gains entries from the converter, same shape and same merge point as the existing `tavern_helper_vardecls` output.

## 6. Frontend

No new components. `PackEditor.vue`'s existing Panel section (html/css fields + live `PanelView` preview) displays converted content exactly like manually-authored content — the user opens the imported pack and finds the Panel section already populated instead of empty, and can rename `field11`-style variables or trim the markup directly there.

`ImportSummary` (surfaced in `BookView.vue` after import) gets one additional line when conversion happened, e.g. "Detected a status bar — generated a native panel preview", so the user is aware something was added even though import isn't interrupted by a dialog (per the earlier decision to not block import on a choice prompt).

## 7. Testing

**`shirita-core` unit tests (`charcard.rs`):**
- Exactly one candidate → correct `panel.html`/`panel.css`/`VarDecl`s/`capture_vars`, including multi-digit group numbers (`$11`).
- Zero candidates (no `$N` in any script) → `pack.meta.panel` stays unset.
- Multiple ambiguous candidates → conversion skipped, no panic, regex_rule Definitions still created normally.
- `<style>`/`<script>` blocks correctly extracted/dropped, including mixed-case tags (`<Style>`) and multi-line content.
- A `$N` beyond the pattern's actual capture-group count is left as literal text, generates no `VarDecl`, and does not count toward candidate detection (e.g. a 3-group pattern with a `$10` in `replaceString` must not be misread as a 10th-group reference).
- Re-import over a pack with an existing manually-authored panel → conversion skipped, existing panel untouched.

**`shirita-core` unit tests (`assembly.rs`):**
- `capture_panel_updates` produces the expected `Update` list from matching text.
- No match → empty list.
- Merge ordering: when both a regex capture and a `<state_update>` tag target the same key, the tag's value wins in the final snapshot.

**`shirita-core` integration tests (`conversation.rs`):**
- End-to-end: generate a reply containing an `<update>`-style tag matching a converted pack's pattern → assert the resulting `assistant.snapshot_state` contains the expected `field{N}` values. Repeat for the regenerate path.

## 8. Edge cases

- Invalid `findRegex` (fails to compile): the script is excluded from the candidate set outright (doesn't count toward "exactly one" or "ambiguous"); never causes a panic.
- A `$N` repeated multiple times in `replaceString`: one `VarDecl` is generated; every `{{fieldN}}` occurrence shares the same value (native `{{var}}` interpolation already supports repeated references).
- `replaceString` contains a `$N` higher than the pattern's actual capture-group count: per the validation rule in §3, this is never treated as a reference — no `VarDecl` is generated for it, and the literal text `$N` is left untouched in `panel.html`. (Superseded an earlier draft of this spec that treated it as "declared but always empty" — that was wrong: an out-of-range `$N` isn't a capture reference at all, so generating a variable for it would be inventing a binding the card never asked for.)
- A variable name collision (e.g. the card's `tavern_helper.variables` already declares `field11`): the converter skips adding a duplicate `VarDecl`, reusing the existing declaration rather than erroring.
