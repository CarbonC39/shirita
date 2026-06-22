# Pack Import Button + Web Binary Release CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an Import button to the Pack section (matching the Template/Definitions sections) and a CI workflow that builds the standalone `shirita-web` binary (embedded UI) for Linux/macOS/Windows on `v*` tags.

**Architecture:** Frontend — restructure the Pack toolbar in `BookView.vue` to always-render its buttons in the canonical order (rename → import → export → duplicate → delete), the Import button reusing the existing shared `importInput`. CI — a new `web.yml` mirroring `desktop.yml` minus GTK deps, building the Linux target against musl for a static (glibc-free) binary, with a Rust cache action.

**Tech Stack:** Vue 3 `<script setup>`, Vitest + `@vue/test-utils`, GitHub Actions (`dtolnay/rust-toolchain`, `Swatinem/rust-cache@v2`, `actions/setup-node@v4`, `actions/upload-artifact@v4`), Rust `x86_64-unknown-linux-musl`.

## Global Constraints

- Code comments and git commit messages in **English**; end every commit with `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- Build UI strings via i18n keys; **no new locale keys** in this work (reuse `common.import`).
- Pack toolbar button order, verbatim from the spec: **rename → import → export → duplicate → delete**, styled `w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg` (delete uses `hover:text-coral`; selection-dependent buttons add `disabled:opacity-40`).
- Web binary: build with `--features embed-ui`; Linux target `x86_64-unknown-linux-musl` (needs `musl-tools`); triggers `workflow_dispatch` + push `v*` tags only (no push/PR test gate).

---

## File Structure

- `shirita-ui/src/views/BookView.vue` — **modify**. Pack toolbar block (lines ~1063-1068): drop `v-if="selectedPack"` on the button row, reorder, add the Import button.
- `shirita-ui/src/views/BookView.test.ts` — **modify**. Add a test for the pack-import button presence + click wiring.
- `.github/workflows/web.yml` — **create**. The `web-build` workflow.

---

## Task 1: Pack section Import button

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue` (Pack toolbar, currently lines ~1063-1068)
- Test: `shirita-ui/src/views/BookView.test.ts`

**Interfaces:**
- Consumes (already present in `BookView.vue`): `importInput` ref (`<input ref="importInput" type="file" …>`), `importBusy` ref, `selectedPack` computed, `Upload` icon import (line 4), and handlers `startRenamePack` / `exportSelectedPack` / `dupPack` / `delPack`.
- Produces: a `[data-test="pack-import"]` button wired to `importInput?.click()`.

- [ ] **Step 1: Write the failing test**

In `shirita-ui/src/views/BookView.test.ts`, add this test inside the `describe('BookView scopes', …)` block (after the existing `'shows the Pack section …'` test, before the closing `})`):

```ts
  it('renders a pack Import button that triggers the shared file input', async () => {
    const ui = useUiStore(); ui.setActiveChatId(null)
    const w = mount(BookView)
    await flushPromises()
    // Import lives in the Pack section even with no pack selected (it creates one).
    const btn = w.find('[data-test="pack-import"]')
    expect(btn.exists()).toBe(true)
    // Clicking it opens the shared hidden file input.
    const input = w.find('input[type="file"]').element as HTMLInputElement
    const clickSpy = vi.spyOn(input, 'click').mockImplementation(() => {})
    await btn.trigger('click')
    expect(clickSpy).toHaveBeenCalled()
  })
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/views/BookView.test.ts -t "pack Import button"`
Expected: FAIL — `[data-test="pack-import"]` does not exist (`btn.exists()` is `false`).

- [ ] **Step 3: Restructure the Pack toolbar**

In `shirita-ui/src/views/BookView.vue`, replace the current button row:

```vue
                    <div v-if="selectedPack" class="flex items-center">
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" :title="$t('common.rename')" @click="startRenamePack"><Pencil :size="15" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" :title="$t('common.duplicate')" @click="dupPack"><Copy :size="16" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" :title="$t('book.exportPackTitle')" data-test="pack-export" @click="exportSelectedPack"><Download :size="16" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-coral rounded-lg" :title="$t('common.delete')" @click="delPack"><Trash2 :size="16" /></button>
                    </div>
```

with the always-rendered, reordered row (rename → import → export → duplicate → delete), mirroring the Template header:

```vue
                    <div class="flex items-center">
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40" :title="$t('common.rename')" :disabled="!selectedPack" @click="startRenamePack"><Pencil :size="15" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40" :title="$t('common.import')" data-test="pack-import" :disabled="importBusy" @click="importInput?.click()"><Upload :size="16" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40" :title="$t('book.exportPackTitle')" data-test="pack-export" :disabled="!selectedPack" @click="exportSelectedPack"><Download :size="16" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40" :title="$t('common.duplicate')" :disabled="!selectedPack" @click="dupPack"><Copy :size="16" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-coral rounded-lg disabled:opacity-40" :title="$t('common.delete')" :disabled="!selectedPack" @click="delPack"><Trash2 :size="16" /></button>
                    </div>
```

(The `<PackEditor v-if="selectedPack" …>` line immediately below is unchanged — its reveal-on-select behavior stays.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/views/BookView.test.ts`
Expected: PASS — all BookView tests green, including the new pack-import test.

- [ ] **Step 5: Type-check and confirm no regressions**

Run: `cd shirita-ui && npx vue-tsc -b --noEmit && npx vitest run`
Expected: no type errors; full frontend test suite passes.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/views/BookView.vue shirita-ui/src/views/BookView.test.ts
git commit -m "$(cat <<'EOF'
feat(ui): add Import button to the Pack section

Restructure the Pack toolbar to always render its buttons in the canonical
order (rename/import/export/duplicate/delete), matching the Template header
and the Definitions editor. The Import button reuses the shared importInput,
so it works with no pack selected (import creates one). Selection-dependent
buttons are disabled when no pack is selected.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Web binary release CI workflow

**Files:**
- Create: `.github/workflows/web.yml`

**Interfaces:**
- Consumes: the `embed-ui` cargo feature (`shirita-web/Cargo.toml`), `shirita-ui` npm build producing `shirita-ui/dist`.
- Produces: per-platform artifacts `shirita-web-ubuntu-latest` / `-macos-latest` / `-windows-latest`, each containing the `shirita-web` binary.

- [ ] **Step 1: Create the workflow file**

Create `.github/workflows/web.yml`:

```yaml
name: web-build

on:
  workflow_dispatch: {}
  push:
    tags:
      - "v*"

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: ubuntu-latest
            target: x86_64-unknown-linux-musl
            bin: target/x86_64-unknown-linux-musl/release/shirita-web
          - platform: macos-latest
            target: aarch64-apple-darwin
            bin: target/aarch64-apple-darwin/release/shirita-web
          - platform: windows-latest
            target: x86_64-pc-windows-msvc
            bin: target/x86_64-pc-windows-msvc/release/shirita-web.exe
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4

      - name: Install musl tools (Linux static build)
        if: matrix.platform == 'ubuntu-latest'
        run: |
          sudo apt-get update
          sudo apt-get install -y musl-tools

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - uses: Swatinem/rust-cache@v2

      - uses: actions/setup-node@v4
        with:
          node-version: 20

      - name: Install frontend deps
        run: npm ci
        working-directory: shirita-ui

      - name: Build frontend (embedded into the binary)
        run: npm run build
        working-directory: shirita-ui

      - name: Build web binary (embed-ui, static on Linux)
        run: cargo build --release -p shirita-web --features embed-ui --target ${{ matrix.target }}

      - name: Upload binary
        uses: actions/upload-artifact@v4
        with:
          name: shirita-web-${{ matrix.platform }}
          path: ${{ matrix.bin }}
          if-no-files-found: error
```

- [ ] **Step 2: Validate the workflow is well-formed YAML**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/web.yml')); print('ok')"`
Expected: `ok` (no YAML parse error).

- [ ] **Step 3: Reproduce the Linux musl build locally**

This proves the exact command line the workflow runs produces a static, glibc-free binary. (The user's environment is Debian-based; `apt` and `rustup` are available.)

Run:
```bash
rustup target add x86_64-unknown-linux-musl
sudo apt-get install -y musl-tools
cd shirita-ui && npm ci && npm run build && cd ..
cargo build --release -p shirita-web --features embed-ui --target x86_64-unknown-linux-musl
```
Expected: the build completes and `target/x86_64-unknown-linux-musl/release/shirita-web` exists.

- [ ] **Step 4: Confirm the binary is statically linked (no glibc)**

Run: `ldd target/x86_64-unknown-linux-musl/release/shirita-web`
Expected: `not a dynamic executable` (or `statically linked`) — proves it carries no glibc dependency and runs on any Linux.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/web.yml
git commit -m "$(cat <<'EOF'
ci: build standalone shirita-web binary on v* tags (Linux musl/macOS/Windows)

New web-build workflow builds the embed-ui binary per platform and uploads it
as an artifact. The Linux target is x86_64-unknown-linux-musl (fully static,
no glibc dependency) so it runs on older hosts; deps are musl-safe (rustls-tls,
bundled SQLite via musl-tools). Swatinem/rust-cache speeds repeat builds.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: Final verification (manual, on GitHub)**

After pushing, trigger the workflow via the Actions tab → **web-build** → **Run workflow** (`workflow_dispatch`). Confirm all three matrix jobs succeed and each uploads a `shirita-web-<platform>` artifact. (This is the only way to verify the macOS/Windows targets and the rust-cache action; it cannot be run locally.)

---

## Self-Review notes

- **Spec coverage:** §2 (Pack Import button: order rename→import→export→duplicate→delete, always-rendered toolbar, `data-test="pack-import"`, `common.import`, reuse `importInput`, no script change) → Task 1. §3 (web.yml: name/triggers/matrix, musl Linux + `musl-tools`, `targets:` toolchain, `Swatinem/rust-cache@v2`, npm build, `cargo build --features embed-ui --target`, per-OS artifact, `if-no-files-found`) → Task 2. §4 testing (frontend button test; YAML validity; local musl build + `ldd`; manual `workflow_dispatch`) → Task 1 Steps 1-5, Task 2 Steps 2-6.
- **Placeholder scan:** none — every code/YAML block is complete and copy-pasteable.
- **Consistency:** the Pack toolbar order and styling match the spec verbatim and the Template header (`BookView.vue:920-968`); the matrix `target`/`bin` values match the spec table exactly; `if-no-files-found: error` is consistent with the exact per-OS `bin` path.
- **Note vs. spec:** the spec §3 first listed `if-no-files-found: ignore` (two candidate paths), then the table settled on one exact `bin` path per OS — the plan uses `error`, correct for a single exact path. Intentional.

## Out of scope

Push/PR `cargo test` gate; ST JS/XML compat (#4); archiving/checksums/GitHub Release attachment; changes to `docker.yml` / `desktop.yml`.
