# Pack Import Button + Web Binary Release CI Design

> Two small M9 follow-ups from desktop testing, batched: (#1) the Pack section is missing an Import button, and (CI) the standalone `shirita-web` binary is only ever built inside the Dockerfile — there's no release workflow that produces it as a downloadable artifact.

## 1. Goals

1. **Pack Import button** — give the Pack section an Import affordance whose button order and visual style match the Template and Definitions sections exactly.
2. **Web binary release CI** — a GitHub Actions workflow that builds the `shirita-web` release binary with the embedded UI for Linux/macOS/Windows and uploads each as an artifact, triggered on `v*` tags like the existing docker/desktop workflows.

These are independent (frontend vs. CI) and share no code; they are batched only because both are small. Each gets its own task in the plan.

## 2. Piece 1 — Pack section Import button

### Current state
`shirita-ui/src/views/BookView.vue` Pack section toolbar (lines ~1063-1068) is wrapped in `v-if="selectedPack"` and contains, in order: rename (`Pencil`) / duplicate (`Copy`) / export (`Download`, `data-test="pack-export"`) / delete (`Trash2`). There is **no** Import button. Export already exists.

The two reference sections both use the order **rename → import → export → duplicate → delete** with identical styling (`w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg`; delete uses `hover:text-coral`):
- Template header — `BookView.vue:920-968`. All buttons always rendered; selection-dependent ones use `:disabled="!selectedTemplateId"` + `disabled:opacity-40`; import uses `:disabled="importBusy"`.
- Definitions — `DefinitionEditor.vue:161-165` (`data-test` `rename-btn`/`import-btn`/`export-btn`/`delete-btn`).

### Change
Restructure the Pack toolbar to mirror the Template header:
- Render the button row **always** (drop `v-if="selectedPack"` on the toolbar `<div>`), so Import is reachable when no pack is selected — import *creates* a pack.
- Button order becomes **rename → import → export → duplicate → delete**.
- Selection-dependent buttons (rename/export/duplicate/delete) get `:disabled="!selectedPack"` and the `disabled:opacity-40` class. Their click handlers already guard internally (`if (!selectedPack.value) return;`), so disabling is purely visual alignment.
- **Import button:** `Upload` icon (size 16), `data-test="pack-import"`, `:title="$t('common.import')"`, `:disabled="importBusy"`, `@click="importInput?.click()"`. It reuses the existing shared hidden `<input ref="importInput">` (already used by the Template header and `DefinitionEditor`).
- The `<PackEditor v-if="selectedPack">` body below the toolbar is unchanged (reveal-on-select stays).

### Why no script changes
The backend already sniffs `.zip` pack bundles (PK magic) and `runImport` already calls `library.loadAll()` (which reloads packs), so an imported pack appears immediately. The new button only adds a third trigger for the existing `importInput`; no new handler, no new state.

### i18n
Reuse the existing `common.import` key (already used by `DefinitionEditor`'s import button) for the tooltip — no new keys, so locale parity is unaffected. The shared input imports any supported type (png/json/zip), so a generic "Import" label is accurate.

## 3. Piece 2 — Web binary release CI

### Current state
- `.github/workflows/docker.yml` — builds & pushes the Docker image to ghcr on `v*` tags; the web binary is built *inside* the Dockerfile.
- `.github/workflows/desktop.yml` — builds Tauri installers (Linux/macOS/Windows) on `v*` tags, uploads artifacts. Installs webkit2gtk/GTK Linux deps for Tauri's webview.
- No workflow produces the standalone `shirita-web` binary as an artifact.

### New workflow `.github/workflows/web.yml`
Mirrors `desktop.yml`'s shape, minus the GTK/webkit deps (the web server is plain Axum — no webview):
- **name:** `web-build`
- **Triggers:** `workflow_dispatch: {}` + `push` on tags `v*`.
- **Matrix:** `ubuntu-latest`, `macos-latest`, `windows-latest`; `fail-fast: false`.
- **Steps:**
  1. `actions/checkout@v4`
  2. `dtolnay/rust-toolchain@stable`
  3. `actions/setup-node@v4` (node 20)
  4. `npm ci` then `npm run build` in `shirita-ui` (produces `shirita-ui/dist`, the rust-embed source folder).
  5. `cargo build --release -p shirita-web --features embed-ui`.
  6. `actions/upload-artifact@v4`: name `shirita-web-${{ matrix.platform }}`, path lists both `target/release/shirita-web` and `target/release/shirita-web.exe`, `if-no-files-found: ignore` (one path matches per OS).

The binary is self-contained: `embed-ui` bakes `shirita-ui/dist` into the executable via rust-embed, so the artifact needs no sidecar files. `shirita-web` is the package name, hence the binary name.

## 4. Testing

- **Pack Import button:**
  - A frontend test asserting the Pack section renders a `pack-import` button and that clicking it triggers the shared file `<input>` (matching how the existing harness tests `pack-export` / `import-btn`). The exact mechanism (Vitest component test vs. existing e2e) is matched to the current test setup during planning.
- **Web binary CI:**
  - Validate `web.yml` is well-formed YAML and structurally mirrors `desktop.yml` (triggers, matrix, artifact step).
  - Locally re-run the exact build commands the workflow uses — `npm run build` (in `shirita-ui`) then `cargo build --release -p shirita-web --features embed-ui` — and confirm a `shirita-web` binary is produced (this command path was already verified once in M9; re-running guards the workflow's command line).
  - Final real verification is a manual `workflow_dispatch` run on GitHub.

## 5. Out of scope

- A push/PR CI gate running `cargo test` on every commit (the repo has none today) — explicitly declined for this batch.
- The ST regex/JS frontend-compat work (#4), still deferred.
- Packaging the web binary into an archive, checksums, or attaching to a GitHub Release — the artifact upload is sufficient for now.
- Any change to `docker.yml` / `desktop.yml`.

## 6. Decomposition

One plan, two independent tasks:
- **Task 1 — Pack Import button** (`BookView.vue` toolbar restructure + test).
- **Task 2 — `web.yml` workflow** (new file + local build-command verification).
