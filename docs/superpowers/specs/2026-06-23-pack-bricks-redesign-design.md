# Pack Bricks Redesign — Design

> Restructure packs so prompt assembly is "Lego bricks," not SillyTavern's rigid
> god-object. The panel facets that still live as untyped blobs in `pack.meta`
> (`panel.{html,css,caps}`) and `pack.meta.variables` become first-class
> **definitions** (bricks) referenced by the node tree, like `regex_rule` already
> is. A pack becomes "a name + identity + a tree of brick-refs" with **no
> internal priority**; packs themselves remain ordered (mount order).
>
> Project is in the testing phase with no users, so there is **no data
> migration** — this is a clean break. The ST charcard importer is still updated
> to emit the new brick set going forward.

## 0. Background — current state (verified in code)

- `Definition` (`shirita-core/src/models/definition.rs`): `id / type / name /
  content / meta`. This is already the universal brick. The node tree
  (`prompt_nodes`, owner_kind ∈ {template, session, pack}) holds `ref` nodes
  pointing at definitions of any type.
- **Regex is already a brick:** `regex_rule` definitions; the panel-sync variant
  carries `meta.capture_vars` and is read by `assembly::capture_panel_updates`.
- **Two facets are NOT yet bricks** — they sit in `pack.meta` as untyped JSON:
  - `pack.meta.panel = { html, css, caps }` — exactly one panel per pack, read in
    `PackEditor.vue` and `ChatView.vue` (`panelOf(p) = p.meta.panel`).
  - `pack.meta.variables = VarDecl[]` — merged into session schema by
    `state::resolve_schema_with_packs` (and `template.meta.variables` likewise).
- **Assembly / ordering** (`assembly::assemble_from_nodes_with_packs`): mounted
  packs' bricks are injected at a `content` slot in the template/session tree,
  **grouped by `def_type`**, mount-order across packs and tree-walk order within.
  There is already no priority axis — no depth/weight/priority field exists.
- `assembly::is_non_rendering` = `{regex_rule, first_message}` — these never emit
  into the LLM prompt. `def_type::RESERVED` = `{prompt, regex_rule, tool,
  first_message, protocol}` — reserved types are never user-created containers.

## 1. New brick types

Add three definition types, all **reserved** and **non-rendering**:

| type        | `content`        | `meta`                          |
|-------------|------------------|---------------------------------|
| `html`      | HTML markup      | (none required)                 |
| `css`       | a stylesheet     | (none required) — reusable      |
| `variables` | empty / a note   | `meta.decls: VarDecl[]`         |

Changes:
- `shirita-core/src/models/def_type.rs`: extend `RESERVED` to include `html`,
  `css`, `variables` (array length grows from 5 to 8). Update the
  `reserved_classification` test.
- `shirita-core/src/assembly.rs`: extend `is_non_rendering` to
  `matches!(def_type, "regex_rule" | "first_message" | "html" | "css" |
  "variables")`. This is the single guard that keeps the new bricks out of the
  LLM prompt (they're consumed by the UI/state layer only).

`variables` stores its declarations in `meta.decls` (structured), not `content`
— `content` is the free-text payload convention and `VarDecl[]` is structured
data. A single `variables` brick may declare several names (e.g. "hp, mood").

## 2. Panel = a `panel` folder

A panel is a **folder node** (`kind = 'folder'`) with **`tag = "panel"`**. Its
children are `ref` nodes to `html` / `css` / `regex_rule` / `variables` bricks.

- **Caps** (`write` / `insert` / `send`) live in the folder node's `meta`, e.g.
  `node.meta = { caps: { write: true } }`.
- **Display name:** since all panel folders share `tag = "panel"`, a panel's
  user-facing name is stored in folder `meta.name` (e.g.
  `node.meta = { name: "Status Bar", caps: {...} }`), falling back to "Panel".
- **Rendering** (per panel folder): concatenate enabled `css` children (tree
  order) → one `<style>` payload; concatenate enabled `html` children (tree
  order) → one body. The folder's `regex_rule` children are its sync rules; its
  `variables` children are its declared state.
- **CSS reuse:** a `css` brick may be ref'd into multiple `panel` folders (drop
  the same "neon theme" brick into each). No id-cross-references in meta — reuse
  is expressed structurally by referencing the same definition.

`panel` is a reserved **folder tag**, distinct from the reserved def **types** in
§1. It is not a `DefType` registry row.

## 3. Ordering — no internal priority

- **Within a pack:** bricks carry no priority. Prompt-contributing bricks land
  via the existing type-grouping at the `content` slot; panels render in
  tree-walk order. No depth/priority/weight field is introduced.
- **Across packs:** packs stay an **ordered list** (`session.mounted_packs`,
  mount order) — the only ranking axis. It drives type-group concatenation,
  variable-name conflict resolution (later mount wins, matching today's
  `merge_decls`), and panel stacking order.

This preserves the existing deterministic pipeline; the redesign removes the
god-object meta facets, it does not add a priority system.

## 4. Runtime wiring (backend)

### 4.1 Session panel resolution + endpoint
- New `resolve_session_panels(storage, session) -> Vec<RenderedPanel>` (core;
  place in `shirita-core/src/conversation.rs` next to `effective_nodes` /
  `mounted_pack_trees`, or a small new `panels.rs`). It walks `panel` folders
  across the effective template/session tree **and** mounted-pack trees, resolves
  each folder's child brick contents, and returns one
  `RenderedPanel { id, name, html, css, caps }` per folder (css children joined,
  html children joined, caps from folder meta). `id` = the folder node id;
  `name` = folder `meta.name`, falling back to the first html brick's name, then
  "Panel".
- New route **`GET /sessions/:id/panels`** (`shirita-web/src/routes/`) returning
  `Vec<RenderedPanel>` as JSON. The frontend stays dumb: it binds live `values`
  in `PanelView` exactly as today (the endpoint returns raw `{{var}}` html/css,
  not interpolated).

### 4.2 Variables schema from bricks
- `conversation::session_schema` and `state::resolve_schema_with_packs` /
  `resolve_schema` switch from reading `*.meta.variables` to collecting
  `VarDecl` from `variables` bricks' `meta.decls` found in the effective trees
  (template/session) and mounted-pack trees. Same merge precedence: template
  first, then packs in mount order; later wins on name conflict.
- `state::parse_decls` keeps parsing a `VarDecl[]` value; callers feed it
  `def.meta.decls` instead of `meta.variables`.

### 4.3 Regex sync unchanged
`effective_regex_rules` and `capture_panel_updates` are unchanged in mechanism —
they already operate on `regex_rule` bricks in the effective + mounted-pack
trees. Panel-sync rules simply live inside `panel` folders now.

### 4.4 Portable export/import
- The pack envelope (`portable::export_pack` / parse) already carries bricks as
  `definitions` + `nodes` via `inline_subtree` — **no envelope change**. Panel
  folders and their child refs travel automatically.
- `portable::collect_pack_assets` switches from scanning
  `manifest.pack.meta.panel.{html,css}` to scanning the **`content`** of each
  `manifest.definitions[]` whose `type ∈ {html, css}` for `/assets/<path>`
  occurrences (reuse `ASSET_REF_RE`). The identity-avatar and per-def
  `meta.avatar` scans are unchanged.
- `portable::rewrite_pack_assets` drops its `pack.meta.panel.{html,css}` rewrite
  loop and instead rewrites the `content` of each `html`/`css` definition in
  `manifest.definitions[]` (same `re.replace_all` with blank-on-unmapped, applied
  to `def.content` instead of `pack.meta.panel.<key>`). The avatar remaps are
  unchanged.

### 4.5 charcard importer
- `adapters/charcard.rs::try_convert_status_panel` / the loreset builder emit a
  `panel` folder into the pack tree containing: an `html` brick (the converted
  markup), a `css` brick (the gathered `<style>`), a `variables` brick
  (`meta.decls` from the status-bar fields), and the existing `regex_rule`
  brick(s) for capture-sync — instead of writing `meta.panel` + `meta.variables`.
- `PanelConversion` and the import-summary "panel item" reporting stay; only the
  output shape (bricks vs. meta) changes.

### 4.6 pack.meta
`pack.meta` stops being used for `panel`/`variables`. Leave the column in place
(inert, defaults `{}`) — no table rebuild for the testing-phase DB. `Pack::new`
already seeds `meta = {}`.

## 5. Frontend overhaul

- **`PackEditor.vue` collapses to: identity + one unified brick tree.** Delete
  the dedicated "Panel" section (html/css textareas, caps checkboxes, preview)
  and the separate "Variables" section, plus their `savePanel` / `saveVars` /
  `panelHtml/Css/Caps` state. The pack editor becomes: `AssetPicker` +
  display-name + `PromptTree`. (Matches the single-column / no-separate-sections
  / select-then-reveal UI preference.)
- **Adding bricks:** `NodePicker` / `PromptTree` gain `html`, `css`, `variables`,
  `regex` as creatable types, plus a one-click **"Add panel"** that scaffolds a
  `panel`-tagged folder pre-filled with a blank `html` + `css` brick.
- **`DefinitionEditor.vue`** gains per-type blocks (it already dispatches on
  `definition.type`): `html` (code textarea + live `PanelView` preview), `css`
  (code textarea), `variables` (reuse `VariablesEditor` bound to `meta.decls`),
  `regex` (reuse `RegexRuleEditor`). A `panel` folder shows a name field
  (`meta.name`) + caps toggles (`meta.caps`) + a combined `PanelView` preview
  (its child html+css, bound to the pack's `variables`-brick decls at initial
  values).
- **`ChatView.vue`** stops reading `pack.meta.panel`. `loadPanels` calls
  `getSessionPanels(sessionId)` and renders the returned stack; `onPanelAction`
  reads caps from the response item.
- **`api/types.ts` / `api/client.ts`:** remove `Panel`/`PanelCaps` from
  `pack.meta` typing; add `SessionPanel { id, name, html, css, caps }` and
  `getSessionPanels(sessionId): Promise<SessionPanel[]>`; type `variables` defs'
  `meta.decls: VarDecl[]`.

## 6. Testing (TDD)

Backend (Rust):
- `def_type`: `html`/`css`/`variables` are reserved; `panel` is not a def type.
- `assembly::is_non_rendering` true for the three new types → not emitted into
  the prompt (extend an existing assembly test that asserts content omission).
- `resolve_session_panels`: a `panel` folder with multiple css/html children →
  one combined `{html, css}`; caps from folder meta; multiple folders across
  template + mounted packs → correct count and order.
- schema from `variables` bricks: template + pack `variables` decls merge with
  mount-order precedence; conflict → later wins.
- charcard import: a status-bar card converts to a `panel` folder + html/css/
  variables/regex bricks (adapt existing `try_convert_status_panel` tests).
- portable round-trip: export a pack with a panel folder, re-import → bricks +
  folder reconstructed; `collect_pack_assets` finds `/assets/` refs in html/css
  brick content.

Frontend (Vitest):
- `PackEditor` renders only identity + tree (no panel/variables sections).
- `DefinitionEditor` shows html/css/variables editors for those types.
- `ChatView` renders panels from `getSessionPanels` (mock the client).

## 7. Non-goals

- No priority/depth/weight system (explicitly rejected).
- No flattening of packs to bare definition-sets (Approach B in brainstorming) —
  packs keep an organizational tree.
- No data migration (testing phase, no users).
- No change to the activation/world-info scan pipeline, summaries, or provider
  adapters.
