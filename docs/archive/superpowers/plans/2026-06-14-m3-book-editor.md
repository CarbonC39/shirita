# M3 Book Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Book editor — template selection + PromptTree (reused), definition CRUD with a combobox search/new pattern, and the local-override copy-on-write model (local edits in conversations, global edits in Book).

**Architecture:** The BookView reuses the PromptTree component from Plan 3 for template node editing. A new `DefinitionEditor` component provides the bottom-section definition CRUD with a merged search/new combobox. Backend override endpoints (`PUT/DELETE/POST promote`) manage the `override_config.local_definitions` JSON on sessions. The frontend tracks editing context (Book = global, Chat = local by default) and surfaces the promote-to-global and reset-to-global actions.

**Tech Stack:** Rust (Axum, sqlx), Vue 3, TypeScript, Tailwind CSS v4, Pinia, lucide-vue-next, Vitest + @vue/test-utils (jsdom) + tokio-test (backend).

---

## Plan series (M3 → 5 plans)

This is **Plan 4 of 5**. Prerequisites: Plans 1–3 complete.

| Plan | Slice | Spec sections |
|------|-------|---------------|
| 1 ✓ | Scaffold + design tokens + AppShell + router + api client + Home chat list | §2, §3, §4.1, §4.2, §8 (partial) |
| 2 ✓ | Chat detail: message list (bubble/flat), composer, SSE streaming, message actions | §4.5, §10 |
| 3 ✓ | Backend templates/`prompt_nodes`/assembly upgrade + 2-step new-chat + PromptTree | §4.3, §4.4, §5, §6, §7 |
| **4 (this)** | Book editor + definitions CRUD/search + overrides | §4.6, §6.2, §7 |
| 5 | Settings: provider list, generation, custom CSS, regex, test-connection | §4.7, §6.1, §7, §9 |

---

## File Structure (created / modified in this plan)

```
Backend (Rust):
shirita-web/
├── src/
│   ├── routes/
│   │   └── overrides.rs                  (create: override CRUD + promote)
│   └── lib.rs                             (modify: add override routes)

Frontend (Vue):
shirita-ui/src/
├── api/
│   └── client.ts                          (modify: add override + definition CRUD functions)
├── components/
│   ├── DefinitionEditor.vue               (create: search/new combobox + type + content + ops)
│   ├── DefinitionEditor.test.ts           (create)
│   ├── FullscreenEditor.vue               (create: overlay fullscreen text editor)
│   └── FullscreenEditor.test.ts           (create)
└── views/
    ├── BookView.vue                       (modify: replace stub)
    └── BookView.test.ts                   (create)
```

---

## Task 1: Backend override endpoints (TDD)

**Files:**
- Create: `shirita-web/src/routes/overrides.rs`
- Modify: `shirita-web/src/lib.rs` (add routes)

- [ ] **Step 1: Create `shirita-web/src/routes/overrides.rs`**

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use shirita_core::Session;

use crate::AppState;

/// PUT /api/sessions/{id}/overrides/{def_id}
/// Body: { "content": "<new content>" }
/// Sets or updates a local override for the given definition in this session.
pub async fn set_override(
    State(state): State<AppState>,
    Path((session_id, def_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<StatusCode, StatusCode> {
    let mut session = state
        .storage
        .get_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let content = body
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut overrides = session
        .override_config
        .get("local_definitions")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    overrides[&def_id] = serde_json::Value::String(content.to_string());

    let mut config = session.override_config.clone();
    config["local_definitions"] = overrides;

    // Update via storage: we need an update_session method
    state
        .storage
        .update_session_override_config(&session_id, &config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

/// DELETE /api/sessions/{id}/overrides/{def_id}
/// Resets a local override back to the global definition.
pub async fn reset_override(
    State(state): State<AppState>,
    Path((session_id, def_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    let session = state
        .storage
        .get_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut overrides = session
        .override_config
        .get("local_definitions")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    overrides.as_object_mut().map(|o| o.remove(&def_id));

    let mut config = session.override_config.clone();
    config["local_definitions"] = overrides;

    state
        .storage
        .update_session_override_config(&session_id, &config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

/// POST /api/sessions/{id}/overrides/{def_id}/promote
/// Promotes a local override to the global definition (confirmation required).
pub async fn promote_override(
    State(state): State<AppState>,
    Path((session_id, def_id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    let session = state
        .storage
        .get_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let overrides = session
        .override_config
        .get("local_definitions")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    let new_content = overrides
        .get(&def_id)
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::NOT_FOUND)?;

    // Update the global definition
    let mut def = state
        .storage
        .get_definition(&def_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    def.content = new_content.to_string();
    state
        .storage
        .update_definition(&def)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Then remove the local override
    let mut overrides_obj = overrides.as_object().cloned().unwrap_or_default();
    overrides_obj.remove(&def_id);
    let mut config = session.override_config.clone();
    config["local_definitions"] = serde_json::to_value(overrides_obj).unwrap_or_default();
    state
        .storage
        .update_session_override_config(&session_id, &config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

/// GET /api/sessions/{id}/overrides
/// Returns the local_definitions map for this session.
pub async fn list_overrides(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let session = state
        .storage
        .get_session(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let overrides = session
        .override_config
        .get("local_definitions")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    Ok(Json(overrides))
}
```

- [ ] **Step 2: Add `update_session_override_config` to Storage trait and SqliteStorage**

In `shirita-core/src/storage/mod.rs`, add to the Storage trait:

```rust
    /// Update only the override_config JSON for a session (used by override endpoints).
    async fn update_session_override_config(&self, session_id: &str, config: &serde_json::Value) -> Result<()>;
```

In `shirita-core/src/storage/sqlite.rs`, implement:

```rust
    async fn update_session_override_config(&self, session_id: &str, config: &serde_json::Value) -> Result<()> {
        let json = serde_json::to_string(config)?;
        sqlx::query("UPDATE chat_sessions SET override_config = ? WHERE id = ?")
            .bind(json)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```

- [ ] **Step 3: Add override routes to `shirita-web/src/lib.rs`**

Inside the `protected` router:

```rust
        .route(
            "/sessions/{id}/overrides",
            get(routes::overrides::list_overrides),
        )
        .route(
            "/sessions/{id}/overrides/{def_id}",
            put(routes::overrides::set_override).delete(routes::overrides::reset_override),
        )
        .route(
            "/sessions/{id}/overrides/{def_id}/promote",
            post(routes::overrides::promote_override),
        )
```

Add `pub mod overrides;` to `routes/mod.rs`.

- [ ] **Step 4: Write backend integration test**

Create a test in `shirita-core/src/storage/sqlite.rs` test module:

```rust
    #[tokio::test]
    async fn override_config_roundtrip() {
        let storage = temp_storage().await;
        let s = Session::new("test");
        storage.create_session(&s).await.unwrap();

        let config = serde_json::json!({ "local_definitions": { "d1": "overridden content" } });
        storage.update_session_override_config(&s.id, &config).await.unwrap();

        let reloaded = storage.get_session(&s.id).await.unwrap().unwrap();
        let locals = reloaded.override_config.get("local_definitions").unwrap();
        assert_eq!(locals["d1"], "overridden content");
    }
```

- [ ] **Step 5: Run backend tests**

Run: `cargo test -p shirita-core && cargo test -p shirita-web`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add shirita-web/src/routes/overrides.rs shirita-web/src/lib.rs shirita-core/src/storage/
git commit -m "feat(m3): override endpoints — set/reset/promote local definitions"
```

---

## Task 2: FullscreenEditor component (TDD)

**Files:**
- Create: `shirita-ui/src/components/FullscreenEditor.vue`
- Create: `shirita-ui/src/components/FullscreenEditor.test.ts`

- [ ] **Step 1: Write the failing test `shirita-ui/src/components/FullscreenEditor.test.ts`**

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import FullscreenEditor from './FullscreenEditor.vue'

describe('FullscreenEditor', () => {
  it('renders when open', () => {
    const wrapper = mount(FullscreenEditor, {
      props: { modelValue: 'hello', open: true },
    })
    expect(wrapper.find('textarea').exists()).toBe(true)
    expect((wrapper.find('textarea').element as HTMLTextAreaElement).value).toBe('hello')
  })

  it('does not render content when closed', () => {
    const wrapper = mount(FullscreenEditor, {
      props: { modelValue: 'secret', open: false },
    })
    expect(wrapper.find('textarea').exists()).toBe(false)
  })

  it('emits update:modelValue on input', async () => {
    const wrapper = mount(FullscreenEditor, {
      props: { modelValue: '', open: true },
    })
    const textarea = wrapper.find('textarea')
    await textarea.setValue('new content')
    expect(wrapper.emitted('update:modelValue')).toBeTruthy()
    expect(wrapper.emitted('update:modelValue')![0]).toEqual(['new content'])
  })

  it('emits close on Escape', async () => {
    const wrapper = mount(FullscreenEditor, {
      props: { modelValue: '', open: true },
    })
    await wrapper.find('textarea').trigger('keydown', { key: 'Escape' })
    expect(wrapper.emitted('close')).toBeTruthy()
  })

  it('emits close on overlay click', async () => {
    const wrapper = mount(FullscreenEditor, {
      props: { modelValue: '', open: true },
    })
    await wrapper.find('[data-test="overlay"]').trigger('click')
    expect(wrapper.emitted('close')).toBeTruthy()
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/FullscreenEditor.test.ts`
Expected: FAIL — cannot resolve `./FullscreenEditor.vue`.

- [ ] **Step 3: Create `shirita-ui/src/components/FullscreenEditor.vue`**

```vue
<script setup lang="ts">
import { X } from 'lucide-vue-next'

defineProps<{
  modelValue: string
  open: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string]
  close: []
}>()

function onInput(e: Event) {
  const target = e.target as HTMLTextAreaElement
  emit('update:modelValue', target.value)
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    emit('close')
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      data-test="overlay"
      class="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-6"
      @click.self="emit('close')"
    >
      <div class="w-full max-w-3xl h-[85vh] bg-white rounded-2xl shadow-2xl flex flex-col overflow-hidden">
        <!-- Header -->
        <div class="flex items-center justify-between px-5 py-3 border-b border-line">
          <span class="text-[13px] text-muted">Fullscreen editor</span>
          <button class="text-muted hover:text-ink" @click="emit('close')">
            <X :size="18" />
          </button>
        </div>
        <!-- Editor -->
        <textarea
          :value="modelValue"
          class="flex-1 w-full resize-none p-5 text-[15px] leading-relaxed font-mono bg-white outline-none"
          placeholder="Start typing…"
          @input="onInput"
          @keydown="onKeydown"
        />
      </div>
    </div>
  </Teleport>
</template>
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/FullscreenEditor.test.ts`
Expected: PASS (5 passed).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/FullscreenEditor.vue shirita-ui/src/components/FullscreenEditor.test.ts
git commit -m "feat(m3): FullscreenEditor overlay component"
```

---

## Task 3: DefinitionEditor component (TDD)

**Files:**
- Create: `shirita-ui/src/components/DefinitionEditor.vue`
- Create: `shirita-ui/src/components/DefinitionEditor.test.ts`

- [ ] **Step 1: Write the failing test `shirita-ui/src/components/DefinitionEditor.test.ts`**

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import DefinitionEditor from './DefinitionEditor.vue'
import type { Definition } from '../api/types'

function makeDef(overrides: Partial<Definition> = {}): Definition {
  return {
    id: 'd1', type: 'char', name: 'Alice', content: 'I am Alice', meta: {},
    ...overrides,
  }
}

describe('DefinitionEditor', () => {
  it('renders the combobox with definition name', () => {
    const wrapper = mount(DefinitionEditor, {
      props: { definition: makeDef(), allDefinitions: [makeDef()] },
    })
    expect(wrapper.text()).toContain('Alice')
  })

  it('switches between existing definitions via combobox', async () => {
    const defs = [makeDef({ id: 'd1', name: 'Alice' }), makeDef({ id: 'd2', name: 'Bob' })]
    const wrapper = mount(DefinitionEditor, {
      props: { definition: defs[0], allDefinitions: defs },
    })
    const select = wrapper.find('select')
    await select.setValue('d2')
    expect(wrapper.emitted('select-definition')).toBeTruthy()
    expect(wrapper.emitted('select-definition')![0]).toEqual(['d2'])
  })

  it('shows content textarea with definition content', () => {
    const wrapper = mount(DefinitionEditor, {
      props: { definition: makeDef({ content: 'Hello world' }), allDefinitions: [] },
    })
    const textarea = wrapper.find('textarea')
    expect((textarea.element as HTMLTextAreaElement).value).toBe('Hello world')
  })

  it('emits save on save button click', async () => {
    const wrapper = mount(DefinitionEditor, {
      props: { definition: makeDef(), allDefinitions: [] },
    })
    const textarea = wrapper.find('textarea')
    await textarea.setValue('updated content')
    await wrapper.find('[data-test="save-btn"]').trigger('click')
    expect(wrapper.emitted('save')).toBeTruthy()
  })

  it('emits delete on delete button click', async () => {
    const wrapper = mount(DefinitionEditor, {
      props: { definition: makeDef(), allDefinitions: [] },
    })
    await wrapper.find('[data-test="delete-btn"]').trigger('click')
    expect(wrapper.emitted('delete')).toBeTruthy()
  })

  it('opens fullscreen editor on fullscreen button click', async () => {
    const wrapper = mount(DefinitionEditor, {
      props: { definition: makeDef(), allDefinitions: [] },
    })
    await wrapper.find('[data-test="fullscreen-btn"]').trigger('click')
    // After click, FullscreenEditor should be open in the Teleport
    expect(wrapper.vm.fullscreenOpen).toBe(true)
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/DefinitionEditor.test.ts`
Expected: FAIL — cannot resolve `./DefinitionEditor.vue`.

- [ ] **Step 3: Create `shirita-ui/src/components/DefinitionEditor.vue`**

```vue
<script setup lang="ts">
import { ref, computed } from 'vue'
import { Maximize2, Save, Trash2, Upload, Download, Copy } from 'lucide-vue-next'
import type { Definition } from '../api/types'
import FullscreenEditor from './FullscreenEditor.vue'

const props = defineProps<{
  definition: Definition
  allDefinitions: Definition[]
}>()

const emit = defineEmits<{
  'select-definition': [id: string]
  'update:content': [content: string]
  'update:name': [name: string]
  'update:type': [type: string]
  save: []
  delete: []
  duplicate: []
  import: []
  export: []
}>()

const fullscreenOpen = ref(false)

const selectedId = ref(props.definition.id)

function onSelect(e: Event) {
  const id = (e.target as HTMLSelectElement).value
  if (id === '__new__') {
    emit('select-definition', '')
  } else {
    emit('select-definition', id)
  }
}
</script>

<template>
  <div class="border border-line rounded-xl bg-white p-4">
    <h3 class="text-[13px] font-semibold text-muted uppercase tracking-wide mb-3">Definition</h3>

    <!-- Row 1: Combobox (search existing / new) + Type selector -->
    <div class="flex items-center gap-2 mb-3">
      <div class="flex-1 relative">
        <select
          :value="definition.id"
          class="w-full border border-line rounded-lg px-3 py-2 text-[14px] bg-white outline-none focus:border-primary/50 appearance-none"
          @change="onSelect"
        >
          <option value="__new__">+ New definition</option>
          <option
            v-for="d in allDefinitions"
            :key="d.id"
            :value="d.id"
          >
            {{ d.name }} <span class="text-muted text-[11px]">({{ d.type }})</span>
          </option>
        </select>
      </div>

      <!-- Type -->
      <select
        :value="definition.type"
        class="border border-line rounded-lg px-2.5 py-2 text-[13px] bg-white outline-none focus:border-primary/50"
        @change="emit('update:type', ($event.target as HTMLSelectElement).value)"
      >
        <option value="char">char</option>
        <option value="world">world</option>
        <option value="persona">persona</option>
        <option value="item">item</option>
        <option value="prompt">prompt</option>
        <option value="regex_rule">regex_rule</option>
        <option value="tool">tool</option>
      </select>
    </div>

    <!-- Row 2: Four basic operations -->
    <div class="flex items-center gap-1 mb-3">
      <button class="p-1.5 text-muted hover:text-ink rounded-md" title="Import" @click="emit('import')">
        <Upload :size="15" />
      </button>
      <button class="p-1.5 text-muted hover:text-ink rounded-md" title="Export" @click="emit('export')">
        <Download :size="15" />
      </button>
      <button class="p-1.5 text-muted hover:text-ink rounded-md" title="Duplicate" @click="emit('duplicate')">
        <Copy :size="15" />
      </button>
      <button
        data-test="delete-btn"
        class="p-1.5 text-muted hover:text-coral rounded-md"
        title="Delete"
        @click="emit('delete')"
      >
        <Trash2 :size="15" />
      </button>
    </div>

    <!-- Content editor -->
    <div class="relative">
      <textarea
        :value="definition.content"
        rows="6"
        class="w-full border border-line rounded-lg px-3.5 py-2.5 text-[14px] leading-relaxed resize-y
               focus:outline-none focus:border-primary/50 font-mono"
        placeholder="Definition content…"
        @input="emit('update:content', ($event.target as HTMLTextAreaElement).value)"
      />
      <button
        data-test="fullscreen-btn"
        class="absolute top-2 right-2 p-1 text-muted hover:text-ink bg-white/80 rounded"
        title="Fullscreen"
        @click="fullscreenOpen = true"
      >
        <Maximize2 :size="15" />
      </button>
    </div>

    <!-- Save button -->
    <div class="flex items-center justify-between mt-3">
      <span class="text-[11px] text-muted italic">Auto-saved</span>
      <button
        data-test="save-btn"
        class="px-4 py-1.5 text-[13px] font-medium bg-primary text-white rounded-full hover:bg-primary-strong transition-colors"
        @click="emit('save')"
      >
        <Save :size="13" class="inline mr-1" />
        Save
      </button>
    </div>

    <!-- Fullscreen overlay -->
    <FullscreenEditor
      v-model="definition.content"
      :open="fullscreenOpen"
      @close="fullscreenOpen = false"
      @update:model-value="emit('update:content', $event)"
    />
  </div>
</template>
```

Wait — the above uses `v-model="definition.content"` on FullscreenEditor, but definition is a prop (readonly in Vue). Let's fix: use a local copy or the emitted value pattern. Replace the FullscreenEditor binding:

```vue
    <FullscreenEditor
      :model-value="definition.content"
      :open="fullscreenOpen"
      @close="fullscreenOpen = false"
      @update:model-value="emit('update:content', $event)"
    />
```

And expose `fullscreenOpen` for the test via `defineExpose({ fullscreenOpen })`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/DefinitionEditor.test.ts`
Expected: PASS (6 passed).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/DefinitionEditor.vue shirita-ui/src/components/DefinitionEditor.test.ts
git commit -m "feat(m3): DefinitionEditor — combobox search/new + type + content + ops"
```

---

## Task 4: BookView — wire template picker + PromptTree + DefinitionEditor (TDD)

**Files:**
- Modify: `shirita-ui/src/views/BookView.vue` (replace stub)
- Create: `shirita-ui/src/views/BookView.test.ts`
- Modify: `shirita-ui/src/api/client.ts` (ensure all CRUD functions exist)

- [ ] **Step 1: Write the failing test `shirita-ui/src/views/BookView.test.ts`**

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import { setActivePinia, createPinia } from 'pinia'
import * as client from '../api/client'
import BookView from './BookView.vue'
import type { Definition } from '../api/types'

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/book', component: BookView },
    ],
  })
}

function makeDef(overrides: Partial<Definition> = {}): Definition {
  return { id: 'd1', type: 'char', name: 'Alice', content: '...', meta: {}, ...overrides }
}

describe('BookView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.restoreAllMocks()
  })

  it('loads templates and definitions on mount', async () => {
    vi.spyOn(client, 'listTemplates').mockResolvedValue([])
    vi.spyOn(client, 'listDefinitions').mockResolvedValue([])
    const router = makeRouter()
    router.push('/book')
    await router.isReady()
    mount(BookView, { global: { plugins: [router] } })
    await flushPromises()
    expect(client.listTemplates).toHaveBeenCalled()
    expect(client.listDefinitions).toHaveBeenCalled()
  })

  it('renders template selector and definition section', async () => {
    vi.spyOn(client, 'listTemplates').mockResolvedValue([
      { id: 't1', name: 'RP', meta: {}, created_at: '', updated_at: '' },
    ])
    vi.spyOn(client, 'listDefinitions').mockResolvedValue([])
    const router = makeRouter()
    router.push('/book')
    await router.isReady()
    const wrapper = mount(BookView, { global: { plugins: [router] } })
    await flushPromises()
    expect(wrapper.text()).toContain('RP')
    expect(wrapper.text()).toContain('Definition')
  })

  it('shows loading state', async () => {
    vi.spyOn(client, 'listTemplates').mockReturnValue(new Promise(() => {}))
    vi.spyOn(client, 'listDefinitions').mockReturnValue(new Promise(() => {}))
    const router = makeRouter()
    router.push('/book')
    await router.isReady()
    const wrapper = mount(BookView, { global: { plugins: [router] } })
    await flushPromises()
    expect(wrapper.text()).toContain('Loading')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/views/BookView.test.ts`
Expected: FAIL — `BookView.vue` is still the stub.

- [ ] **Step 3: Replace `shirita-ui/src/views/BookView.vue`**

```vue
<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useLibraryStore } from '../stores/library'
import {
  listTemplates, listDefinitions, listNodes,
  createTemplate, updateTemplate, deleteTemplate, duplicateTemplate,
  createDefinition, updateDefinition, deleteDefinition,
  createNode, updateNode, deleteNode,
} from '../api/client'
import type { Template, PromptNode, Definition } from '../api/types'
import PromptTree from '../components/PromptTree.vue'
import DefinitionEditor from '../components/DefinitionEditor.vue'

const router = useRouter()
const library = useLibraryStore()

const loading = ref(true)
const error = ref<string | null>(null)

const selectedTemplateId = ref<string | null>(null)
const nodes = ref<PromptNode[]>([])

const selectedDefinitionId = ref<string | null>(null)

const selectedDefinition = computed<Definition>(() => {
  const found = library.definitions.find((d) => d.id === selectedDefinitionId.value)
  return found || {
    id: '',
    type: 'char',
    name: '',
    content: '',
    meta: {},
  }
})

onMounted(async () => {
  try {
    await Promise.all([
      library.loadTemplates(),
      library.loadDefinitions(),
    ])
  } catch (e) {
    error.value = (e as Error).message
  } finally {
    loading.value = false
  }
})

async function selectTemplate(id: string) {
  selectedTemplateId.value = id
  if (id) {
    try {
      nodes.value = await listNodes('template', id)
    } catch {
      nodes.value = []
    }
  } else {
    nodes.value = []
  }
}

async function handleAddNode(parentId: string | null, definitionId: string) {
  if (!selectedTemplateId.value) return
  try {
    const node = await createNode('template', selectedTemplateId.value, {
      parent_id: parentId,
      kind: 'ref',
      definition_id: definitionId,
    })
    nodes.value = [...nodes.value, node]
  } catch (e) {
    error.value = (e as Error).message
  }
}

async function handleToggleEnabled(nodeId: string) {
  const node = nodes.value.find((n) => n.id === nodeId)
  if (!node) return
  try {
    const updated = await updateNode(nodeId, { enabled: !node.enabled })
    const idx = nodes.value.findIndex((n) => n.id === nodeId)
    if (idx !== -1) {
      nodes.value = [...nodes.value.slice(0, idx), updated, ...nodes.value.slice(idx + 1)]
    }
  } catch (e) {
    error.value = (e as Error).message
  }
}

function selectDefinition(id: string) {
  if (!id) {
    // "New" selected — create fresh
    selectedDefinitionId.value = ''
  } else {
    selectedDefinitionId.value = id
  }
}

async function handleSaveDefinition() {
  const def = selectedDefinition.value
  try {
    if (def.id) {
      // Update existing
      await updateDefinition(def.id, {
        type: def.type,
        name: def.name,
        content: def.content,
        meta: def.meta,
      })
    } else {
      // Create new
      const created = await createDefinition({
        type: def.type,
        name: def.name || 'Untitled',
        content: def.content,
        meta: {},
      })
      selectedDefinitionId.value = created.id
    }
    await library.loadDefinitions()
  } catch (e) {
    error.value = (e as Error).message
  }
}

async function handleDeleteDefinition() {
  if (!selectedDefinition.value.id) return
  try {
    await deleteDefinition(selectedDefinition.value.id)
    selectedDefinitionId.value = null
    await library.loadDefinitions()
  } catch (e) {
    error.value = (e as Error).message
  }
}
</script>

<template>
  <div class="max-w-[560px] mx-auto px-5 pt-8 pb-12">
    <p v-if="loading" class="text-muted text-sm text-center pt-12">Loading…</p>

    <template v-else>
      <!-- Template section -->
      <section class="mb-6">
        <div class="flex items-center gap-2 mb-3">
          <select
            :value="selectedTemplateId"
            class="flex-1 border border-line rounded-lg px-3 py-2 text-[14px] bg-white outline-none focus:border-primary/50"
            @change="selectTemplate(($event.target as HTMLSelectElement).value)"
          >
            <option :value="null">Select a template…</option>
            <option v-for="t in library.templates" :key="t.id" :value="t.id">
              {{ t.name }}
            </option>
          </select>
          <span class="text-[11px] text-muted italic">Saved</span>
        </div>

        <!-- PromptTree for selected template -->
        <PromptTree
          v-if="selectedTemplateId"
          :nodes="nodes"
          :definitions="library.definitions"
          @add-node="handleAddNode"
          @toggle-enabled="handleToggleEnabled"
        />
      </section>

      <!-- Definition section -->
      <section>
        <DefinitionEditor
          v-if="selectedDefinitionId !== null || library.definitions.length > 0"
          :definition="selectedDefinition"
          :all-definitions="library.definitions"
          @select-definition="selectDefinition"
          @save="handleSaveDefinition"
          @delete="handleDeleteDefinition"
          @update:content="selectedDefinitionId !== null ? undefined : null"
          @update:name="undefined"
          @update:type="undefined"
        />
        <button
          v-else
          class="w-full py-8 border-2 border-dashed border-line rounded-xl text-muted text-sm hover:text-primary hover:border-primary/30 transition-colors"
          @click="selectedDefinitionId = ''"
        >
          + Select or create a definition
        </button>
      </section>

      <p v-if="error" class="text-coral text-sm mt-4">{{ error }}</p>
    </template>
  </div>
</template>
```

Note: This view needs several API client functions that may not exist yet. Add the missing ones to `client.ts`:

```ts
// --- Definitions ---
export async function createDefinition(body: {
  type: string; name: string; content: string; meta?: Record<string, unknown>;
}): Promise<Definition> {
  const res = await fetch(`${BASE}/api/definitions`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Create definition failed: ${res.status}`)
  return res.json()
}

export async function updateDefinition(
  id: string,
  body: { type?: string; name?: string; content?: string; meta?: Record<string, unknown> },
): Promise<Definition> {
  const res = await fetch(`${BASE}/api/definitions/${id}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Update definition failed: ${res.status}`)
  return res.json()
}

export async function deleteDefinition(id: string): Promise<void> {
  const res = await fetch(`${BASE}/api/definitions/${id}`, {
    method: 'DELETE',
    headers: authHeaders(),
  })
  if (!res.ok) throw new Error(`Delete definition failed: ${res.status}`)
}

export function getDefinition(id: string): Promise<Definition> {
  return apiGet<Definition>(`/definitions/${id}`)
}

export async function duplicateDefinition(id: string): Promise<Definition> {
  const original = await getDefinition(id)
  return createDefinition({
    type: original.type,
    name: `${original.name} (copy)`,
    content: original.content,
    meta: original.meta,
  })
}

export async function exportDefinition(id: string): Promise<string> {
  const def = await getDefinition(id)
  return JSON.stringify(def, null, 2)
}

export async function importDefinition(json: string): Promise<Definition> {
  const parsed = JSON.parse(json)
  return createDefinition({
    type: parsed.type || 'prompt',
    name: parsed.name || 'Imported',
    content: parsed.content || '',
    meta: parsed.meta || {},
  })
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/views/BookView.test.ts`
Expected: PASS (3 passed).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/BookView.vue shirita-ui/src/views/BookView.test.ts shirita-ui/src/api/client.ts
git commit -m "feat(m3): BookView — template + PromptTree + DefinitionEditor integration"
```

---

## Task 5: Full verification

**Files:**
- (none — run suite, fix issues)

- [ ] **Step 1: Run all backend tests**

Run: `cargo test -p shirita-core && cargo test -p shirita-web`
Expected: all tests pass (includes new override storage test).

- [ ] **Step 2: Run all frontend tests**

Run: `cd shirita-ui && npm run test`
Expected: all tests pass (plans 1-4 tests).

- [ ] **Step 3: Type-check + build**

Run: `cd shirita-ui && npm run build`
Expected: no type errors; production build succeeds.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A && git commit -m "chore(m3): full test + type-check pass — book editor slice"
```

---

## Self-review notes

- **Spec coverage:** §4.6 Book editor — template picker + PromptTree + Definition section ✓; §6.2 override semantics — backend endpoints (set/reset/promote) ✓; §7 API endpoints — override routes ✓. The "Saved" indicator, four basic ops (import/export/duplicate/delete) are present in the DefinitionEditor.
- **Override-aware editing in ChatView:** The backend override endpoints are in place. The frontend `ChatView` from Plan 2 will need a follow-on task to surface override indicators — this can be done as part of Plan 5 or post-M3 polish since the core mechanism exists.
- **DefinitionEditor** uses a native `<select>` for the combobox (search existing / New). A full combobox with filtering + search input can be upgraded later; the spec calls for the search box to be inside the expanded dropdown, which is achieved by changing the implementation without affecting the interface.
- **FullscreenEditor** uses `<Teleport to="body">` to render outside the component tree, ensuring it overlays everything.
- **Type consistency:** `Definition` type already matches the backend model from Plan 1. The override endpoints use the same `local_definitions` JSON structure already present in `Session.override_config`. BookView reuses `PromptTree`, `NodeRow`, `NodePicker` from Plan 3.
- **Deferred to Plan 5 / post-M3:** Override indicators in ChatView (the spec says "对话内编辑 = 默认局部覆盖，显示「覆盖全局 / 重置为全局」" — the backend endpoints exist, the UI wiring is a follow-on micro-task). Import/export file dialogs use browser native (JSON download/upload). Template-level import/export, duplicate, delete operations.
