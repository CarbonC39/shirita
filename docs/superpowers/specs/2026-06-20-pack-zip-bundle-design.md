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

## 4. Asset reference discovery (deterministic markers only)

Asset refs are collected **only from designated, typed fields** — never by scanning arbitrary string values. A general "walk every string and match it against existing asset paths" is **rejected on purpose**: a normal text variable whose value happens to be `123.png` or a uuid would be falsely bundled, and worse, **rewritten to a new uuid on import** — silently corrupting the user's data. The deterministic sources:

1. `pack.identity.avatar` — the avatar field.
2. Each **inlined definition**'s `meta.avatar` (char / persona) — the avatar field.
3. The pack panel `meta.panel.html` + `meta.panel.css` — `/assets/<path>` occurrences (the `/assets/` prefix is the marker).

Nothing else is scanned. (A pack-level "background" is therefore carried only when it appears as a panel `/assets/` image — packs have no standalone background field. This is the deliberate trade for not corrupting look-alike text values.)

Discovery returns the distinct relative paths from those fields; only those whose file exists under `assets_dir` are bundled (missing ones are warned + skipped, like a dangling `definition_id` in `export_template`).

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

Extend the existing `POST /api/import` content sniff. Leading bytes `PK\x03\x04` → **zip branch**.

**Archive safety (before reading content):**
- Reject any entry whose name isn't exactly `manifest.json` or `assets/<basename>` — no `..`, no absolute path, no nested subdirs (path-traversal guard). Asset bytes are always re-stored under our own freshly-generated uuid filenames, so the archive's entry paths are never used to write to disk.
- Cap the entry count and the per-entry **and** total **decompressed** size (zip-bomb guard); abort with an error if exceeded. (The multipart upload keeps the existing `DefaultBodyLimit`, which bounds only the compressed size.)

**Restore:**
1. Read `manifest.json` (must be `shirita.pack`) and each `assets/` entry's bytes.
2. For each asset entry: hash its bytes → dedup (§5) → build an `oldRelPath → newRelPath` map.
3. Rewrite the manifest's asset refs through the map — **designated fields only** (identity.avatar, inlined defs' meta.avatar, panel html/css `/assets/…`; §4). **Dead-link guard:** if a designated ref has **no** entry in the map (its asset is absent from the bundle), set that field to **null / empty** instead of keeping the stale ref — so the frontend never fires a 404 for a dead link.
4. **Create a new pack** (always additive — never overwrite by name), then insert its inlined definitions + nodes topologically (parent-before-child, reusing the M7 bundle-restore ordering that avoids the self-referential FK bug).

**Atomicity:** steps 2–4's database writes — new asset records, the pack, its definitions, its prompt_nodes — run inside **one DB transaction** (`BEGIN … COMMIT`, the atomic restore the M7 template import already uses). Any failure rolls the whole thing back; no half-imported pack. (Asset *files* written for new, non-deduped assets before a rollback are harmless orphans.)

A `shirita.pack` **JSON** (no zip / no assets) imports through the same restore with an empty asset map. Returns the existing `ImportSummary` (the created pack in `created`). The `definition` / `template` / PNG / worldinfo branches are unchanged.

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

- **Core (pure):** `export_pack` manifest shape (identity + inlined nodes/defs + variables + panel); asset-ref collector finds **only** the designated refs (identity.avatar + inlined def `meta.avatar` + panel-css/html `/assets/…`) and **ignores a look-alike text variable** (e.g. a var valued `123.png` is not collected); `parse_pack` round-trips.
- **Dedup (integration):** importing two bundles that share an identical image creates **one** asset file (second reuses by hash); `ensure_asset_hashes` backfills.
- **Zip round-trip (integration):** export a pack with an avatar → zip has `manifest.json` + `assets/<file>`; import it → a new pack with the avatar restored (remapped ref) and inlined defs; `$background`/panel refs rewritten.
- **JSON degradation:** a binary-less pack exports as `.json` (not zip) and re-imports.
- **Import sniff:** `PK` magic routes to the zip branch; the existing definition/template/PNG branches still pass.
- **Security (integration):** a bundle with a `../`-traversal entry name is rejected; an over-cap decompressed size (zip bomb) is rejected; nothing is written outside `assets_dir`.
- **Atomicity (integration):** a restore that fails mid-way (e.g., a malformed node) leaves **no** pack / definition / node / asset rows (full rollback).
- **Dead-link guard:** a manifest avatar ref with no matching `assets/` entry imports with that field `null` (no stale `/assets/` ref).

## 11. Decomposition (for writing-plans)

1. **Asset content-hash infra** — migration + `Asset.hash` + compute-on-save + `find_asset_by_hash` + `ensure_asset_hashes` startup backfill.
2. **Pack portable codec** — `export_pack` / `parse_pack` (`shirita.pack`) + the asset-ref collector (pure core, unit-tested).
3. **Zip export** — `zip`/`sha2` deps, in-memory zip packer, `GET /api/packs/{id}/export` (zip-or-json).
4. **Zip import** — `/api/import` `PK` sniff → archive safety (traversal + zip-bomb caps) → unzip + hash-dedup restore + designated-field ref rewrite (+ dead-link blanking) + topological pack create, all in **one DB transaction** (atomic rollback).
5. **Frontend** — Book pack Export button, `.zip` import accept, `downloadExport` Content-Disposition filename.
