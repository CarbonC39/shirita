# Pack Zip Bundle (export / import with assets) — Design

> Point 1 of the export rework. A **Pack** can be exported as a self-contained **zip bundle** — its `manifest.json` plus every binary asset it transitively references (avatars, panel images, backgrounds) — and re-imported plug-and-play. Definitions and templates keep their existing JSON export (no binary). Assets are **content-hash deduplicated** on import, consistent with the project's reference-not-copy principle.

## 1. Motivation

Today's export (M7 `portable.rs`) is **JSON-only**: `export_definition` / `export_template` emit text envelopes and drop every image. There is also **no Pack export at all** (it was deferred from the pack/preset spec to here). Now that a Pack carries real binary — `identity.avatar`, panel `/assets/…` images, and inlined char/persona definitions' `meta.avatar` — sharing a character means shipping its images. A zip bundle does that; importing it elsewhere reproduces the pack exactly.

## 2. Scope

- **In:** Pack export → zip (or plain JSON when the pack has zero binary); Pack import (zip and JSON); asset content-hash dedup infrastructure.
- **Out (unchanged):** Definition / Template export stay JSON (`shirita.definition` / `shirita.template`). No full-library zip, no ST-card zip.

## 3. Container format

A pack export is a `.zip` named `<pack-name>.zip` containing:

```
manifest.json          # the shirita.pack envelope
assets/<file>          # every binary the pack transitively references
```

- `manifest.json` — `{ "format": "shirita.pack", "version": 1, "pack": { name, identity, meta }, "nodes": [...], "definitions": [...] }`. `nodes` + `definitions` mirror `export_template`'s `local_id` indirection (definitions inlined, referenced by `def_local_id`), plus the pack's `identity` (avatar + display_name), `meta` (variables + `panel`), and any bound `regex_rule` defs are already inline in `definitions` (they're nodes in the pack tree).
- `assets/<file>` — each referenced asset stored under its current relative filename (the `<uuid>.png` already used in `assets_dir`). The manifest keeps the **same relative ref strings**; import remaps them.

**Degradation:** a pack with **no** binary refs exports as a plain `.json` (`shirita.pack` manifest, no zip). The server sets `Content-Disposition` with the right extension; the frontend honors it.

## 4. Asset reference discovery

"Every binary the pack transitively references" = scan the assembled manifest for asset relative-paths in these places:

1. `pack.identity.avatar`.
2. Each **inlined definition**'s `meta.avatar` (char / persona).
3. The pack panel `meta.panel.html` + `meta.panel.css` — `/assets/<path>` occurrences.
4. Pack `meta.variables` initials (and any meta field) whose value is a string naming an asset (e.g., a default `$background`) — matched as a known asset path.

Discovery returns the set of distinct relative paths; only those whose file exists under `assets_dir` are bundled (missing refs are warned + skipped, like dangling `definition_id` in `export_template`).

## 5. Asset content-hash dedup (reference-not-copy)

To avoid storing the same image twice, assets gain a content hash:

- **Schema:** add `hash TEXT` (sha256 hex of the file bytes) to the `assets` table (migration, nullable).
- **Populate on every save:** the shared asset-store path (manual upload, PNG-card import, zip import) computes and records the hash.
- **Startup backfill:** a `ensure_asset_hashes` step (alongside the existing `ensure_*` startup calls) fills `hash` for any asset row missing one by reading its file — so dedup also covers pre-existing assets.
- **Storage:** add `find_asset_by_hash(hash) -> Option<Asset>`.

On **import**, for each bundled asset: compute its hash; if an existing asset has the same hash, **reuse that asset's path** (rewrite the ref to point at it — no new file/record); otherwise store a fresh asset (new uuid file + record + hash). One image, one copy in the library; every pack references it.

## 6. Export flow (backend)

- `portable::export_pack(pack, nodes, defs) -> Value` — produce the `shirita.pack` manifest (mirrors `export_template`, adds `identity` + the pack `meta`).
- A web-layer packer collects the referenced assets (§4), and:
  - **assets present** → build a zip in memory (the `zip` crate): write `manifest.json` + each `assets/<file>` (bytes read from `assets_dir`); respond `application/zip`, `Content-Disposition: attachment; filename="<name>.zip"`.
  - **no assets** → respond the manifest JSON, `filename="<name>.json"`.
- Endpoint: `GET /api/packs/{id}/export`.

## 7. Import flow (backend)

Extend the existing `POST /api/import` content sniff:

- Leading bytes `PK\x03\x04` → **zip branch**: open the archive, read `manifest.json` (must be `shirita.pack`) + the `assets/` entries.
  1. For each `assets/` entry: hash its bytes → dedup (§5) → build an `oldRelPath → newRelPath` map.
  2. Rewrite the manifest's asset refs (identity.avatar, inlined defs' meta.avatar, panel html/css `/assets/…`, asset-valued variables) through the map.
  3. **Create a new pack** (always additive — never overwrite by name), then topologically insert its inlined definitions + nodes (parent-before-child, reusing the M7 bundle-restore approach that avoids the self-referential FK ordering bug).
- A `shirita.pack` **JSON** (no zip, no assets) imports the manifest directly through the same path (empty asset map).
- Returns the existing `ImportSummary` shape (the created pack in `created`).

The `definition` / `template` / PNG / worldinfo sniff branches are unchanged.

## 8. Frontend

- **Export:** the Book pack ops row (next to rename / duplicate / delete) gains an **Export** button → `downloadExport('/packs/{id}/export', …)`.
- **`downloadExport`** is adjusted to take the filename from the response's `Content-Disposition` (so it lands `.zip` or `.json` correctly, since the server decides per-pack).
- **Import:** reuse the existing Book Import button (`POST /api/import`, multipart) — add `.zip` to the file-input `accept`. The import summary already surfaces the created pack.

## 9. Backend pieces / new dependencies

- `zip` crate (read + write archives), `sha2` crate (content hash). Both new workspace deps.
- `shirita-core`: `portable::{export_pack, parse_pack}` + an asset-ref collector (pure, over a manifest `Value`); `models::asset::Asset` gains `hash: Option<String>`.
- `shirita-web`: the zip pack/unpack glue in the export + import routes; `ensure_asset_hashes` startup step (re-exported + called in both `shirita-web` and `shirita-tauri` mains, like `ensure_templates_have_content_node`).
- Migration: `00NN_assets_hash.sql` adds the nullable `hash` column.

## 10. Testing

- **Core (pure):** `export_pack` manifest shape (identity + inlined nodes/defs + variables + panel); asset-ref collector finds avatar + def avatar + panel-css `/assets` + background-var refs; `parse_pack` round-trips.
- **Dedup (integration):** importing two bundles that share an identical image creates **one** asset file (second reuses by hash); `ensure_asset_hashes` backfills.
- **Zip round-trip (integration):** export a pack with an avatar → zip has `manifest.json` + `assets/<file>`; import it → a new pack with the avatar restored (remapped ref) and inlined defs; `$background`/panel refs rewritten.
- **JSON degradation:** a binary-less pack exports as `.json` (not zip) and re-imports.
- **Import sniff:** `PK` magic routes to the zip branch; the existing definition/template/PNG branches still pass.

## 11. Decomposition (for writing-plans)

1. **Asset content-hash infra** — migration + `Asset.hash` + compute-on-save + `find_asset_by_hash` + `ensure_asset_hashes` startup backfill.
2. **Pack portable codec** — `export_pack` / `parse_pack` (`shirita.pack`) + the asset-ref collector (pure core, unit-tested).
3. **Zip export** — `zip`/`sha2` deps, in-memory zip packer, `GET /api/packs/{id}/export` (zip-or-json).
4. **Zip import** — `/api/import` `PK` sniff → unzip + hash-dedup restore + ref rewrite + topological pack create.
5. **Frontend** — Book pack Export button, `.zip` import accept, `downloadExport` Content-Disposition filename.
