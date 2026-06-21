# SillyTavern Chat-Completion Preset Import Design

> Found during M9 desktop testing: importing a real ST chat-completion preset (`examples/示例预设.json`) returns `import failed: 400`. The importer sniffs `shirita.*` envelopes, ST character cards, and worldinfo — but not ST presets. This spec adds preset → Shirita Template import.

## 1. Goal

Importing a SillyTavern chat-completion preset (the "prompt manager" export: `prompts` + `prompt_order` + sampler settings) produces an editable Shirita **Template** that approximates the preset's enabled prompt assembly. Faithful but lossy: the user gets a working, editable template, not a bit-exact replay of ST behavior.

## 2. Background — the ST preset shape

A preset JSON has no `format` field. Relevant keys:
- `prompts`: array of prompt pieces. Each: `identifier`, `name`, `content`, `role` (system/user/assistant), `system_prompt` (bool), `marker` (bool), `injection_position` (0 = in-order, 1 = absolute depth), `injection_depth`, `injection_order`, `enabled`.
  - **`marker: true`** entries are placeholder slots ST fills from the character/persona/world/history — `chatHistory`, `charDescription`, `charPersonality`, `scenario`, `personaDescription`, `dialogueExamples`, `worldInfoBefore`, `worldInfoAfter`, `enhanceDefinitions`. They carry no authored text.
  - **non-marker** entries are authored text (`main`, `nsfw`, `jailbreak`, custom).
- `prompt_order`: array of groups `{ character_id, order: [{ identifier, enabled }] }`. `character_id == 100000` is ST's default/global group. `order` is the assembled sequence; only its `enabled` entries are placed (a preset can carry 140 prompts in the library but enable only ~11).
- Sampler settings (`temperature`, `top_p`, `max_tokens`, …) — generation params.

## 3. Target — what a successful import produces

Shirita's Template tree uses nodes `Folder/Ref/History/Content` and assembles into a **system preamble + history**; a `Definition` has `def_type/name/content/meta` (no role field). The import maps the preset's **enabled, ordered** prompts onto that model (decisions approved during brainstorming):

Walk group `character_id == 100000`, entries with `enabled == true`, in list order, assigning `sort_order` 0, 1, 2, …. For each entry, resolve its prompt by `identifier`, then classify by the prompt's **`marker`** field (not a hardcoded identifier list — e.g. `enhanceDefinitions` carries `marker: false` and is authored text, not a placeholder):

| Prompt kind | → Shirita |
|---|---|
| `marker != true` — authored text (`main`, `nsfw`, `jailbreak`, `enhanceDefinitions`, custom…) | a `Definition { def_type:"prompt", name: prompt.name, content: prompt.content }` + a root `Ref` node referencing it |
| `marker == true` && `identifier == "chatHistory"` | a `History` node |
| `marker == true`, any other (the char/persona/world/examples placeholders: `charDescription`, `charPersonality`, `scenario`, `personaDescription`, `dialogueExamples`, `worldInfoBefore`, `worldInfoAfter`, …) | **first such marker** emits one `Content` node; later ones skipped |
| identifier absent from `prompts`, or a non-marker with empty/whitespace `content` | skipped with a `tracing::warn!` |

**Dropped** (out of scope, lossy by design): sampler params; `injection_position == 1` / `injection_depth` (Shirita's preamble has no depth injection — these authored prompts are still imported as ordinary in-order Refs, their depth ignored); per-prompt `role` (everything folds into the system preamble); and every prompt not in the enabled order (~129 in the example).

### Name + edge cases
- **Template name** = the uploaded filename stem (`示例预设.json` → `示例预设`); fallback `"Imported preset"` when the filename is absent/empty.
- **No `chatHistory`** in the enabled order → append one `History` node at the end (a template needs a history mount to be usable).
- **Empty/missing** group `100000`, or zero enabled entries → the adapter yields a template with no usable content; the web layer returns `400` (nothing to import) rather than creating an empty template.

## 4. Architecture

Reuse the existing import chain — the same one char-card import already flows through.

- **Core:** new `shirita-core/src/adapters/stpreset.rs`, sibling to `charcard.rs`/`worldinfo.rs`:
  ```rust
  pub fn stpreset_to_loreset(preset: &serde_json::Value, name: &str) -> LoreSet
  ```
  Pure, deterministic, unit-testable. Builds `LoreSet { template, definitions, nodes }` per §3. Re-exported from `lib.rs` (`pub use adapters::stpreset::stpreset_to_loreset;`).

- **Web:** in `routes::import_export::import`, the JSON sniff gains a preset arm. Detection is structural (no `format` field): **`prompts` is an array AND `prompt_order` is an array**. This arm runs in the existing `_ =>` fallback, checked **before** the char-card/worldinfo heuristics (a preset has neither `data.name` nor `entries`, so ordering is safe, but explicit is clearer). On match:
  ```rust
  persist_loreset(&state, stpreset_to_loreset(&v, &name), oc, &mut summary).await?;
  ```
  `persist_loreset` already dedups definitions by name+def_type per `on_conflict`, creates the template, and inserts nodes topologically — no new persistence code.

- **Filename threading:** `import` currently reads bytes via `first_field_bytes`, which drops the multipart filename. Replace that read at the preset path (or generally) so the first field's `file_name()` is captured alongside its bytes; pass the stem as `name`. PNG/other branches are unaffected (they ignore the name).

## 5. Testing

- **Core unit tests** (`stpreset.rs`, always compiled): a small hand-built preset `Value` →
  - authored prompts become `prompt` defs (type/name/content) with matching Ref nodes in `sort_order`;
  - `chatHistory` → a `History` node at its position;
  - two char/world markers → exactly one `Content` node (first wins);
  - an enabled order with no `chatHistory` → a `History` node appended at the end;
  - a disabled entry and an identifier missing from `prompts` are skipped;
  - only group `100000` is read (a second group is ignored).
- **Web integration test** (`shirita-web/tests/`): POST the real `examples/示例预设.json` via multipart → `200`; the summary reports a created `template`; a few expected `prompt` definitions exist. Plus: a preset with an empty enabled order → `400`.

## 6. Decomposition

One plan, two tasks:
- **Task 1 — core `stpreset_to_loreset` adapter** (TDD against §3/§5 unit tests).
- **Task 2 — web sniff + filename threading + integration test** (wire into `import`, reuse `persist_loreset`, test against the real example file).

## 7. Out of scope

Sampler-setting preservation; depth-injection fidelity; per-prompt roles; importing the full prompt library (only the enabled order); exporting Shirita templates *as* ST presets (the existing `tree_to_preset` is a different, Shirita-native format and is unchanged); the ST regex/JS frontend-compat work (deferred separately). The unrelated Pack-section import/export button gap is its own small plan.
