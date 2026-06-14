# Prompt Tree v2 — Plan 4: Trigger editor + scan settings

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users author world-book triggers (Constant / Keyword / Random) on definitions — in the book's `DefinitionEditor` and inline on expanded tree refs — and expose the global scan settings (depth + recursive toggle) in Settings, wired so the backend actually reads them.

**Architecture:** A reusable `TriggerEditor` v-models a `{ mode, keys, probability }` object (segmented mode + keyword chips + probability slider). Both `DefinitionEditor` and the expanded `NodeRow` host it; edits persist to `definition.meta.trigger` via `updateDefinition`. Settings gains a "World Info" section writing `worldinfo_scan_depth` / `worldinfo_recursive`; `send_message` reads those (defaults 4 / true) instead of the Plan 1 hard-codes.

**Tech Stack:** Vue 3 `<script setup>` + TS, Pinia, lucide-vue-next, existing `SegmentedControl`/`SliderControl`/`ToggleSwitch`, Vitest + `@vue/test-utils`; Rust (`conversation.rs` + storage settings).

**Spec:** `docs/superpowers/specs/2026-06-13-prompt-tree-worldbook-design.md` §5 (trigger model), §6 (scan settings), §9 (inline trigger), §13. Builds on Plan 1 (`parse_trigger`, `meta.trigger`) and Plan 3 (tree + NodeRow).

**Out of scope:** secondary keys / whole-word / per-entry recursion exclusion (spec defers); in-chat override of triggers (`override_config`) — assembly already honours it (Plan 1), UI deferred.

---

## File structure

- `shirita-ui/src/components/TriggerEditor.vue` — **new**: mode + keys + probability editor.
- `shirita-ui/src/components/DefinitionEditor.vue` — host the editor; emit `update:meta`.
- `shirita-ui/src/views/BookView.vue` — persist `editDef.meta.trigger`; handle `updateTrigger` from the tree.
- `shirita-ui/src/components/NodeRow.vue` — inline `TriggerEditor` in the expanded ref; emit `updateTrigger`.
- `shirita-ui/src/components/PromptTree.vue` — relay `updateTrigger` up.
- `shirita-ui/src/views/SettingsView.vue` — "World Info" section.
- `shirita-core/src/conversation.rs` — read scan settings from storage.

---

## Task 1: `TriggerEditor` component

**Files:**
- Create: `shirita-ui/src/components/TriggerEditor.vue`
- Test: `shirita-ui/src/components/TriggerEditor.test.ts` (new)

- [ ] **Step 1: Write the failing test.** Create `shirita-ui/src/components/TriggerEditor.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import TriggerEditor from './TriggerEditor.vue'

const base = { mode: 'constant' as const, keys: [] as string[], probability: 100 }

describe('TriggerEditor', () => {
  it('switches mode and shows keyword input only for keyword mode', async () => {
    const w = mount(TriggerEditor, { props: { modelValue: base } })
    expect(w.find('[data-test="trigger-keys"]').exists()).toBe(false)
    // click the Keyword segment
    await w.findAll('[data-test="segmented"] button').find((b) => b.text() === 'Keyword')!.trigger('click')
    expect(w.emitted('update:modelValue')!.at(-1)![0]).toMatchObject({ mode: 'keyword' })
  })

  it('adds a keyword chip on Enter', async () => {
    const w = mount(TriggerEditor, { props: { modelValue: { ...base, mode: 'keyword' } } })
    const input = w.find('[data-test="trigger-keys"] input')
    await input.setValue('zion')
    await input.trigger('keydown.enter')
    expect(w.emitted('update:modelValue')!.at(-1)![0]).toMatchObject({ keys: ['zion'] })
  })
})
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/TriggerEditor.test.ts`
Expected: FAIL (component missing).

- [ ] **Step 3: Implement.** Create `shirita-ui/src/components/TriggerEditor.vue`:

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { X } from 'lucide-vue-next'
import SegmentedControl from './SegmentedControl.vue'
import SliderControl from './SliderControl.vue'

export interface Trigger { mode: 'constant' | 'keyword' | 'random'; keys: string[]; probability: number }

const props = defineProps<{ modelValue: Trigger }>()
const emit = defineEmits<{ 'update:modelValue': [value: Trigger] }>()

const draft = ref('')

function patch(p: Partial<Trigger>) {
  emit('update:modelValue', { ...props.modelValue, ...p })
}
function addKey() {
  const k = draft.value.trim()
  if (!k || props.modelValue.keys.includes(k)) { draft.value = ''; return }
  patch({ keys: [...props.modelValue.keys, k] })
  draft.value = ''
}
function removeKey(k: string) {
  patch({ keys: props.modelValue.keys.filter((x) => x !== k) })
}
</script>

<template>
  <div class="space-y-2.5" data-test="trigger-editor">
    <div class="flex items-center gap-2">
      <span class="text-[12px] text-muted">Trigger</span>
      <SegmentedControl
        :model-value="modelValue.mode"
        :options="[
          { value: 'constant', label: 'Constant' },
          { value: 'keyword', label: 'Keyword' },
          { value: 'random', label: 'Random' },
        ]"
        @update:model-value="patch({ mode: $event as Trigger['mode'] })"
      />
    </div>

    <div v-if="modelValue.mode === 'keyword'" data-test="trigger-keys">
      <div class="flex flex-wrap items-center gap-1.5 border border-line rounded-[9px] bg-white px-2.5 py-2">
        <span
          v-for="k in modelValue.keys"
          :key="k"
          class="flex items-center gap-1 bg-mauve/15 text-ink text-[12px] rounded-full pl-2.5 pr-1.5 py-0.5"
        >
          {{ k }}
          <button class="text-muted hover:text-coral" @click="removeKey(k)"><X :size="12" /></button>
        </span>
        <input
          v-model="draft"
          type="text"
          placeholder="Add keyword…"
          class="flex-1 min-w-[80px] text-[13px] bg-transparent outline-none placeholder:text-muted/60"
          @keydown.enter.prevent="addKey"
        />
      </div>
    </div>

    <div v-else-if="modelValue.mode === 'random'">
      <SliderControl
        :model-value="modelValue.probability"
        label="Probability %"
        :min="0"
        :max="100"
        :step="1"
        @update:model-value="patch({ probability: $event })"
      />
    </div>
  </div>
</template>
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cd shirita-ui && npx vitest run src/components/TriggerEditor.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/TriggerEditor.vue shirita-ui/src/components/TriggerEditor.test.ts
git commit -m "feat(ui): TriggerEditor (constant/keyword/random) component"
```

---

## Task 2: Trigger in `DefinitionEditor` (book)

**Files:**
- Modify: `shirita-ui/src/components/DefinitionEditor.vue`, `shirita-ui/src/views/BookView.vue`

- [ ] **Step 1: Add a helper to read a trigger from meta.** In `shirita-ui/src/api/types.ts` (or a small util), add an exported helper used by both hosts:

```ts
import type { Trigger } from '../components/TriggerEditor.vue'
export function triggerFromMeta(meta: Record<string, unknown>): Trigger {
  const t = (meta?.trigger ?? {}) as Partial<Trigger>
  return {
    mode: t.mode === 'keyword' || t.mode === 'random' ? t.mode : 'constant',
    keys: Array.isArray(t.keys) ? t.keys.filter((k): k is string => typeof k === 'string') : [],
    probability: typeof t.probability === 'number' ? t.probability : 100,
  }
}
```

> If importing a type from a `.vue` is awkward for your tsconfig, move the `Trigger` interface into `api/types.ts` and have `TriggerEditor.vue` import it from there. Pick one home for `Trigger` and use it everywhere.

- [ ] **Step 2: Host it in DefinitionEditor.** In `shirita-ui/src/components/DefinitionEditor.vue`:
  - Import `TriggerEditor` + `triggerFromMeta`.
  - Add emit `'update:meta': [meta: Record<string, unknown>]`.
  - After the type chips block, render (only for non-reserved container definitions — triggers are meaningless on `prompt`/`regex_rule`; gate on `definition.type !== 'prompt' && definition.type !== 'regex_rule' && definition.type !== 'tool'`):

```html
    <div v-if="!['prompt','regex_rule','tool'].includes(definition.type)" class="mb-3">
      <TriggerEditor
        :model-value="triggerFromMeta(definition.meta)"
        @update:model-value="emit('update:meta', { ...definition.meta, trigger: $event })"
      />
    </div>
```

- [ ] **Step 3: Persist it in BookView.** In `shirita-ui/src/views/BookView.vue`:
  - On the `<DefinitionEditor>`, add `@update:meta="editDef.meta = $event"`.
  - `saveDefinition` already sends `meta: editDef.meta` — no change needed. Verify the body includes `meta`.

- [ ] **Step 4: Component test.** Add to `shirita-ui/src/components/DefinitionEditor.test.ts` (new file; mount with a `world` definition and assert the TriggerEditor renders):

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import DefinitionEditor from './DefinitionEditor.vue'

const def = { id: 'd', type: 'world', name: 'Zion', content: '', meta: { trigger: { mode: 'keyword', keys: ['zion'], probability: 100 } } }

describe('DefinitionEditor trigger', () => {
  it('shows the trigger editor for a world definition with the existing keyword', () => {
    const w = mount(DefinitionEditor, { props: { definition: def, allDefinitions: [def] } })
    expect(w.find('[data-test="trigger-editor"]').exists()).toBe(true)
    expect(w.text()).toContain('zion')
  })

  it('hides the trigger editor for a prompt definition', () => {
    const p = { ...def, type: 'prompt', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: p, allDefinitions: [p] } })
    expect(w.find('[data-test="trigger-editor"]').exists()).toBe(false)
  })
})
```

- [ ] **Step 5: Run tests, verify pass**

Run: `cd shirita-ui && npx vitest run src/components/DefinitionEditor.test.ts`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/components/DefinitionEditor.vue shirita-ui/src/views/BookView.vue shirita-ui/src/api/types.ts shirita-ui/src/components/DefinitionEditor.test.ts
git commit -m "feat(ui): edit world-book trigger in DefinitionEditor"
```

---

## Task 3: Inline trigger on expanded tree refs

**Files:**
- Modify: `shirita-ui/src/components/NodeRow.vue`, `shirita-ui/src/components/PromptTree.vue`, `shirita-ui/src/views/BookView.vue`

- [ ] **Step 1: Write the failing test.** Add to `shirita-ui/src/components/NodeRow.test.ts`:

```ts
it('shows the trigger editor in an expanded container ref', () => {
  const defs = { d1: { id: 'd1', type: 'world', name: 'Zion', content: 'b', meta: { trigger: { mode: 'keyword', keys: ['zion'], probability: 100 } } } }
  const ref = node({ kind: 'ref', definition_id: 'd1' })
  const w = mount(NodeRow, { props: { node: ref, definitions: defs, depth: 1, isExpanded: true } })
  expect(w.find('[data-test="trigger-editor"]').exists()).toBe(true)
})
```
(reuse the `node`/`defs` helpers already in that file; add a `world` def inline as above.)

- [ ] **Step 2: Run it, verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/NodeRow.test.ts`
Expected: FAIL (no trigger editor in NodeRow).

- [ ] **Step 3: Implement.** In `shirita-ui/src/components/NodeRow.vue`:
  - Import `TriggerEditor` + `triggerFromMeta`. Add emit `updateTrigger: [trigger: import('./TriggerEditor.vue').Trigger]`.
  - In the inline editor block (the `v-if="!isFolder && !isHistory && isExpanded"` div), below the textarea, add — but only when the referenced definition is a container type (not prompt):

```html
        <div v-if="def && !['prompt','regex_rule','tool'].includes(def.type)" class="mt-2.5">
          <TriggerEditor
            :model-value="triggerFromMeta(def.meta)"
            @update:model-value="emit('updateTrigger', $event)"
          />
        </div>
```

- [ ] **Step 4: Relay through PromptTree.** In `shirita-ui/src/components/PromptTree.vue`, add emit `updateTrigger: [definitionId: string, trigger: import('./TriggerEditor.vue').Trigger]` and on every `<NodeRow>`:

```html
        @update-trigger="(t) => node.definition_id && emit('updateTrigger', node.definition_id, t)"
```
(and the child-row variant uses `child.definition_id`).

- [ ] **Step 5: Persist in BookView.** In `shirita-ui/src/views/BookView.vue`, add a handler + wire it on `<PromptTree @update-trigger="handleUpdateTrigger">`:

```ts
async function handleUpdateTrigger(definitionId: string, trigger: import('../components/TriggerEditor.vue').Trigger) {
  const def = library.definitions.find((d) => d.id === definitionId)
  if (!def) return
  try {
    await updateDefinition(definitionId, { meta: { ...def.meta, trigger } })
    await library.loadDefinitions()
  } catch (e) { error.value = (e as Error).message }
}
```

- [ ] **Step 6: Run tests, verify pass**

Run: `cd shirita-ui && npx vitest run src/components/NodeRow.test.ts && npx vitest run src/components/PromptTree.test.ts`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add shirita-ui/src/components/NodeRow.vue shirita-ui/src/components/PromptTree.vue shirita-ui/src/views/BookView.vue
git commit -m "feat(ui): inline trigger editor on expanded tree refs"
```

---

## Task 4: Settings — World Info scan section

**Files:**
- Modify: `shirita-ui/src/views/SettingsView.vue`

- [ ] **Step 1: Add the writable computeds.** In `SettingsView.vue` `<script setup>`, after `customCss`:

```ts
const scanDepth = computed({ get: () => (get('worldinfo_scan_depth') as number) ?? 4, set: (v: number) => set('worldinfo_scan_depth', v) })
const recursiveScan = computed({ get: () => (get('worldinfo_recursive') as boolean) ?? true, set: (v: boolean) => set('worldinfo_recursive', v) })
```

- [ ] **Step 2: Include them in `handleSave`.** Add to the object passed to `settings.save({ … })`:

```ts
      worldinfo_scan_depth: scanDepth.value,
      worldinfo_recursive: recursiveScan.value,
```

- [ ] **Step 3: Add the section markup.** Insert a new `<section>` (after Generation, before Appearance):

```html
      <div class="border-t border-line my-6" />
      <section class="mb-8">
        <h3 class="text-[13px] font-semibold text-muted uppercase tracking-wide mb-4">World Info</h3>
        <div class="flex items-center justify-between mb-4">
          <span class="text-[14px] text-ink">Scan depth</span>
          <input
            data-test="scan-depth"
            :value="scanDepth"
            type="number" min="1" max="50"
            class="w-[88px] border border-line rounded-lg px-3 py-2 text-[14px] text-right tabular-nums outline-none focus:border-primary/50"
            @input="scanDepth = parseInt(($event.target as HTMLInputElement).value) || 1"
          />
        </div>
        <div class="flex items-center justify-between">
          <span class="text-[14px] text-ink">Recursive scan</span>
          <ToggleSwitch :model-value="recursiveScan" @update:model-value="recursiveScan = $event" />
        </div>
      </section>
```

- [ ] **Step 4: Build + test.**

Run: `cd shirita-ui && npx vue-tsc -b && npx vitest run`
Expected: green (no SettingsView test asserts the old layout; if one does, update it).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/SettingsView.vue
git commit -m "feat(ui): Settings — World Info scan depth + recursive toggle"
```

---

## Task 5: Backend reads scan settings

**Files:**
- Modify: `shirita-core/src/conversation.rs`

- [ ] **Step 1: Write the failing test.** Add to the `tests` module in `shirita-core/src/conversation.rs` — a recursive-off setting should prevent a chained keyword entry from activating (so its content is absent from the system prompt). Build a template: a `world` container with two refs — `A` (constant, content mentions "zion") and `B` (keyword `["zion"]`). With `worldinfo_recursive=false`, "zion" appears only via A's content, which is NOT scanned when recursion is off → B excluded.

```rust
    #[tokio::test]
    async fn send_message_respects_recursive_setting() {
        use crate::models::prompt_node::{NodeKind, OwnerKind, PromptNode};
        use crate::models::template::Template;
        let storage = Arc::new(temp_storage().await);
        storage.set_setting("worldinfo_recursive", &serde_json::json!(false)).await.unwrap();

        let a = crate::models::definition::Definition::new("world", "A", "We mention zion here");
        let mut b = crate::models::definition::Definition::new("world", "B", "Zion lore");
        b.meta = serde_json::json!({ "trigger": { "mode": "keyword", "keys": ["zion"] } });
        storage.create_definition(&a).await.unwrap();
        storage.create_definition(&b).await.unwrap();

        let t = Template::new("T");
        storage.create_template(&t).await.unwrap();
        let wf = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 0, "world");
        storage.create_node(&wf).await.unwrap();
        storage.create_node(&PromptNode::new_ref(OwnerKind::Template, &t.id, Some(wf.id.clone()), 0, &a.id)).await.unwrap();
        storage.create_node(&PromptNode::new_ref(OwnerKind::Template, &t.id, Some(wf.id.clone()), 1, &b.id)).await.unwrap();
        let mut hist = PromptNode::new_folder(OwnerKind::Template, &t.id, None, 1, "history");
        hist.kind = NodeKind::History; hist.tag = None;
        storage.create_node(&hist).await.unwrap();

        let mut session = Session::new("s");
        session.template_id = Some(t.id.clone());
        storage.create_session(&session).await.unwrap();

        let seen = Arc::new(Mutex::new(None));
        let provider: Arc<dyn ModelProvider> = Arc::new(RecordingProvider { seen: seen.clone(), reply: "ok".into() });
        let storage_dyn: Arc<dyn Storage> = storage.clone();
        let counter: Arc<dyn TokenCounter> = Arc::new(TiktokenCounter::new());

        // user says nothing about zion → A constant active, B only if recursion scans A's content.
        let stream = send_message(storage_dyn, provider, counter, "m".into(), session.id.clone(), "hello".into());
        futures::pin_mut!(stream);
        while stream.next().await.is_some() {}

        let req = seen.lock().unwrap().clone().unwrap();
        let sys = &req.messages[0].content;
        assert!(sys.contains("We mention zion here"), "constant A present");
        assert!(!sys.contains("Zion lore"), "B must NOT activate with recursion off");
    }
```

- [ ] **Step 2: Run it, verify it fails**

Run: `cargo test -p shirita-core conversation::tests::send_message_respects_recursive_setting`
Expected: FAIL (send_message hard-codes `recursive = true`, so B activates via recursion).

- [ ] **Step 3: Implement.** In `shirita-core/src/conversation.rs` `send_message`, replace the hard-coded `let scan_depth = 4usize;` and the `true` recursive argument with settings reads (place the reads just before the `assemble_from_nodes` call):

```rust
        let scan_depth = storage
            .get_setting("worldinfo_scan_depth")
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_u64())
            .unwrap_or(4) as usize;
        let recursive = storage
            .get_setting("worldinfo_recursive")
            .await
            .ok()
            .flatten()
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
```
Then pass `recursive` (not `true`) as the `recursive` argument to `assemble_from_nodes`, and keep `scan_depth` flowing into the `recent` window + the call. (The `recent` slice already uses `scan_depth`.)

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test -p shirita-core conversation::`
Expected: PASS (incl. existing `assembled_system_is_sent`, which has no setting → defaults to recursive=true).

- [ ] **Step 5: Commit**

```bash
git add shirita-core/src/conversation.rs
git commit -m "feat(core): send_message reads worldinfo_scan_depth/recursive from settings"
```

---

## Task 6: Full verification

- [ ] **Step 1:** `cd shirita-ui && npx vue-tsc -b && npx vitest run` → green.
- [ ] **Step 2:** `cargo test` → green.
- [ ] **Step 3: Manual smoke.** In `/settings`, set Recursive scan off + Scan depth 2, Save. In `/book`, give a `world` definition a Keyword trigger; confirm chips persist on reload. (Optional) send a chat and confirm activation matches the setting.

---

## Self-review checklist

- **Spec coverage (§5, §6, §9):** TriggerEditor 3 modes + keyword chips + probability (T1) ✓ · trigger on definitions in book (T2, §5) ✓ · inline trigger on expanded refs (T3, §9) ✓ · Settings scan depth + recursive toggle, recursive **off-switch** required (T4, §6) ✓ · backend honours both settings (T5, §6) ✓. **Deferred (noted):** secondary keys / whole-word / per-entry recursion; in-chat trigger override UI.
- **Placeholder scan:** all steps carry real code + tests.
- **Type consistency:** `Trigger{mode,keys,probability}` (single home — `api/types.ts` or `TriggerEditor.vue`, chosen in T2 Step 1), `triggerFromMeta(meta)`, emits `update:meta` (DefinitionEditor) + `updateTrigger(defId,trigger)` (tree), settings keys `worldinfo_scan_depth`/`worldinfo_recursive`, `send_message` reads them. Names identical across tasks.
- **`v-html`:** none.
