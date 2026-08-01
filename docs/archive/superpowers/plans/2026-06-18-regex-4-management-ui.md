# Regex Phase 4 — Management UI + scope three-way (Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Settings · Regex a compact master-detail list — global rules pinned on top (distinct background), loreset rules below with their source-card label, accordion edit (one open), scope/target badges, hide-disabled + search, invalid patterns flagged — and expose the prompt/both/display three-way in the editor.

**Architecture:** A read-only `GET /api/regex-rules/scopes` endpoint returns each rule's scope (global vs template), source template names, and any pattern compile error (via a new core `regex_error`). The frontend merges that with the rule list, sorts global-first, renders compact rows, and edits the selected rule inline. The editor's scope view model replaces the `display_only` boolean with a `phase: 'display'|'both'|'prompt'`.

**Tech Stack:** Rust (`shirita-web`, `shirita-core`), Vue 3 + TS, vue-i18n.

## Global Constraints

- Code comments and git commit messages in English; UI copy added to all four locales (`en`, `zh-Hans`, `zh-Hant`, `ja`) to pass `src/locales/parity.test.ts`.
- Build UI in English; avoid fixed-width label layouts (i18n memory).
- Depends on Phase 1 (fancy-regex `regex_error`) and Phase 3 (scope/targets meta already written by the editor via `scopeFlagsToMeta`).
- After each task: relevant `cargo test` / `vue-tsc --noEmit` / `vitest run` green, then commit. Do not push.

---

### Task 1: `regex_error` core helper + `GET /api/regex-rules/scopes`

**Files:**
- Modify: `shirita-core/src/assembly.rs` (add `regex_error`); `shirita-core/src/lib.rs` (re-export)
- Create: `shirita-web/src/routes/regex_rules.rs`
- Modify: `shirita-web/src/routes/mod.rs` (declare module); `shirita-web/src/lib.rs` (register route)
- Test: `shirita-core/src/assembly.rs` (regex_error test); `shirita-web/tests/regex_scopes_test.rs`

**Interfaces:**
- Produces: `shirita_core::regex_error(pattern: &str) -> Option<String>`; `GET /api/regex-rules/scopes -> Vec<RegexScope { id, scope, template_names, pattern_error }>`.

- [ ] **Step 1: Write the failing core test**

In `assembly.rs` tests:

```rust
#[test]
fn regex_error_reports_only_invalid() {
    assert!(regex_error(r"\d+").is_none());
    assert!(regex_error(r"foo(?=bar)").is_none()); // valid under fancy-regex
    assert!(regex_error(r"foo(").is_some());       // unbalanced paren -> error string
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p shirita-core regex_error_reports_only_invalid`
Expected: FAIL — `regex_error` undefined.

- [ ] **Step 3: Implement core helper**

In `assembly.rs`, next to `is_valid_regex`:

```rust
/// 编译错误信息（合法或空 pattern 返回 None），供 UI 标记失效规则。
pub fn regex_error(pattern: &str) -> Option<String> {
    if pattern.is_empty() {
        return None;
    }
    fancy_regex::Regex::new(pattern).err().map(|e| e.to_string())
}
```

Re-export in `lib.rs` (`pub use assembly::{ ..., regex_error };`).

- [ ] **Step 4: Run core test**

Run: `cargo test -p shirita-core regex_error_reports_only_invalid`
Expected: PASS.

- [ ] **Step 5: Create the web handler**

Create `shirita-web/src/routes/regex_rules.rs`:

```rust
use std::collections::HashMap;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use shirita_core::OwnerKind;

use crate::AppState;

#[derive(Serialize)]
pub struct RegexScope {
    pub id: String,
    /// "global" (orphan, applies everywhere) or "template" (loreset-scoped).
    pub scope: String,
    /// Names of templates whose tree references this rule (deduped).
    pub template_names: Vec<String>,
    /// fancy-regex compile error, if the pattern is invalid.
    pub pattern_error: Option<String>,
}

/// Per-`regex_rule` scope + source templates + validity, for the Settings UI.
pub async fn list_regex_scopes(
    State(state): State<AppState>,
) -> Result<Json<Vec<RegexScope>>, StatusCode> {
    let err = |_| StatusCode::INTERNAL_SERVER_ERROR;
    let defs = state.storage.list_definitions().await.map_err(err)?;
    let templates = state.storage.list_templates().await.map_err(err)?;

    // def_id -> ordered unique template names referencing it
    let mut refs: HashMap<String, Vec<String>> = HashMap::new();
    for t in &templates {
        let nodes = state.storage.list_nodes(&OwnerKind::Template, &t.id).await.map_err(err)?;
        for n in nodes {
            if let Some(did) = n.definition_id {
                let names = refs.entry(did).or_default();
                if !names.contains(&t.name) {
                    names.push(t.name.clone());
                }
            }
        }
    }

    let out = defs
        .iter()
        .filter(|d| d.def_type == "regex_rule")
        .map(|d| {
            let names = refs.get(&d.id).cloned().unwrap_or_default();
            let scope = if names.is_empty() { "global" } else { "template" };
            let pattern = d.meta.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            RegexScope {
                id: d.id.clone(),
                scope: scope.to_string(),
                template_names: names,
                pattern_error: shirita_core::regex_error(pattern),
            }
        })
        .collect();
    Ok(Json(out))
}
```

- [ ] **Step 6: Register module + route**

In `shirita-web/src/routes/mod.rs`, add `pub mod regex_rules;`.
In `shirita-web/src/lib.rs`, add to the router (near the other `/...` routes):

```rust
        .route("/regex-rules/scopes", get(routes::regex_rules::list_regex_scopes))
```

(ensure `get` is imported — it already is for existing GET routes.)

- [ ] **Step 7: Write + run the web test**

Create `shirita-web/tests/regex_scopes_test.rs` mirroring the existing web test harness: seed one orphan regex rule and one referenced by a template, then GET `/api/regex-rules/scopes`, assert the orphan → `scope=="global"`/empty names, the referenced → `scope=="template"`/`template_names==["<name>"]`, and an invalid-pattern rule → `pattern_error` set.

Run: `cargo test -p shirita-web regex_scopes`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add shirita-core/src/assembly.rs shirita-core/src/lib.rs shirita-web/src/routes/regex_rules.rs shirita-web/src/routes/mod.rs shirita-web/src/lib.rs shirita-web/tests/regex_scopes_test.rs
git commit -m "feat(core,web): regex_error helper + GET /regex-rules/scopes"
```

---

### Task 2: Scope three-way in the editor view model

**Files:**
- Modify: `shirita-ui/src/api/types.ts` (`RegexRule.scope`)
- Modify: `shirita-ui/src/utils/regexRule.ts` (`metaToRule`, `scopeFlagsToMeta`)
- Modify: `shirita-ui/src/components/RegexRuleEditor.vue` (apply-to section)
- Test: `shirita-ui/src/utils/regexRule.test.ts`

**Interfaces:**
- Produces: `RegexRule['scope'] = { ai_output: boolean; user_input: boolean; phase: 'display' | 'both' | 'prompt' }`.
- `scopeFlagsToMeta(scope)` → `{ scope: 'display'|'both'|'prompt'; targets: string[] }`.

- [ ] **Step 1: Update the failing test**

In `regexRule.test.ts`, replace the `display_only` expectations with `phase`, and add a prompt case:

```ts
it('maps prompt scope round-trip', () => {
  const meta = scopeFlagsToMeta({ ai_output: false, user_input: true, phase: 'prompt' })
  expect(meta).toEqual({ scope: 'prompt', targets: ['user_input'] })
})
it('reads phase back from meta', () => {
  const rule = metaToRule({ id: 'r', type: 'regex_rule', name: 'R', content: '',
    meta: { pattern: 'x', replacement: '', scope: 'both', targets: ['ai_output'] } } as never)
  expect(rule.scope).toEqual({ ai_output: true, user_input: false, phase: 'both' })
})
```

(Adjust any existing assertion that referenced `display_only`.)

- [ ] **Step 2: Run to verify failure**

Run (from `shirita-ui`): `npx vitest run regexRule`
Expected: FAIL — `phase` not produced.

- [ ] **Step 3: Update the type**

In `types.ts`, change `RegexRule.scope`:

```ts
  scope: { ai_output: boolean; user_input: boolean; phase: 'display' | 'both' | 'prompt' }
```

- [ ] **Step 4: Update the mappers**

In `regexRule.ts`:

```ts
export function metaToRule(def: Definition): RegexRule {
  const meta = def.meta as Record<string, unknown>
  const scopeStr = typeof meta.scope === 'string' ? meta.scope : 'display'
  const phase: 'display' | 'both' | 'prompt' =
    scopeStr === 'prompt' ? 'prompt' : scopeStr === 'both' ? 'both' : 'display'
  const targets = Array.isArray(meta.targets) ? (meta.targets as unknown[]) : []
  const hasTargets = targets.length > 0
  return {
    id: def.id,
    name: def.name,
    pattern: typeof meta.pattern === 'string' ? meta.pattern : '',
    replacement: typeof meta.replacement === 'string' ? meta.replacement : '',
    enabled: meta.disabled !== true,
    scope: {
      ai_output: !hasTargets || targets.includes('ai_output'),
      user_input: targets.includes('user_input'),
      phase,
    },
  }
}

export function scopeFlagsToMeta(scope: RegexRule['scope']): { scope: string; targets: string[] } {
  const targets: string[] = []
  if (scope.ai_output) targets.push('ai_output')
  if (scope.user_input) targets.push('user_input')
  return { scope: scope.phase, targets }
}
```

- [ ] **Step 5: Update the editor apply-to section**

In `RegexRuleEditor.vue`, replace the third checkbox (`display_only`) with a phase select. Replace the `regexDisplayOnly` label `<label>...</label>` with:

```html
          <select
            :value="rule.scope.phase"
            class="text-[13px] border border-line rounded-md px-1.5 py-1 outline-none focus:border-primary/50"
            @change="emit('update:scope', { ...rule.scope, phase: ($event.target as HTMLSelectElement).value as 'display'|'both'|'prompt' })"
          >
            <option value="display">{{ $t('settings.regexPhaseDisplay') }}</option>
            <option value="both">{{ $t('settings.regexPhaseBoth') }}</option>
            <option value="prompt">{{ $t('settings.regexPhasePrompt') }}</option>
          </select>
```

- [ ] **Step 6: Add i18n keys**

In each of `en.ts`, `zh-Hans.ts`, `zh-Hant.ts`, `ja.ts`, under `settings:`, replace `regexDisplayOnly` with:

```ts
    regexPhaseDisplay: 'Display only',   // en; zh-Hans '仅显示'; zh-Hant '僅顯示'; ja '表示のみ'
    regexPhaseBoth: 'Display + prompt',  // '显示 + prompt' / '顯示 + prompt' / '表示＋prompt'
    regexPhasePrompt: 'Prompt only',     // '仅 prompt' / '僅 prompt' / 'promptのみ'
```

- [ ] **Step 7: Run UI checks**

Run (from `shirita-ui`): `npx vue-tsc --noEmit && npx vitest run regexRule && npx vitest run locales`
Expected: PASS (incl. `parity.test.ts`).

- [ ] **Step 8: Commit**

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/utils/regexRule.ts shirita-ui/src/utils/regexRule.test.ts shirita-ui/src/components/RegexRuleEditor.vue shirita-ui/src/locales/
git commit -m "feat(ui): expose regex display/both/prompt phase in the editor"
```

---

### Task 3: Compact, scope-aware regex list

**Files:**
- Modify: `shirita-ui/src/api/client.ts` (add `getRegexScopes`)
- Modify: `shirita-ui/src/api/types.ts` (add `RegexScope`)
- Modify: `shirita-ui/src/views/SettingsView.vue` (fetch scopes, sort, render compact list, search/hide-disabled, single-open)
- Modify: `shirita-ui/src/components/RegexRuleEditor.vue` (compact row: source label, badges, invalid flag; controlled `open`)
- Modify: `shirita-ui/src/locales/*` (new keys)
- Test: extend `shirita-ui/src/components/` test if one exists for RegexRuleEditor, else a SettingsView-level assertion.

**Interfaces:**
- Consumes: `GET /regex-rules/scopes` (Task 1).
- Produces: `getRegexScopes(): Promise<RegexScope[]>`; `RegexScope { id: string; scope: 'global'|'template'; template_names: string[]; pattern_error: string | null }`.

- [ ] **Step 1: Add the client fn + type**

In `types.ts`:

```ts
export interface RegexScope {
  id: string
  scope: 'global' | 'template'
  template_names: string[]
  pattern_error: string | null
}
```

In `client.ts`:

```ts
export function getRegexScopes(): Promise<import('./types').RegexScope[]> {
  return apiGet('/regex-rules/scopes')
}
```

(Use the same GET helper the file already uses — match the existing `listDefinitions` style: `fetch(`${BASE}/api/regex-rules/scopes`, { headers: authHeaders() })` then `.json()`.)

- [ ] **Step 2: SettingsView — load + sort + render**

In `SettingsView.vue` `<script setup>`:
- import `getRegexScopes`, `RegexScope`.
- add `const regexScopes = ref<Record<string, RegexScope>>({})`.
- in the regex load (`onMounted`/loader where `regexRules` is set), also `const sc = await getRegexScopes(); regexScopes.value = Object.fromEntries(sc.map((s) => [s.id, s]))`.
- add UI state: `const regexSearch = ref(''); const hideDisabled = ref(false); const openRuleId = ref<string | null>(null)`.
- add a computed ordered+filtered list:

```ts
const visibleRegexRules = computed(() => {
  const q = regexSearch.value.trim().toLowerCase()
  return [...regexRules.value]
    .filter((r) => (hideDisabled.value ? (r.meta as Record<string, unknown>).disabled !== true : true))
    .filter((r) => !q || r.name.toLowerCase().includes(q))
    .sort((a, b) => {
      const ga = regexScopes.value[a.id]?.scope === 'global' ? 0 : 1
      const gb = regexScopes.value[b.id]?.scope === 'global' ? 0 : 1
      return ga - gb || a.name.localeCompare(b.name)
    })
})
```

In the template, replace the `v-for="rule in regexRules"` block: add a header row with a search `<input v-model="regexSearch">` and a hide-disabled toggle, then `v-for="rule in visibleRegexRules"` passing the new props:

```html
<RegexRuleEditor
  v-for="rule in visibleRegexRules"
  :key="rule.id"
  :rule="metaToRule(rule)"
  :scope="regexScopes[rule.id]?.scope ?? 'global'"
  :source-names="regexScopes[rule.id]?.template_names ?? []"
  :pattern-error="regexScopes[rule.id]?.pattern_error ?? null"
  :open="openRuleId === rule.id"
  @toggle-open="openRuleId = openRuleId === rule.id ? null : rule.id"
  ... (existing @update:* / @delete handlers unchanged) ...
/>
```

After create/delete, refresh scopes: `regexScopes.value[created.id] = { id: created.id, scope: 'global', template_names: [], pattern_error: null }` on create; `delete regexScopes.value[rule.id]` on delete.

- [ ] **Step 3: RegexRuleEditor — compact row, badges, controlled open**

Update `RegexRuleEditor.vue`:
- props: add `scope: 'global' | 'template'`, `sourceNames: string[]`, `patternError: string | null`, `open: boolean`; emit `toggleOpen`.
- remove the internal `expanded` ref; drive expansion from `props.open`; the chevron button emits `toggleOpen`.
- root element background by scope:

```html
<div :class="['rounded-lg mb-1.5 border', scope === 'global'
        ? 'bg-primary/5 border-primary/20' : 'bg-surface/60 border-line']">
```

- in the header row, after the name, add source label (templates) + badges + invalid flag:

```html
<span v-if="scope === 'template' && sourceNames.length" class="text-[11px] text-mauve/80 truncate">
  {{ sourceNames.join(', ') }}
</span>
<span class="text-[10px] text-muted/70 uppercase">
  {{ rule.scope.ai_output ? 'AI' : '' }}{{ rule.scope.ai_output && rule.scope.user_input ? '·' : '' }}{{ rule.scope.user_input ? $t('settings.regexUserShort') : '' }}
  · {{ $t('settings.regexPhase_' + rule.scope.phase) }}
</span>
<span v-if="patternError" class="text-[11px] text-coral" :title="patternError">⚠ {{ $t('settings.regexInvalid') }}</span>
```

- dim disabled rows: add `:class="{ 'opacity-50': !rule.enabled }"` on the header row.
- the expand toggle button: `@click="emit('toggleOpen')"`, `:class="open ? '' : '-rotate-90'"`; the expanded panel `v-if="open"`.

- [ ] **Step 4: i18n keys**

Add to all four locales under `settings:`: `regexUserShort` ('User'/'用户'/'用戶'/'ユーザー'), `regexPhase_display`/`regexPhase_both`/`regexPhase_prompt` (short badge forms), `regexInvalid` ('invalid'/'失效'/'失效'/'無効'), `regexSearch` placeholder, `regexHideDisabled`.

- [ ] **Step 5: Run UI checks**

Run (from `shirita-ui`): `npx vue-tsc --noEmit && npx vitest run`
Expected: PASS (incl. `parity.test.ts`); fix any RegexRuleEditor test that relied on the old internal `expanded`/`display_only`.

- [ ] **Step 6: Visual check against the running app**

With the dev servers up (backend + `npm run dev`), open Settings · Regex: confirm global rules sit on top with the distinct background, loreset rules show their card name, one row opens at a time, disabled rows are dimmed, search + hide-disabled work, an invalid pattern shows the ⚠ flag. Tune Tailwind spacing/colors here.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/
git commit -m "feat(ui): compact scope-aware regex management list"
```

---

## Self-Review

- **Spec coverage (§3 + §2.5):** scopes+validity endpoint (Task 1, §3.3) ✓; global pinned + background color + source label + badges + hide-disabled + search + invalid flag + single-open accordion (Task 3, §3.2/§3.4) ✓; prompt/both/display three-way (Task 2, §2.5) ✓.
- **Placeholders:** Task 1 Step 7 and Task 3 Step 6 reference the existing web-test harness and live visual tuning — behavior and assertions are specified; exact Tailwind values are expected to be tuned against the running app (frontend-design).
- **Type consistency:** `RegexScope` shape matches between web `RegexScope` (Task 1) and TS `RegexScope` (Task 3); `RegexRule.scope.phase` defined in Task 2 and consumed in Task 3 badges; `getRegexScopes` return type matches the endpoint.
