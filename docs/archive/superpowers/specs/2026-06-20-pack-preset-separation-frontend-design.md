# Pack/Preset Separation — Frontend Design (Book editor split + new-chat)

> Refines §12 of `2026-06-19-pack-preset-separation-design.md` into concrete frontend UX. The backend (Plans 1–4) and the shared component foundation (Plan 5: `content` row, folder `select` switch, `Pack`/`PackIdentity` types) are already done and merged on this branch. This spec drives **Plan 6 (Book editor split + plumbing)** and **Plan 7 (single-screen new-chat)**.

## 1. Vocabulary (exactly three nouns)

The whole feature uses three user-facing concepts and no metaphors:

- **Definition** — one typed piece of content: a `char` block, a `world` entry, a `first_message`, a `regex_rule`, etc. (`type` + `name` + `content` + `meta`). The atoms.
- **Pack** — a character/world you mount into chats. Holds an **identity** (`avatar` + `display_name`), a **content tree** (`owner_kind=pack`: refs to definitions in folders), and its own **variables** (`pack.meta.variables`). Bound regex = `regex_rule` definitions referenced in its tree (no separate list).
- **Template** — the prompt frame, character-agnostic. Holds a **content tree** (`owner_kind=template`: system/jailbreak prose, the `«Mounted packs»` mount row, the `history` row, post-history) and **variables** (`template.meta.variables`). **No identity.**

UI labels: the definitions list is just **"Definitions"**; the mount row inside a template is **"Mounted packs"** (Plan 5, key `prompt.contentMount`). "Library / bundle / frame / 中间层" are not used anywhere.

Relationship: **Definitions are atoms; a Pack bundles atoms + gives them a face + variables; a Template is the frame with a mount slot.** At chat time, the mounted packs inject into the template's `«Mounted packs»` slot, grouped by definition type.

## 2. Constraints (from the user)

- **Single column, no view-switching** — no tabs, no segmented control, no left rail. Sections stack and you scroll.
- **Search-box pickers** — each editable thing is fronted by the type-to-filter / pick / create-new construction (the PromptTree omnibox / DefinitionEditor search style), not a plain `<select>`.
- The Book is **only for editing/authoring**. Choosing a pack to *use* happens in new-chat / in-chat, never in the Book — so pickers are labeled for editing ("Edit template", "Edit pack").
- English copy; flexible-width labels; i18n keys in all four locales (`en` is source; `parity.test.ts` enforces).
- Reuse existing components (`AvatarPicker`, `PromptTree`, `VariablesEditor`, `DefinitionEditor`, the omnibox pattern). No new heavy components.

## 3. Book view (Plan 6)

One scrolling column. The existing "this-chat overrides" section stays at the very top when inside a chat (unchanged). Below it, three stacked sections in this order.

**Section color-coding (required):** each of the three sections carries a distinct accent (its heading label + a thin left rule), drawn from the existing palette so the eye can tell them apart while scrolling: **Template = `primary` (teal)**, **Pack = `mauve`**, **Definitions = `muted` (neutral)**. The accent is section chrome only (heading + rule); it must not tint the tree contents, so it never collides with the per-type icon tints inside the trees (`char=sky`, `persona=coral`, `world=mauve`, …).

### 3.1 TEMPLATE (top — the essential frame)

Template is the one thing you cannot chat without (a "Default" is auto-seeded), so it leads.

- A **search-box picker**: type to filter existing templates, pick one, or "+ New template". Replaces today's `<select>`. Reveals the editor on select (same reveal pattern as today).
- Row of ops on the selected template: rename · import · export · duplicate · delete (today's buttons, kept).
- Editor body: the **PromptTree** (`owner_kind=template`) — which now shows the `«Mounted packs»` row and `history` row from Plan 5 — plus the template **VariablesEditor** (`template.meta.variables`). All today's tree wiring is reused as-is.

### 3.2 PACK (the new section)

- A **search-box picker**: type to filter existing packs, pick one, or "+ New pack". Reveals the editor on select.
- Row of ops: rename · duplicate · delete (export/import of packs is out of scope for this plan; ST-import→Pack is a later plan).
- Editor body, in order:
  1. **Identity** — `AvatarPicker` (writes `pack.identity.avatar`) + a display-name input (writes `pack.identity.display_name`). Empty fields == unset (Plan 4 treats empty as fall-through).
  2. **Content tree** — `PromptTree` bound to `owner_kind=pack` CRUD, mirroring how `BookView` wires the template tree (`listNodes('pack', id)`, `createNode('pack', …)`, `updateNode`, `deleteNode`, `reorderNodes('pack', …)`, inline content/trigger edits write the referenced *global* definition, same as the template tree does today).
  3. **Variables** — `VariablesEditor` bound to `pack.meta.variables`, saved via `updatePack`.

### 3.3 DEFINITIONS (bottom)

- The existing `DefinitionEditor` (its own search picker + fields + create/delete/duplicate/import/export/type management). Unchanged except for being relabeled/positioned as the bottom section.

### 3.4 Folder `select=one` — visual + behavior (deferred from Plan 5)

Two parts, both applied to the template and pack trees:

- **Visual (required, component level).** The direct children of a `select=one` folder render their enable control as a **radio button** (round, single-select look), not the default rounded-square checkbox. NodeRow only knows its own node, so `PromptTree` passes a `singleSelect` prop to the child rows it renders under a `select=one` folder; NodeRow switches the enable control's shape on that prop. Children of an `all` folder and root rows keep the square checkbox.
- **Behavior.** Enabling a child in a `select=one` folder disables its currently-enabled siblings (radio-style; deselect is allowed). Wired at the Book/handler level (the layer that owns `toggleEnabled` persistence and the node list). The backend already renders only the first enabled child for `select=one`; this makes the UI single-select intent explicit and deterministic.

## 4. New chat (Plan 7)

Replace the two-view wizard (`NewChatView` + `NewChatPromptView`) with **one screen**:

```
New chat
  name…                         (optional; on create, defaults to the first mounted character pack's name)
  Template      ▼ search-pick   (defaults to the Default / first template)
  Mount packs   ▼ search-add  →  [ Alice × ] [ Lorebook × ]   (removable chips)
  avatar preview (resolved from the first mounted character pack; optional override)
  [ Create ]
```

- **Template**: search-box picker (required-ish; pre-selected to the first template since one is always seeded).
- **Mount packs**: a search-add control that appends chosen packs to an ordered chip list. **Mount order is meaningful** — it drives identity precedence (first character pack wins) and assembly order — so chips are both **removable** and **reorderable** (drag, reusing the same native drag-handle pattern as PromptTree rows). "+ New pack" in this picker routes to the Book's Pack editor (no inline pack authoring).
- **Identity/avatar**: the assistant face comes from the first mounted character pack (Plan 4 `resolve_identity_with_packs`). Show a resolved avatar preview; allow an optional per-chat avatar override (`AvatarPicker`) that becomes the session avatar (the Plan-4 fallback).
- **No inline tree editing** — authoring lives in the Book.
- Create → `createSession(name, templateId, avatar, pack_ids)` → navigate `/chat/:id`.

## 5. Plumbing (Plan 6, shared by both)

- `api/client.ts`: add `listPacks() / getPack(id) / createPack(body) / updatePack(id, body) / deletePack(id) / duplicatePack(id)` and `setSessionPacks(sessionId, packIds)`; extend `createSession` to send `pack_ids` (default `[]`, back-compatible).
- `stores/library.ts`: add `packs` ref + `loadPacks()`; include it in `loadAll()`.
- `api/types.ts`: `Pack` / `PackIdentity` already added in Plan 5.
- Endpoints already exist (Plan 3): `/api/packs`, `/api/packs/{id}`, `/api/packs/{id}/duplicate`, `/api/packs/{id}/nodes(+/reorder)`, `/api/sessions/{id}/packs`, and `POST /api/sessions` accepts `pack_ids`.

## 6. Out of scope (later plans / specs)

- ST character-card → Pack import (later plan; mapping already in the backend spec §11).
- Pack export/import files.
- HTML rendering binding (separate spec per the backend design).
- Multi-character per-message identity rendering beyond the first-char-pack rule.

## 7. Testing

- **Component**: search-box pickers (filter/select/create-new emits); Pack identity editor (avatar + name → `updatePack`); Pack tree wiring (add/toggle/delete/reorder against `owner_kind=pack`); `select=one` children render the radio-style enable control (square for `all`/root); `select=one` mutual exclusion (enabling one emits disables for enabled siblings); new-chat (template select + mount-pack chips add/remove/**reorder**; Create posts `pack_ids` in chip order).
- **Visual-only (no unit test)**: the three Book sections' accent color-coding — verified by build + eyeball, not asserted in tests.
- **i18n**: all new keys in `en/zh-Hans/zh-Hant/ja`; `parity.test.ts` green.
- **Typecheck/build**: `vue-tsc` clean.

## 8. Plan split

- **Plan 6** — Book editor split + plumbing: api/client pack functions + `library.packs`/`loadPacks`; Book PACK section (identity + pack tree + variables); reorder Template above Pack and swap both pickers to search-box; section color-coding; `select=one` radio-style child controls (NodeRow/PromptTree) + mutual exclusion (Book handlers).
- **Plan 7** — Single-screen new-chat: collapse the two views into one (template search-pick + mount-pack chips that are removable + reorderable + optional avatar override), wire `createSession` with `pack_ids` in chip order, route to chat.
