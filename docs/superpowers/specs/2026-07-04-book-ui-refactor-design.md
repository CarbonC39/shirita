# Book UI Refactor — Design

> Replace `BookView.vue`'s 1285-line god-view (local session overrides + global
> library + full CRUD + import/export + copy-on-write, all vertically tiled)
> with a **single-viewport, drill-down** UI. Two modes share one `/book` route:
> **Library** (browse/manage packs, templates, definitions one entity type at a
> time via a dropdown) and **This chat** (drill into the session's prompt —
> Template root → mounted pack → definition — via an in-component navigation
> stack). The drill-down kills the "nested accordion hell" of the current local
> section while preserving the existing copy-on-write override semantics.
>
> This is a **frontend-only** refactor. No backend, data-model, or assembly
> changes. The project is in testing with no users, so existing component tests
> are rewritten freely (no backward-compat surface).

## 0. Background — current state (verified in code)

- **Routing**: `BookView` is a standalone `/book` route (`src/router/index.ts`),
  rendered through `<router-view>` in `App.vue`. The shell `AppShell.vue` is a
  3-tab nav (Chat / Book / Settings). Book is **not** a panel inside chat.
- **`BookView.vue` is 1285 lines** doing: (a) a per-session LOCAL section
  (template subtree + pack subtree + definition-override chips + variables),
  (b) a GLOBAL library section (template + pack + definition editors, tiled),
  (c) full CRUD/import-export for all three entities, (d) copy-on-write
  materialization. This is the god-view being decomposed.
- **Customize gate**: `customizedLocally` (ref, defaults `false`) gates the local
  editing surface behind a "Customize locally" button. Clicking it runs
  `materializeAll()` → `ensureMaterialized()` / `ensurePackMaterialized()`,
  which copy template/pack nodes into `owner_kind='session'` nodes.
- **Packs are NOT tree children.** A session mounts packs via
  `session.mounted_packs: string[]` (`src/api/types.ts`); their nodes live in
  separate `owner_kind='pack'` trees and are merged in at assembly time. So the
  "pack chips under the Template root" in This-chat mode render
  `mounted_packs`, not tree children.
- **Override data**: `session.override_config.local_definitions` (per-def
  patches by id) and `override_config.local_variables` (`VarDecl[]`). APIs:
  `setLocalDefinition`, `clearLocalDefinition`, `promoteLocalDefinition`,
  `materializeNodes`, `materializePackNodes`.
- **Components are already well-separated and reusable**: `PromptTree`,
  `PackEditor`, `DefinitionEditor`, `VariablesEditor`, `EntityPicker`,
  `AssetPicker`, `NodeRow`, `RegexRuleEditor`, `TriggerEditor`. The refactor
  recomposes these; it does not rewrite them.
- **No drill-down / stack nav exists today.** Navigation is flat route-based.
  There is a `<transition name="page" mode="out-in">` wrapper (`App.vue`) and
  `.expand` CSS transitions we can lean on.

## 1. Goals & non-goals

**Goals**

- Escape the god-view: decompose `BookView` into focused components.
- Single-viewport focus: one context renders at a time (no vertical tiling of
  Template + Pack + Definition editors, no nested accordions).
- Drill-down via an in-component navigation stack (view replacement + breadcrumb
  back), not URL routing — preserves chat-page external state.
- Preserve the existing copy-on-write override model (This-chat edits are
  session-local; global entities edited in Library).
- Reuse existing editor components unchanged where possible.

**Non-goals**

- No backend, storage, data-model, or assembly changes.
- No URL-router-based navigation for the drill-down levels.
- No changes to `ChatView` (Book stays its own route/tab).
- No local override of **pack identity** (avatar/display name) — identity stays
  global; only the node tree + definition content/meta are locally overridable,
  matching today's model.

## 2. Surface model

`BookView` becomes a thin shell holding **mode state** and the **customize gate**:

- **Default = Library mode.** Shown when (a) no chat is active, or (b) a chat is
  active but the user has not customized.
- **Customize gate (unchanged intent).** When a chat is active, Library mode
  shows a "Customize locally" affordance. Clicking it:
  1. materializes the session template tree (`materializeNodes`),
  2. switches to **This-chat mode**,
  3. hides the Library view.
  This preserves the current "click to reveal local, hide global" behavior.
- **Toggle back.** From This-chat mode the user can return to Library (e.g. a
  mode switch control). Toggling does **not** discard overrides — the session's
  materialized nodes and `local_definitions` persist in the DB; re-entering
  This-chat shows them again without re-materializing.
- A session that already has overrides when Book opens: Library is still the
  landing view; the customize affordance reflects "this chat has local edits"
  and entering This-chat shows them (no re-materialize).

## 3. Library mode — `LibraryBrowser`

Replaces the three tiled global editors with **one entity type at a time**:

```
[ Packs ▼ ]                 <- entity-type switcher (Packs / Templates / Definitions)
[ search...        ] [+ New]
[ item A ]
[ item B ] (selected)       <- list
------ edit surface below ------
<editor for selected item>  <- PackEditor / template editor / DefinitionEditor
```

- Top-level entity selection is a **dropdown switcher** (not drill-down). Picking
  Packs/Templates/Definitions swaps the list + editor below. This is what kills
  the three-way tiling: only one entity type is on screen.
- Clicking a list item shows its editor below (current EntityPicker pattern).
- Editing a definition from within a pack/template tree still drills into
  `DefinitionView` — Library keeps a **shallow editor ⇄ definition-view** drill
  with a single back action; it does **not** need the full multi-level stack.
  The stable reuse seam between modes is the level components
  (`PackView`/`DefinitionView` with `editTarget`); the two modes differ only in
  **root** (entity list vs session template tree) and **edit target** (global vs
  local).

## 4. This-chat mode — drill-down (`BookNavigator`)

An in-component navigation stack, **not** URL routing:

- `navigationStack: Ref<Target[]>` lives in `BookNavigator`. `Target` describes a
  level: `{ kind: 'sessionRoot' } | { kind: 'pack', packId } | { kind:
  'definition', definitionId, ownerId, ownerKind }`.
- **Stack invariant**: `stack[0]` is always the session template root
  (`{ kind: 'sessionRoot' }`). Only `stack[stack.length-1]` renders (view
  replacement).
- **Push (drill-in):** clicking a mounted pack chip pushes `{ kind:'pack',
  packId }`; clicking a definition-bearing tree node pushes `{ kind:'definition',
  ... }`. The view replaces.
- **Pop (back):** the breadcrumb `< back` does `stack.pop()`. When the stack
  returns to length 1, the session root is shown.
- **Levels**:
  - **L0 `SessionTemplateRoot`** (this-chat only) — see §6.
  - **L1 `PackView`** — pack identity (read-only in local mode) + the pack's
    node tree (editable). Shared component (§7).
  - **L2 `DefinitionView`** — full-width definition editor. Shared (§7).

```
L0  SessionTemplateRoot
      mounted packs:  [ girl ] [ boy ] [ + ]   -- click girl pushes L1
L1  ‹ back to Template        (PackView, editTarget=local)
      Pack: girl
        char/ ▸ char-setting   -- click pushes L2
L2  ‹ back to Pack (girl)     (DefinitionView, editTarget=local)
      char-setting  [ full-width editor ]
```

## 5. Override semantics & materialization

- **This-chat mode = override mode.** Every edit writes to session-local data:
  definition content/meta → `setLocalDefinition`; tree node changes →
  `updateNode` on the `owner_kind='session'` materialized nodes.
- **Library mode = global edits.** Edits hit the shared entity (`updatePack`,
  `updateDefinition`, `updateNode` on `owner_kind='pack'`/`'template'`).
- **editTarget prop.** `PackView` and `DefinitionView` accept `editTarget:
  'global' | 'local'`. Same rendering, different save targets. This is the reuse
  seam between the two modes.
- **Materialization stays explicit at the gate** (per the customize button) for
  the **template tree**. **Pack nodes materialize lazily** — the first time the
  user drills into a mounted pack in This-chat mode, `materializePackNodes` runs
  for that pack only (an improvement over today's "materialize all"). Already-
  materialized packs are not re-materialized.
- **Revert to global.** `DefinitionView` in local mode, when editing a
  definition that has a `local_definitions` patch, shows a "revert to global"
  action (`clearLocalDefinition`).
- **No separate "changed in this chat" chip strip.** That strip was an artifact
  of the tiled design. In the drill-down, overridden nodes simply show a small
  marker in the tree, and This-chat mode as a whole is the override context.

## 6. L0 — `SessionTemplateRoot` layout

- The session's materialized template tree (`PromptTree` bound to
  `owner_kind='session'` nodes), with inline enable/disable toggles (existing
  `toggleEnabled` semantics).
- **Mounted pack chips** rendered from `session.mounted_packs` (add/remove via
  existing mount APIs); clicking a chip pushes L1.
- **Per-chat Variables editor** (`VariablesEditor`) as a collapsible section on
  L0. Variables are a sibling concern (not part of the template→pack→definition
  hierarchy), so they stay on the root rather than entering the nav stack.
- Tree nodes whose definition has a local override show a small marker so the
  user can see what's customized at a glance.

## 7. Component decomposition

```
BookView (thin shell)
├── mode state (library | this-chat) + customize gate
├── <LibraryBrowser>            // mode === library
└── <BookNavigator>             // mode === this-chat
     ├── navigationStack: Ref<Target[]>
     ├── breadcrumb (pop)
     ├── <Transition name="drill">
     │     └── top-of-stack:
     │          SessionTemplateRoot (L0, this-chat only)
     │          PackView            (L1, shared, editTarget)
     │          DefinitionView      (L2, shared, editTarget)
     └── (level components own global-vs-local save wiring)
```

- **Reuse, don't rewrite**: `PromptTree`, `EntityPicker`, `AssetPicker`,
  `VariablesEditor` are embedded unchanged. `PackEditor` and `DefinitionEditor`
  get a **small `editTarget`-aware refactor** so their save handlers route to
  global vs local APIs; `PackView` / `DefinitionView` are the thin
  breadcrumb + mode wrappers around them. (Exact prop/event wiring is an
  implementation-plan detail; the contract is: editors stay presentational, the
  level component decides the save target.)
- **Orchestration differs by mode.** `BookNavigator` orchestrates the
  **This-chat** multi-level stack (L0→L1→L2). **Library** composes the same
  level components (`PackView`/`DefinitionView`) directly behind its
  dropdown+list, with only the shallow editor⇄definition back action — no
  full nav stack. The level-component contract (`editTarget` + presentational
  editors) is the stable seam; the two orchestrators may be unified in Phase 3.
- **Why not alternatives**: route-per-level (rejected by requirement — breaks
  chat-page state); keep single-file `BookView` (defeats the refactor's purpose).

## 8. Transitions

- **Drill**: push → slide new level in from the right; pop → slide back to the
  left. Implemented with Vue `<Transition :name="driftDir">` + CSS `translateX`.
- **Mode switch** (Library ↔ This-chat): a simple fade, consistent with the
  existing `<transition name="page">`.
- Transition direction is derived from stack push/pop, not from absolute level,
  so back always feels like "back."

## 9. i18n

New keys added to `en` (source schema) with parity in `zh-Hans`, `zh-Hant`,
`ja` — the parity test (`shirita-ui`) must pass:

- `book.mode.library`, `book.mode.thisChat`
- `book.customizeLocally` (exists), `book.hasLocalEdits` (state hint)
- `book.nav.back`, `book.nav.backToTemplate`, `book.nav.backToPack`
- `book.library.entityType.packs` / `.templates` / `.definitions`

Labels are built in English; no fixed-width label layouts (per project i18n
guidance).

## 10. Testing

- **`BookNavigator`**: push populates the stack and renders the new level; pop
  restores the previous; breadcrumb depth matches stack length; stack[0] is
  always the session root.
- **`PackView` / `DefinitionView`**: with `editTarget='global'` they call global
  APIs; with `'local'` they call session-override APIs and show "revert to
  global" only in local mode.
- **`LibraryBrowser`**: entity-type switching swaps list + editor; selecting an
  item shows its editor; drill into a definition pushes the stack.
- **`SessionTemplateRoot`**: renders materialized tree + mounted pack chips +
  variables editor; chip click pushes a pack target.
- **Migrate `BookView.test.ts`**: existing tests assert the tiled structure and
  are rewritten against the new components (no users → no compat).
- Gates: `vue-tsc --noEmit` clean; `vitest` green.

## 11. Phased migration

The 1285-line file is rewritten in three risk-bounded phases; each phase ships
green (tests + tsc).

- **Phase 1 — This-chat drill-down.** Extract `BookNavigator` + `L0/L1/L2` from
  the current LOCAL section; wire the nav stack + transitions; give
  `PackEditor`/`DefinitionEditor` the `editTarget` refactor. Replace the tiled
  local section with the drill-down. Leave the GLOBAL section as-is temporarily.
  Rewrite affected `BookView.test.ts` cases.
- **Phase 2 — Library browser.** Refactor the three global editors into
  `LibraryBrowser` (entity dropdown + list + editor-below). Migrate their CRUD
  handlers.
- **Phase 3 — Unify & clean up.** Confirm `PackView`/`DefinitionView` are shared
  across both modes via `editTarget`; delete the old tiled `BookView` template;
  final test/typecheck pass.

## 12. Out of scope / to revisit

- **Pack identity local override** — not supported (identity is global). Revisit
  if per-chat avatar/name override is needed.
- **L0 density** — variables editor + tree + pack chips on one screen; if it
  reads as overloaded after Phase 1, move variables behind a drill entry.
- **Animation polish** — slide timing/easing tuned in Phase 1, not before.
