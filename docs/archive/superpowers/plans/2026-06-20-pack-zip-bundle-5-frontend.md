# Pack Zip Bundle — Plan 5: Frontend export/import wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface pack zip export/import in the Book UI. Add a **Download** button to the pack toolbar that exports the selected pack — saving it as `<name>.zip` or `<name>.json` per the server's `Content-Disposition` (the extension is content-dependent, decided by Plan 3). Widen the existing import file picker to accept `.zip`, so a `shirita.pack` bundle imports through the same `/api/import` flow (Plan 4) the user already uses for cards/worlds/templates.

**Architecture:** The frontend already has `downloadExport(path, filename)` (fixed filename) and `importFile(file, onConflict)` (multipart POST to `/api/import`). Packs differ only in that the download filename is **chosen by the server** (`.zip` vs `.json`), so a small `downloadPackExport(id, name)` reads `Content-Disposition` to name the saved file (falling back to `<name>.zip`). Import needs **zero** new client code — `importFile` already posts any file; only the `<input accept>` widens. Wiring lives in `BookView.vue`, with one new i18n key (`book.exportPackTitle`) added across all four locales to keep the parity test green.

**Tech Stack:** Vue 3 `<script setup>`, Pinia, vue-i18n v10 (parity test enforces identical key sets; **never put literal `{`/`}` in a value**), Vitest + jsdom, `fetch` + Blob download.

## Global Constraints

- Pack download filename comes from the response `Content-Disposition` header (server already sets `.zip`/`.json`); fall back to `<name>.zip` only if the header is absent.
- Import is unchanged server-side; the picker simply also accepts `.zip` / `application/zip`. The existing `on_conflict` selector flows through untouched.
- New i18n key in **all four** locales (en, zh-Hans, zh-Hant, ja); the book-section `importTitle` updated to mention `.zip` in all four. No literal braces in any value.
- UI stays single-column, inline (no new tabs/views) — the Download button sits in the existing pack toolbar beside rename/duplicate/delete.
- Comments/commits in English. Tests: `npm run test` (Vitest) — specifically `client.test.ts` (download naming) + `parity.test.ts` (locale parity).

---

## File Structure

- `shirita-ui/src/api/client.ts` — add `downloadPackExport`.
- `shirita-ui/src/api/client.test.ts` — two unit tests for the filename behavior.
- `shirita-ui/src/views/BookView.vue` — import + handler + Download button + widened `accept`.
- `shirita-ui/src/views/BookView.test.ts` — add `downloadPackExport` to the client mock.
- `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts` — `book.exportPackTitle` + `book.importTitle` update.

---

### Task 1: `downloadPackExport` client helper

**Files:**
- Modify: `shirita-ui/src/api/client.ts`
- Test: `shirita-ui/src/api/client.test.ts`

**Interfaces:**
- Produces: `downloadPackExport(id: string, name: string): Promise<void>` — fetches `/api/packs/{id}/export` with auth and triggers a blob download named from `Content-Disposition` (fallback `<name>.zip`).

- [ ] **Step 1: Write the failing unit tests**

In `shirita-ui/src/api/client.test.ts`, add `downloadPackExport` to the existing `from './client'` import, then append:

```ts
describe('downloadPackExport', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  // Capture the anchor the helper creates; keep the real DOM otherwise.
  function captureAnchor() {
    const a = document.createElement('a')
    vi.spyOn(document, 'createElement').mockReturnValue(a)
    vi.spyOn(a, 'click').mockImplementation(() => {})
    vi.stubGlobal('URL', { createObjectURL: () => 'blob:x', revokeObjectURL: () => {} })
    return a
  }

  it('names the download from Content-Disposition', async () => {
    const a = captureAnchor()
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      headers: { get: (k: string) => (k.toLowerCase() === 'content-disposition' ? 'attachment; filename="Alice.zip"' : null) },
      blob: () => Promise.resolve(new Blob([new Uint8Array([0x50, 0x4b])])),
    }))
    await downloadPackExport('p1', 'Alice')
    expect(a.download).toBe('Alice.zip')
  })

  it('falls back to <name>.zip when there is no Content-Disposition', async () => {
    const a = captureAnchor()
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      headers: { get: () => null },
      blob: () => Promise.resolve(new Blob([])),
    }))
    await downloadPackExport('p9', 'Bob')
    expect(a.download).toBe('Bob.zip')
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm run test -- src/api/client.test.ts 2>&1 | tail -20`
Expected: FAIL — `downloadPackExport` is not exported yet (import resolves to `undefined`, call throws).

- [ ] **Step 3: Implement `downloadPackExport`**

In `shirita-ui/src/api/client.ts`, after `downloadExport` (next to `exportDefinitionPath`/`exportTemplatePath`), add:

```ts
// Pack export's filename (.zip vs .json) is server-decided, so read it from
// Content-Disposition rather than passing a fixed name; fall back to <name>.zip.
export async function downloadPackExport(id: string, name: string): Promise<void> {
  const res = await fetch(`${BASE}/api/packs/${id}/export`, { headers: authHeaders() })
  if (!res.ok) throw new Error(`export failed: ${res.status}`)
  const cd = res.headers.get('content-disposition') ?? ''
  const m = cd.match(/filename="?([^"]+)"?/)
  const filename = m?.[1] ?? `${name || 'pack'}.zip`
  const blob = await res.blob()
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  URL.revokeObjectURL(url)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm run test -- src/api/client.test.ts 2>&1 | tail -20`
Expected: PASS — both naming cases.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/api/client.ts shirita-ui/src/api/client.test.ts
git commit -m "feat(web): downloadPackExport — name pack download from Content-Disposition"
```

---

### Task 2: Book UI — pack Export button + `.zip` import

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue`
- Modify: `shirita-ui/src/views/BookView.test.ts`
- Modify: `shirita-ui/src/locales/en.ts`, `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts`

**Interfaces:**
- Consumes: `downloadPackExport` (Task 1), existing `selectedPack`, `importFile` flow.
- Produces: a `data-test="pack-export"` Download button; the import `<input accept>` includes `.zip`; `book.exportPackTitle` in all locales.

- [ ] **Step 1: Add the i18n key + update importTitle (all four locales)**

The parity test fails if any locale lacks a key, so edit all four. In each `book:` block, add `exportPackTitle` right after `exportTemplateTitle`, and append `.zip` to the book-section `importTitle` (the long "card / world / template" one — **not** the short conversation-import title).

`shirita-ui/src/locales/en.ts`:
```ts
    importTitle: 'Import card / world / template / pack (.png, .json, .zip)',
    exportTemplateTitle: 'Export template (enabled part)',
    exportPackTitle: 'Export pack (.zip / .json)',
```

`shirita-ui/src/locales/zh-Hans.ts`:
```ts
    importTitle: '导入角色卡 / 世界书 / 模板 / Pack（.png, .json, .zip）',
    exportTemplateTitle: '导出模板（已启用部分）',
    exportPackTitle: '导出 Pack（.zip / .json）',
```

`shirita-ui/src/locales/zh-Hant.ts`:
```ts
    importTitle: '匯入角色卡 / 世界書 / 範本 / Pack（.png, .json, .zip）',
    exportTemplateTitle: '匯出範本（已啟用部分）',
    exportPackTitle: '匯出 Pack（.zip / .json）',
```

`shirita-ui/src/locales/ja.ts`:
```ts
    importTitle: 'カード / ワールド / テンプレート / パックをインポート（.png, .json, .zip）',
    exportTemplateTitle: 'テンプレートをエクスポート（有効な部分）',
    exportPackTitle: 'パックをエクスポート（.zip / .json）',
```

- [ ] **Step 2: Wire the handler + button + widened accept in BookView.vue**

In the client import block (around line 28-36), add `downloadPackExport`:

```ts
    downloadPackExport,
```

In the packs script section (after `delPack`, around line 744), add:

```ts
async function exportSelectedPack() {
    if (!selectedPack.value) return;
    try {
        await downloadPackExport(selectedPack.value.id, selectedPack.value.name || "pack");
    } catch (e) { error.value = (e as Error).message; }
}
```

In the pack toolbar (the `<div v-if="selectedPack" class="flex items-center">` at ~line 1056), add a Download button between duplicate and delete:

```html
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" :title="$t('book.exportPackTitle')" data-test="pack-export" @click="exportSelectedPack"><Download :size="16" /></button>
```

Widen the import `<input>` accept (line ~933) from `.png,.json,application/json,image/png` to:

```html
                        accept=".png,.json,.zip,application/json,image/png,application/zip"
```

(`Download` is already imported from `lucide-vue-next`; no import change.)

- [ ] **Step 3: Add the client mock entry in BookView.test.ts**

In `shirita-ui/src/views/BookView.test.ts`, inside the `vi.mock('../api/client', () => ({ … }))` object (with the other pack fns ~line 25-29), add:

```ts
  downloadPackExport: vi.fn().mockResolvedValue(undefined),
```

- [ ] **Step 4: Run the suites to verify green**

Run: `npm run test -- src/locales/parity.test.ts src/views/BookView.test.ts 2>&1 | tail -20`
Expected: PASS — locale parity holds with the new key; BookView still mounts/renders (the new mock resolves the import).

- [ ] **Step 5: Full frontend sweep + commit**

```bash
npm run test 2>&1 | tail -12
npm run build 2>&1 | tail -8
git add shirita-ui/src/views/BookView.vue shirita-ui/src/views/BookView.test.ts shirita-ui/src/locales/en.ts shirita-ui/src/locales/zh-Hans.ts shirita-ui/src/locales/zh-Hant.ts shirita-ui/src/locales/ja.ts
git commit -m "feat(web): pack Export button + .zip import in Book UI"
```

---

## Final Verification

- [ ] **Frontend test + typecheck/build sweep**

Run: `cd shirita-ui && npm run test 2>&1 | tail -12 && npm run build 2>&1 | tail -8`
Expected: all Vitest suites pass (incl. the two new `downloadPackExport` cases + parity); `vue-tsc`/build clean (no unused-import or type errors). Manual smoke (optional): in Book, select a pack with an avatar → Download saves `<name>.zip`; re-import that `.zip` → the pack reappears with its avatar.

---

## Self-Review

**Spec coverage (spec §6, §8 — UX):**
- Pack export trigger in Book — Task 2 Download button (`data-test="pack-export"`) → `exportSelectedPack` → `downloadPackExport`.
- Server-decided `.zip`/`.json` filename honored — Task 1 reads `Content-Disposition`; asserted by both client tests.
- `.zip` import plug-and-play through existing `/api/import` — Task 2 widened `accept`; `importFile` unchanged (Plan 4 handles the bytes).
- i18n parity preserved — `book.exportPackTitle` added to all four locales + braceless values; guarded by `parity.test.ts`.

**Placeholder scan:** none — full function body, complete tests, exact locale strings, exact button markup, exact accept string, exact commands.

**Type consistency:** `downloadPackExport(id: string, name: string): Promise<void>` matches the BookView call (`selectedPack.value.id`, `selectedPack.value.name`) and both test calls. Reuses the established `BASE`/`authHeaders()`/blob-download idiom from `downloadExport` (kept as a separate function rather than refactoring the shared blob-download mechanics, to isolate the change from the working definition/template export). The import-`accept` change is markup-only; no new import client code. `Download` icon already in scope.

**Risk notes:**
- `Content-Disposition` parse is a simple `filename="?([^"]+)"?` match — fine for the server's `attachment; filename="<safe>.zip"` (safe names are `[A-Za-z0-9_-]`, no quotes/semicolons). Fallback `<name>.zip` covers a missing header.
- The two client tests stub `URL.createObjectURL`/`revokeObjectURL` and the anchor's `click` (jsdom has no real download/navigation); they capture the created anchor via a `document.createElement` spy and assert its `.download`. `vi.restoreAllMocks()` + `vi.unstubAllGlobals()` in `afterEach` prevents leakage into other suites.
- This is the last of the five pack-zip-bundle plans; after it merges, a pack with bound binary round-trips: export (Plan 3) → safe, deduped, atomic import (Plan 4) → re-rendered in the Book/Chat UI (this plan).
