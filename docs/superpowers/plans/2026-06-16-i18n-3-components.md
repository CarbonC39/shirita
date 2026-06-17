# i18n Plan 3 — String Extraction (Components) + Finalize

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract every remaining hard-coded English UI string out of the shared components into the four i18n catalogs, then run the final full-green gate and finish the i18n branch.

**Architecture:** Components grouped by area into a handful of tasks, each following the same repeatable extraction procedure as Plan 2 (restated below so this plan is self-contained). The parity test + `vue-tsc` remain the safety net. This is the last i18n plan — its terminal task finishes the development branch.

**Tech Stack:** Vue 3 `<script setup>`, vue-i18n@^10, Vitest, vue-tsc.

**Spec:** `docs/superpowers/specs/2026-06-16-i18n-zh-ja-design.md` (§5, §6, §8, §10).

**Prerequisite:** Plans 1 & 2 complete (infra + all six views extracted).

---

## Namespace Map

| Component(s) | Namespace |
|--------------|-----------|
| `AppShell.vue` | `shell` (nav seeded in Plan 1 — extract any remaining chrome) |
| `Composer.vue` | `composer` |
| `MessageItem.vue`, `MessageList.vue`, `ChatCard.vue` | `chat` (reuse Plan 2's namespace) / `common` |
| `DefinitionEditor.vue` | `definition` |
| `PromptTree.vue`, `NodeRow.vue`, `NodePicker.vue`, `TriggerEditor.vue` | `prompt` |
| `VariablesEditor.vue`, `VariablesPanel.vue` | `variables` |
| `RegexRuleEditor.vue` | `settings` |
| `AvatarPicker.vue`, `AssetPicker.vue`, `FullscreenEditor.vue` | `common` |
| `SegmentedControl.vue`, `SliderControl.vue`, `ToggleSwitch.vue` | none — labels are passed in as props; verify no literal strings, skip if clean |

---

## The Repeatable Extraction Procedure (same as Plan 2 §"The Repeatable Extraction Procedure")

For every component task below:

**A. Find the strings.** Read the `.vue` file. List visible English: template text nodes, `placeholder`/`title`/`aria-label` attributes, and script strings that get rendered (`error.value = '…'`, ternary labels). Grep starter (from repo root):
```bash
grep -nE '(placeholder|title|aria-label)="[A-Za-z]' shirita-ui/src/components/<Comp>.vue
grep -nE '>[A-Za-z][^<{]*<' shirita-ui/src/components/<Comp>.vue
```
**Do NOT extract:** user content (definition/variable/template names, message text, node content); system identifiers (`def_type` ids, `owner_kind`, `role`, provider keys); `data-test` values; CSS; code placeholders.

**B. Add keys to `en.ts`** under the component's namespace; camelCase keys; plural form for "number + countable noun" (en `'… {count} x | … {count} xs'`, zh/ja single form).

**C. Translate** the same key paths into `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts` (Traditional uses TW/HK vocabulary, not char-mapped Simplified).

**D. Rewrite** the component: template text → `{{ $t('ns.key') }}`; attribute → `:placeholder="$t('ns.key')"`; script → `const { t } = useI18n()` (add `import { useI18n } from 'vue-i18n'`) then `t('ns.key', { count })`.

**E. Verify** (run every task):
```bash
npm --prefix shirita-ui exec vitest run src/locales/parity.test.ts
npm --prefix shirita-ui exec vue-tsc --noEmit
npm --prefix shirita-ui exec vitest run
```

**F. Commit** `feat(ui): i18n <area> strings` + standard trailer.

> Existing component tests assert English literals; under the default jsdom locale (`en`) those strings still render in English, so the tests keep passing. If a test asserts a string you moved to a *different* English wording, update the test to the new literal in the same commit — but prefer keeping the exact English text so tests don't churn.

---

### Task 1: AppShell → `shell`

**Files:** `shirita-ui/src/components/AppShell.vue`, `shirita-ui/src/locales/{en,zh-Hans,zh-Hant,ja}.ts`

- [ ] **Step 1:** Procedure A on `AppShell.vue` (nav labels Chats/New/Book/Settings already in `shell` from Plan 1; extract any tooltips, aria-labels, or other chrome not yet covered). Reuse existing `shell.*` keys; add new ones only for new strings.
- [ ] **Step 2:** Procedure B — extend `shell` in `en.ts`.
- [ ] **Step 3:** Procedure C — translate into the three locales.
- [ ] **Step 4:** Procedure D — rewrite `AppShell.vue` (its nav text → `$t('shell.chats')` etc.).
- [ ] **Step 5:** Procedure E — three commands green.
- [ ] **Step 6:** Commit:
```bash
git add shirita-ui/src/components/AppShell.vue shirita-ui/src/locales/
git commit -m "feat(ui): i18n AppShell strings

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Composer → `composer`

**Files:** `shirita-ui/src/components/Composer.vue`, catalogs.

- [ ] **Step 1:** Procedure A on `Composer.vue` (message `placeholder`, Send button/title, attach controls, any "press Enter to send" hints). Message draft text is user content — exclude.
- [ ] **Step 2–6:** Procedure B→F under namespace `composer`. Commit message: `feat(ui): i18n Composer strings`.

---

### Task 3: Chat list & messages → `chat` / `common`

**Files:** `shirita-ui/src/components/MessageItem.vue`, `MessageList.vue`, `ChatCard.vue`, catalogs.

- [ ] **Step 1:** Procedure A on all three. Likely strings: message action tooltips (Edit/Delete/Regenerate/Copy/Branch), `ChatCard` relative-time labels and menu items, empty/placeholder text. Message *content*, character *names*, and timestamps' underlying values are user data — exclude; only the surrounding labels are extracted.
- [ ] **Step 2:** Procedure B — reuse `common.*` for Edit/Delete/Copy/Duplicate where they already exist; add `chat.*` for chat-specific labels. Any "N messages"/"N tokens" → plural keys (reuse `common.tokensEstimate`).
- [ ] **Step 3:** Procedure C — translate.
- [ ] **Step 4:** Procedure D — rewrite all three components. Watch `MessageItem.test.ts` / existing `ChatCard` tests: keep the English literal identical so assertions hold.
- [ ] **Step 5:** Procedure E — three commands green; pay attention to the full `vitest run` since these components are well-covered.
- [ ] **Step 6:** Commit:
```bash
git add shirita-ui/src/components/MessageItem.vue shirita-ui/src/components/MessageList.vue shirita-ui/src/components/ChatCard.vue shirita-ui/src/locales/
git commit -m "feat(ui): i18n chat list + message components strings

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: DefinitionEditor → `definition`

**Files:** `shirita-ui/src/components/DefinitionEditor.vue`, catalogs.

- [ ] **Step 1:** Procedure A on `DefinitionEditor.vue`. Extract field labels (Name/Content/etc.), section headers, the `wrap_in_tag` toggle label, buttons. **Do NOT** extract the type chips `Character`/`World`/`Prompt` if `DefinitionEditor.test.ts` asserts them as derived from a system `def_type` id — verify whether they are display labels (extractable) or identifiers (not). If they are display labels mapped from ids, extract them under `definition.type*` keys and keep the English values identical so the test's `['Character','World','Prompt']` assertion still passes under the default `en` locale.
- [ ] **Step 2–6:** Procedure B→F under `definition`. Run `DefinitionEditor.test.ts` specifically in Step 5: `npm --prefix shirita-ui exec vitest run src/components/DefinitionEditor.test.ts`. Commit: `feat(ui): i18n DefinitionEditor strings`.

---

### Task 5: Prompt tree → `prompt`

**Files:** `shirita-ui/src/components/PromptTree.vue`, `NodeRow.vue`, `NodePicker.vue`, `TriggerEditor.vue`, catalogs.

- [ ] **Step 1:** Procedure A on all four. Extract: node-type picker labels, add/insert/remove controls, trigger-editor field labels and condition chrome, empty states. Node *names*/*content* and trigger *values* are user data — exclude.
- [ ] **Step 2–6:** Procedure B→F under `prompt` (reuse keys Plan 2 added for `NewChatPromptView` where wording matches). Commit:
```bash
git add shirita-ui/src/components/PromptTree.vue shirita-ui/src/components/NodeRow.vue shirita-ui/src/components/NodePicker.vue shirita-ui/src/components/TriggerEditor.vue shirita-ui/src/locales/
git commit -m "feat(ui): i18n prompt-tree components strings

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 6: Variables → `variables`

**Files:** `shirita-ui/src/components/VariablesEditor.vue`, `VariablesPanel.vue`, catalogs.

- [ ] **Step 1:** Procedure A on both. Extract: add-variable button, column headers (Name/Value/Scope), empty state, any help text. Variable *names*/*values* are user data — exclude.
- [ ] **Step 2–6:** Procedure B→F under `variables`. Commit:
```bash
git add shirita-ui/src/components/VariablesEditor.vue shirita-ui/src/components/VariablesPanel.vue shirita-ui/src/locales/
git commit -m "feat(ui): i18n variables components strings

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 7: RegexRuleEditor → `settings`

**Files:** `shirita-ui/src/components/RegexRuleEditor.vue`, catalogs.

- [ ] **Step 1:** Procedure A on `RegexRuleEditor.vue`. Extract: Pattern/Replacement labels, scope toggles (AI output / User input / Display only), enabled toggle label, delete control. Rule *name*/*pattern*/*replacement* are user data — exclude.
- [ ] **Step 2–6:** Procedure B→F under `settings` (e.g. `settings.regex*`). Commit: `feat(ui): i18n RegexRuleEditor strings`.

---

### Task 8: Pickers & fullscreen → `common`

**Files:** `shirita-ui/src/components/AvatarPicker.vue`, `AssetPicker.vue`, `FullscreenEditor.vue`, catalogs.

- [ ] **Step 1:** Procedure A on all three. Extract: upload/choose/remove labels, empty states, FullscreenEditor's close/done control + title. File names are user data — exclude.
- [ ] **Step 2–6:** Procedure B→F under `common` (these are generic). Commit:
```bash
git add shirita-ui/src/components/AvatarPicker.vue shirita-ui/src/components/AssetPicker.vue shirita-ui/src/components/FullscreenEditor.vue shirita-ui/src/locales/
git commit -m "feat(ui): i18n picker + fullscreen components strings

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 9: Pure controls — verify clean

**Files:** `shirita-ui/src/components/SegmentedControl.vue`, `SliderControl.vue`, `ToggleSwitch.vue` (inspect only).

- [ ] **Step 1:** Read each. Confirm all user-visible text arrives via props (e.g. `SliderControl`'s `label`, `SegmentedControl`'s `options[].label`) and there are no hard-coded English literals.
```bash
grep -nE '>[A-Za-z][^<{]*<|(placeholder|title|aria-label)="[A-Za-z]' shirita-ui/src/components/SegmentedControl.vue shirita-ui/src/components/SliderControl.vue shirita-ui/src/components/ToggleSwitch.vue
```
- [ ] **Step 2:** If clean (expected), no change — note it and move on. If any literal exists, apply procedure B→F to extract it under the appropriate namespace and commit.

---

### Task 10: Final full-green gate + leftover sweep

**Files:** none (verification only)

- [ ] **Step 1:** Leftover-English sweep across all components (spec §10 heuristic):
```bash
grep -rnE '>[A-Z][a-z]+[^<{]*<' shirita-ui/src/components/ | grep -v '\$t('
```
Review each hit. Extract any genuine UI prose missed (re-run the relevant task's procedure + commit). Brand names, code, single-letter/icon content, and `data-test` are expected noise.
- [ ] **Step 2:** Type-check — `npm --prefix shirita-ui exec vue-tsc --noEmit` → exit 0.
- [ ] **Step 3:** Full tests — `npm --prefix shirita-ui exec vitest run` → all green (parity guards all four locales; resolve/switch/SettingsView i18n tests pass).
- [ ] **Step 4:** Build — `npm --prefix shirita-ui run build` → succeeds (`vue-tsc -b` + `vite build`).
- [ ] **Step 5:** Manual smoke (recommended): `npm --prefix shirita-ui run dev`, switch through all four languages in Settings, click through Home / New chat / Chat / Book / Settings and confirm no stray English remains in chrome and no `{{ }}` / missing-key artifacts appear. User content stays in its original language (expected).

---

### Task 11: Finish the i18n branch

- [ ] **Step 1:** Announce: "I'm using the finishing-a-development-branch skill to complete this work." Then **REQUIRED SUB-SKILL:** use superpowers:finishing-a-development-branch.
- [ ] **Step 2:** Follow that skill: verify tests pass (Task 10 already did), detect environment, determine base branch (`main`), present the options menu, execute the user's choice.

This completes sub-project B (i18n). Next sub-projects per the spec lineage: C (avatars/names), then A (deploy + full CI).
