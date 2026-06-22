# ST Preset Import — Fidelity v2 Design

> Follow-up to `2026-06-21-st-preset-import-design.md`. v1 imports a preset but drops almost everything: for `examples/示例预设.json` it keeps 5 nodes and loses 129 library prompts, every disabled prompt, all `{{setvar}}` variables, and any tag structure. v2 makes an imported preset actually usable. All changes live in the existing core adapter `stpreset_to_loreset`; the web `persist_preset` path is unchanged (it already stores `template.meta` and creates folders before their child Refs).

## 1. Goal

Reduce the loss so an imported preset is editable and playable: keep all authored prompts (by enabled/disabled status), recognize `setvar`/`getvar` variables, give each section tag structure, and preserve cross-node XML as folders. Still lossy (samplers, depth, roles, exotic macros) but no longer near-empty.

## 2. Scope

One adapter, three parts, plus the unchanged v1 behavior for markers:
- Markers unchanged: first char/persona/world/examples marker → one Content mount; `chatHistory` → History; if the active order has no `chatHistory`, append a History at the end.
- Active order = group `character_id == 100000`, walked in list order, position = `sort_order` 0,1,2,…
- "Authored" = a prompt with `marker != true`.

### Part A — import every authored prompt by status
- **In active order, enabled** → an `enabled` Ref at its position.
- **In active order, disabled** → a `disabled` Ref at its position (content + slot preserved).
- **Not in the active order** → `disabled` Refs collected under one **`inactive` folder** (kind `Folder`, `tag: "inactive"`, `enabled: false`) appended after History. A disabled folder is skipped whole at assembly, and collapses the long tail (129 here) into one node in the UI.
- Markers that are not in the active order are skipped (they carry no text).

### Part B — tag structure (`wrap_in_tag`) + cross-node spans
Operates on the **enabled, in-order authored** prompts in sort order, after Part C has stripped variables.

- **Cross-node span → folder.** Scanning left to right, if one prompt has an opening `<T>` with no matching `</T>` in its own content, and a *later* enabled in-order prompt has the matching unmatched `</T>`, the contiguous run `[opener … closer]` becomes a **`Folder` tagged `sanitize_tag(T)`** at the opener's position. Those prompts become its Ref children (in order); the literal `<T>` is stripped from the first child and `</T>` from the last. Children in a span folder are **not** individually wrapped (the folder emits `<T>…</T>`).
  - Tag token: `</?NAME>` where `NAME` is any run without whitespace/`<`/`>`/`/` (so CJK tags like `<最新互动>` match).
  - Bounded: non-overlapping, first match wins; a prompt consumed by one span is not reconsidered. Nested, interleaved, overlapping, or mid-content spanning tags are **not** bundled — those prompts are left raw (see below). Span detection ignores disabled and not-in-order prompts. The run must also be contiguous in the active order with **no marker** (Content mount / History) between opener and closer; if one intervenes, the span is not bundled (left raw), so bundling never moves a prompt across the history boundary.
- **`wrap_in_tag` otherwise.** For each enabled in-order authored prompt **not** in a span folder: set the def's `meta.wrap_in_tag = true` **iff** its content is tag-balanced (every `<T>` it opens it also closes, or it has no tags). A prompt with a stray unclosed/unopened tag — e.g. `main`'s four `<最新互动>` references that never close — is left **raw** (`wrap_in_tag` off), never mangled.
  - `wrap_in_tag` wraps in `sanitize_tag(def.name)` (e.g. `📘 字数设置（随意更改）`); empty after sanitize → falls back to `def_type` ("prompt") per existing `maybe_wrap`.
  - Disabled / not-in-order prompt defs follow the same balanced rule (harmless while disabled; correct if later enabled).

### Part C — variable recognition (`setvar` / `getvar` only)
Applied to **every** authored prompt's content (in-order and not-in-order) before Parts A/B decide nodes:
- `{{setvar::name::value}}` → register a `VarDecl { name, var_type, initial: value }` into `template.meta.variables`; **strip** the macro from the content. `var_type` inferred from `value`: all-digits (optional `-`/`.`) → `Number`; `true`/`false` → `Bool`; else `String`. Duplicate `name` across prompts: first occurrence's value is the declared `initial` (deterministic); later ones are still stripped.
- `{{getvar::name}}` → rewrite to `{{name}}` (Shirita's native syntax, resolved by `render_vars`).
- Other macros (`{{trim}}`, `{{user}}`, …) left literal; `{{// comments}}` are already stripped at assembly, so no special handling.
- A prompt whose content is empty/whitespace **after** stripping (e.g. the `jailbreak` "变量（别动）" block — 600+ chars of pure `setvar`) produces **no Ref node**; its variables are still registered. (v1's empty-content skip already covers this.)

## 3. Worked result for `examples/示例预设.json`

- `main` → raw Ref (has stray `<最新互动>`, not wrapped); positioned first.
- Content mount (at `worldInfoBefore`'s position); `charDescription`/`charPersonality`/`scenario`/`worldInfoAfter`/`dialogueExamples` markers skipped.
- `nsfw` → Ref (balanced, `wrap_in_tag` on); registers `wordsCloud` variable.
- History (at `chatHistory`'s position).
- `jailbreak` → **no node**; registers ~15 variables (`JailbreakPrompt`, `cotTitle`, …) into `template.meta.variables`.
- One `inactive` folder holding the not-in-order authored prompts as disabled Refs (pure-`setvar` ones contribute variables only).
- No span folders (this file has no closeable cross-node tag).

## 4. Testing

**Core unit tests** (synthetic presets, in `stpreset.rs`):
- status: enabled→enabled Ref; disabled-in-order→disabled Ref at position; not-in-order authored→disabled Refs under an `inactive` folder.
- span: prompts `A="<rules>foo"`, `B="bar</rules>"` (both enabled, in order) → a `Folder` tagged `rules` with two children, literal `<rules>`/`</rules>` stripped, children not individually wrapped.
- wrap: a balanced/no-tag prompt → `wrap_in_tag: true`; a prompt with a stray `<x>` that never closes → `wrap_in_tag` absent/false (raw).
- variables: `{{setvar::hp::100}}` → `VarDecl{hp, number, 100}` in `template.meta.variables`, stripped from content; `{{getvar::hp}}` → `{{hp}}`; an all-`setvar` prompt → no Ref but variables registered.
- markers/history unchanged from v1.

**Web integration test** (real file): POST `示例预设.json` → 200; `template.meta.variables` contains `wordsCloud` and `JailbreakPrompt`; an `inactive` folder node exists; no `prompt` def equals the `jailbreak` name; `main` def has no `wrap_in_tag` while `nsfw` does. The v1 collision-independence test still holds (`persist_preset` unchanged).

## 5. Out of scope

Samplers; `injection_position`/depth; per-prompt roles; macros beyond `setvar`/`getvar` (incl. `{{trim}}`, `{{user}}`, conditionals, inline JS — the deferred #4 work); nested/interleaved/mid-content cross-node tags; importing alternate `prompt_order` groups.

## 6. Decomposition

One plan, TDD, tasks roughly: (1) Part C variable extraction helper; (2) Part B tag-balance + span detection + wrap decision; (3) Part A status assembly + `inactive` folder, wiring C/B into `stpreset_to_loreset`; (4) web integration test against the real file. Each ends green.
