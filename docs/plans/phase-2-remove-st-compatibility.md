# Phase 2 implementation plan: remove SillyTavern compatibility

> Status: implemented  
> Date: 2026-08-01  
> Scope: remove SillyTavern compatibility from the main application without removing Shirita-native data or generally useful runtime capabilities

## Goal

Reduce the maintenance burden of the main application by deleting the SillyTavern import/export adapters and the runtime semantics that exist only to emulate SillyTavern.

After this phase, Shirita imports and exports only its native portable formats:

- `shirita.definition` JSON;
- `shirita.template` JSON;
- `shirita.pack` JSON or ZIP, including bundled assets.

The phase is a deletion and boundary-cleanup project. It is not a redesign of Prompt composition, regular expressions, state/tools, panels, the default UI, or portable bundle versioning.

## Required invariants

The phase is complete only when all of these are true and covered by tests where applicable:

- The main application contains no Character Card, ST preset, World Info, TavernHelper, PNG-card, or ST preset-export adapter.
- `/api/import` accepts all currently supported Shirita-native definition, template, and pack bundles.
- `/api/import` no longer recognizes arbitrary JSON through ST-shaped heuristics.
- The dedicated `/api/import/charcard` and `/api/import/worldinfo` compatibility endpoints no longer exist.
- The frontend file picker advertises only native JSON and ZIP imports.
- Existing definitions, templates, packs, sessions, messages, assets, and mounted relationships remain usable, including records originally produced by an old ST import.
- No migration deletes an imported definition, template, pack, node, asset, session, or message.
- `depth_inserts` and the `first_message.meta.depth` runtime interpretation are removed.
- Regular-expression transforms, keyword activation, branching/regenerate, native Prompt tree ordering, token counting, context budgeting, HTML/CSS panels, variables, and state updates remain functional.
- Desktop/Tauri and self-hosted Web continue to expose the same native import behavior through the shared server and UI.
- Current documentation describes the resulting code, while archived documents remain untouched historical records.

## Deletion boundary

### Delete

Delete these compatibility-only core modules and their exports:

- `shirita-core/src/adapters/charcard.rs`;
- `shirita-core/src/adapters/stpreset.rs`;
- `shirita-core/src/adapters/worldinfo.rs`;
- `shirita-core/src/adapters/preset.rs`;
- `shirita-core/src/adapters/mod.rs` and the `adapters` module itself;
- `shirita-core/src/pngcard.rs`;
- `charcard_to_loreset`, `loreset_to_pack`, `LoreSet`, `stpreset_to_loreset`, `worldinfo_to_defs`, `tree_to_preset`, and `read_card_json` re-exports.

Delete the following main-application behavior:

- PNG Character Card detection, embedded card JSON parsing, avatar extraction, and related temporary/orphan cleanup in the import route;
- structural sniffing of unmarked JSON as an ST card, preset, or World Info file;
- dedicated character-card and World Info HTTP endpoints;
- compatibility-only persistence helpers such as `persist_loreset_as_pack` and `persist_preset` once their callers are gone;
- `AssembledPlan.depth_inserts`, `DepthInsert`, collection of `first_message.meta.depth`, and depth-based message splicing;
- tests and fixtures whose only contract is ST compatibility.

### Preserve

The following are Shirita capabilities even where their implementation history overlaps with ST:

- scoped and global `regex_rule` definitions, JavaScript-style `/pattern/flags` normalization, capture groups, display/prompt phases, and user/AI targets;
- `capture_vars` behavior used by native regex/state integration unless a separate later design explicitly replaces it;
- branching, active paths, regenerate, alternate replies, hidden messages, and conversation-tree storage;
- keyword-triggered definitions and recursive activation;
- `BeforeHistory`, `AfterHistory`, `History`, `Content`, folders, refs, and the composable Prompt tree;
- `first_message` as a normal session-start greeting, but not `meta.depth` injection;
- HTML/CSS panels and full-document HTML rendering;
- variables/state definitions and state-update processing;
- attachments and ordinary PNG/JPEG/WebP assets. Removing PNG card import must not remove image upload or native pack assets;
- token estimates, actual provider usage, context-window budgeting, trimming, and summaries;
- all `portable.rs` native bundle parsing, ID remapping, conflict handling, asset collection/rewriting, ZIP safety limits, and hash deduplication.

Comments on preserved code should describe Shirita behavior directly rather than retain ST terminology.

## Data and migration policy

Old imports have already been normalized into ordinary Shirita records. Their origin is not a reason to remove them.

Phase 2 must therefore not attempt to identify and delete old imported characters, lore, regex rules, panels, templates, or packs. There is no reliable provenance boundary, and those records can be valid user-owned content after import.

`st_raw` is different: current code writes it only as opaque compatibility data for round-tripping and no non-adapter runtime reads it. Remove the code that creates or interprets it. Add one narrow, idempotent database migration using the next available migration number at implementation time to remove only the top-level `st_raw` member from definition metadata when present. Preserve every other metadata key and the definition itself.

The migration must have storage-level tests proving:

1. `{"st_raw": {...}, "avatar": "a.png", "custom": 1}` becomes `{"avatar": "a.png", "custom": 1}`;
2. metadata without `st_raw` is unchanged;
3. the definition's ID, type, name, content, timestamps, nodes, and pack membership are unchanged;
4. running migrations again is harmless.

Do not recursively strip a property named `st_raw` from arbitrary user JSON. Do not special-case an old portable bundle forever: if an old native bundle later reintroduces an unknown inert metadata key, the runtime must simply not interpret it.

## Task 1: lock the native portable-format contract

### Purpose

The current unified import route mixes native persistence with ST detection and conversion. Before deleting adapters, establish tests that clearly identify the native behavior that must survive.

### Tests first

Consolidate or extend native import coverage in `shirita-web/tests/import_test.rs`, `pack_import_test.rs`, and existing export tests:

1. import a `shirita.definition` JSON document;
2. import a `shirita.template` JSON document with definitions and nodes, verifying fresh ID remapping and valid references;
3. import a binary-less `shirita.pack` JSON document;
4. round-trip a `shirita.pack` ZIP with an avatar or panel asset;
5. retain `skip`, `overwrite`, and `duplicate` behavior for each applicable native unit;
6. retain pack ZIP traversal, nested-asset, entry-count, per-entry-size, total-size, and missing-manifest rejection;
7. retain asset hash deduplication and missing-asset reference cleanup;
8. reject unknown JSON, malformed JSON, arbitrary PNG bytes, and a ZIP without a valid `shirita.pack` manifest;
9. explicitly reject representative ST card, preset, and World Info JSON through the generic endpoint.

The rejection tests define absence of compatibility. Assert `400 Bad Request`; do not require format-specific ST error messages, because detecting the old formats would itself preserve compatibility logic.

### Refactor boundary

- Rename misleading test descriptions and file-level comments that call the unified endpoint an ST importer.
- Remove ST-only cases from the mixed `import_test.rs`; do not delete its native definition and invalid-input cases.
- Keep `pack_import_test.rs` and the native export suites intact.
- If import persistence helpers are difficult to distinguish, group native helpers under names such as `persist_portable_definition`, `persist_template_bundle`, and `persist_pack_bundle`. Do not introduce a new generic import framework.

### Acceptance

- Native tests pass before and after adapter deletion.
- The accepted input contract is based on the explicit `format` discriminator or a valid native pack ZIP manifest, never filename or structural resemblance.
- Asset files remain supported inside native pack bundles even though standalone PNG cards are rejected.

## Task 2: remove ST import/export surfaces end to end

### Core deletion

- Remove the compatibility modules listed in the deletion boundary.
- Remove their declarations and public re-exports from `shirita-core/src/lib.rs`.
- Remove dependencies only if `cargo machete` or repository-wide search proves they have no remaining use. For example, do not remove `base64` merely because PNG-card parsing used it if attachments or providers still require it.
- Run `cargo check --workspace` immediately after deletion to find hidden call sites.

### Web route cleanup

In `shirita-web/src/routes/import_export.rs`:

- retain multipart limits, conflict parsing, import summaries, portable persistence, safe ZIP handling, asset restoration, ID remapping, and native exports;
- delete `PNG_SIG`, PNG card save/parse logic, `with_avatar`, `persist_loreset_as_pack`, `persist_preset`, and any adapter-specific imports;
- make the generic dispatcher follow this exact order:
  1. ZIP signature: unpack and require a valid `shirita.pack` manifest;
  2. otherwise parse JSON;
  3. accept only the three explicit native `format` values;
  4. reject everything else with `400`;
- remove `import_charcard` and `import_worldinfo`;
- remove their routes from `shirita-web/src/lib.rs`.

Do not retain a filename-based `.png` branch or an ST-shaped JSON warning branch. Those would keep parser knowledge in the main application.

### Frontend cleanup

- Change the hidden import input in `shirita-ui/src/views/BookView.vue` from JSON/PNG/ZIP to JSON/ZIP only.
- Keep `importFile`, conflict resolution, import summaries, native selection after import, and error reporting.
- Update `BookView.test.ts` and `api/client.test.ts` fixtures so import tests use a native `.json` or `.zip` file rather than `card.png`.
- Do not infer client-side format from extension beyond the picker hint; the server remains authoritative.

### ST-only tests to remove

Delete test files only when the entire file is compatibility-only:

- `shirita-web/tests/import_charcard_test.rs`;
- `shirita-web/tests/import_preset_test.rs`;
- `shirita-web/tests/import_preset_v2_test.rs`;
- `shirita-web/tests/import_export_test.rs` if inspection confirms it contains only the two dedicated compatibility endpoints;
- `shirita-web/tests/import_empty_test.rs` if inspection confirms it contains only World Info behavior.

Prune individual ST cases from mixed files instead of deleting the file. Search `Cargo.toml` explicit `[[test]]` declarations and remove stale entries if present.

### Acceptance

- A native JSON/ZIP import succeeds through both the UI and HTTP API.
- An ST PNG or JSON import returns `400` and creates no database rows or asset files.
- Requests to the removed dedicated endpoints return `404`.
- No ST adapter symbol remains in compiled code.

## Task 3: remove ST-only runtime semantics

### Depth insertion

Delete the Author's-Note-style depth insertion path from `shirita-core/src/assembly.rs`:

- remove `DepthInsert`;
- remove `AssembledPlan.depth_inserts`;
- remove collection of `first_message` definitions carrying `meta.depth` and `meta.role`;
- remove splicing in `build_chat_messages`;
- update all `AssembledPlan` constructors and test fixtures;
- delete tests that assert depth insertion and replace any combined test with assertions for the still-valid current-turn-last ordering from Phase 1.

Afterward, a `first_message` definition remains a session greeting. An old `meta.depth` field may exist as inert user metadata but has no runtime effect.

### Preserve adjacent behavior

The change must not alter:

- segment placement around History;
- the Phase 1 invariant that a trailing current user turn is provider-visible last;
- image attachments on that current turn;
- adjacent-role normalization;
- state and HTML-patch protocol insertion;
- history-disabled behavior;
- context trimming and token accounting.

Update focused assembly and conversation tests to prove these behaviors still pass without a depth-insert branch.

### TavernHelper boundary

Repository search currently locates TavernHelper conversion inside the Character Card adapter. Deleting that adapter removes its status-bar conversion and generated panel/regex bricks. Do not delete native `capture_vars`, panels, full-document HTML cards, regex/state integration, or panel rendering unless a post-deletion search finds a separate code path whose only caller and only data producer were the deleted adapter.

If such an apparently dead runtime path is found, record it for Phase 4 rather than broadening Phase 2 without review.

### Acceptance

- `rg` finds no `DepthInsert` or `depth_inserts` in current code or current architecture docs.
- Setting `meta.depth` on a first message does not inject it into later requests.
- Normal session greetings and preserved runtime features remain green.

## Task 4: remove compatibility residue without erasing provenance history

### Database metadata

- Add and test the narrow `st_raw` migration described above.
- Remove all code that writes, reads, or asserts `st_raw`.
- Do not add an ST-origin marker to replace it.

### Terminology in current code

Search current source and tests for `SillyTavern`, `ST`, `charcard`, `World Info`, `TavernHelper`, `loreset`, `depth_prompt`, and `Author's Note`.

For preserved behavior, reword comments rather than deleting functionality. Known examples include:

- message edit and regenerate comments in `messages.rs`, `conversation.rs`, and `stores/chat.ts`;
- HTML-card comments in `HtmlCardFrame.vue` and `utils/markdown.ts`;
- regex comments in `assembly.rs` that should describe supported JavaScript regex-literal syntax rather than compatibility;
- architecture rationale for branching, keyword activation, regex scopes, and panels.

Do not mechanically remove ordinary image filenames such as `a.png`, the acronym `ST` inside unrelated words, or references inside `docs/archive/`.

### User-facing text

Replace the four locale `aboutText` strings that call Shirita a SillyTavern alternative. Use equivalent translations of the current product identity: a user-controlled, composable AI role-playing platform.

Update locale parity tests if present. Do not combine this with broad copywriting or visual redesign.

### Current documentation

Update after implementation, not before the code exists:

- `README.md`: native import/export only; removal is completed rather than scheduled;
- `ARCHITECTURE.md`: remove adapter/pngcard sections and `depth_inserts`; accurately document native portable imports and preserved features;
- `docs/current-direction.md`: mark Phase 2 outcomes as completed while keeping later phases unchanged;
- `docs/module-docs.md` and `shirita-core/src/MODULE_DOCS.md`: regenerate through an existing documented generator if one exists; otherwise minimally remove deleted-module and depth-insert claims and note that they are implementation surveys;
- this plan's status may be changed from `proposed` to `implemented` only after all acceptance checks pass.

Do not rewrite `docs/archive/superpowers/` or other archived documents. Historical references to ST are expected there.

## Explicitly out of scope

- A standalone or one-time ST-to-Shirita converter. Decide that separately after the main application is clean.
- A legacy-import plugin API or compatibility crate.
- Prompt assembly modes, a slot editor, Prompt inspector, or Prompt tree redesign.
- Changing regex metadata, regex execution order, or replacing regex with tools.
- Converting state/variables to registered tools.
- Provider-native tool calling or event normalization.
- The Phase 3 AppShell, ChatView, MessageList, Composer, navigation, or CSS rewrite.
- Token usage UI redesign or removal.
- Portable format v2, schema generalization, or arbitrary plugin registration.
- Removing old user content merely because it originated in ST.

## Integration and commit order

Implement in this order so every commit has a clear review boundary:

1. **test(import): lock native portable import contract**  
   Add missing native acceptance/rejection tests and disentangle mixed test descriptions. No production behavior change.
2. **refactor(import): remove SillyTavern compatibility surfaces**  
   Delete core adapters and PNG parser, remove web endpoints/heuristics, prune UI PNG import, and remove ST-only tests in one coordinated compile-safe change.
3. **refactor(prompt): remove depth insertion compatibility semantics**  
   Delete `DepthInsert` and its runtime/test paths while preserving Phase 1 message ordering.
4. **chore(storage): remove obsolete st_raw metadata**  
   Add the narrow idempotent migration and its tests.
5. **docs: describe the native-only import boundary**  
   Clean current terminology, locales, README, architecture, direction, and module surveys.

Do not mix dependency upgrades, UI redesign, Prompt redesign, state/tool normalization, or unrelated formatting into these commits.

## Verification commands

Run focused checks during implementation:

```bash
cargo test -p shirita-core assembly::tests
cargo test -p shirita-core storage::sqlite::tests
cargo test -p shirita-web --test import_test
cargo test -p shirita-web --test pack_import_test
cargo test -p shirita-web --test pack_export_test
npm --prefix shirita-ui run test -- src/api/client.test.ts
npm --prefix shirita-ui run test -- src/views/BookView.test.ts
```

Run static residue checks, interpreting results rather than blindly requiring no match:

```bash
rg -n "charcard|stpreset|worldinfo|TavernHelper|st_raw|DepthInsert|depth_inserts" \
  shirita-core shirita-web shirita-ui README.md ARCHITECTURE.md docs \
  --glob '!docs/archive/**'
rg -n "SillyTavern|loreset|depth_prompt|Author.s Note" \
  shirita-core shirita-web shirita-ui README.md ARCHITECTURE.md docs \
  --glob '!docs/archive/**'
```

Before handoff, run the full relevant suites:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix shirita-ui run test
npm --prefix shirita-ui run build
```

Also start the application once against a migrated copy of a real pre-Phase-2 database and manually verify:

1. an old imported pack and its mounted definitions still open;
2. an existing conversation with that pack still loads and can generate;
3. its avatar/panel assets still resolve;
4. native definition/template JSON and pack ZIP round-trips still work;
5. ST PNG/JSON is rejected without creating rows or files;
6. Tauri and self-hosted Web use the same behavior.

If a suite fails because of unrelated pre-existing work, record the exact command and failure. Do not weaken unrelated tests or preserve compatibility code merely to keep obsolete tests green.

## Definition of done

- Every item in the deletion boundary is gone from the main application's compiled code.
- Native definition, template, and pack import/export tests cover the retained contract and pass.
- Removed formats fail without partial database or filesystem writes.
- Existing normalized user content survives migration and remains usable.
- `st_raw` is no longer produced or interpreted and is narrowly removed from local definition metadata.
- `depth_inserts` has no model, assembly, test, or current-documentation path.
- Regex, keyword activation, branching/regenerate, greetings, panels, variables/state, assets, token accounting, budgeting, and Phase 1 correctness tests remain green.
- Current documentation and all four locales reflect Shirita's own product identity.
- Archived history is unchanged.
- No Phase 3 UI rewrite, Phase 4 tool/state normalization, or Prompt composition redesign is included.
