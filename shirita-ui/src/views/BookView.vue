<script setup lang="ts">
import { ref, reactive, onMounted } from 'vue'
import { Check, Upload, Download, Copy, Trash2 } from 'lucide-vue-next'
import { useLibraryStore } from '../stores/library'
import {
  listNodes, createNode, updateNode, deleteNode, reorderNodes, updateDefinition, createDefinition, deleteDefinition,
  createTemplate, duplicateTemplate, deleteTemplate,
} from '../api/client'
import type { PromptNode, Definition, Trigger } from '../api/types'
import PromptTree from '../components/PromptTree.vue'
import DefinitionEditor from '../components/DefinitionEditor.vue'

const library = useLibraryStore()
const loading = ref(true)
const error = ref<string | null>(null)
const selectedTemplateId = ref<string | null>(null)
const nodes = ref<PromptNode[]>([])

function blankDef(): Definition { return { id: '', type: 'char', name: '', content: '', meta: {} } }
const editDef = reactive<Definition>(blankDef())
function loadDef(d: Definition) { Object.assign(editDef, { id: d.id, type: d.type, name: d.name, content: d.content, meta: { ...d.meta } }) }

onMounted(async () => {
  try { await Promise.all([library.loadTemplates(), library.loadDefinitions(), library.loadTypes()]) } catch (e) { error.value = (e as Error).message }
  finally { loading.value = false }
})

// ── templates ──────────────────────────────────────────────
async function selectTemplate(id: string) {
  if (id === '__new__') { await newTemplate(); return }
  selectedTemplateId.value = id || null
  if (id) { try { nodes.value = await listNodes('template', id) } catch { nodes.value = [] } }
  else { nodes.value = [] }
}
async function newTemplate() {
  try {
    const t = await createTemplate('New template')
    await library.loadTemplates()
    selectedTemplateId.value = t.id
    nodes.value = await listNodes('template', t.id)
  } catch (e) { error.value = (e as Error).message }
}
async function dupTemplate() {
  if (!selectedTemplateId.value) return
  try { const t = await duplicateTemplate(selectedTemplateId.value); await library.loadTemplates(); await selectTemplate(t.id) } catch (e) { error.value = (e as Error).message }
}
async function delTemplate() {
  if (!selectedTemplateId.value) return
  try { await deleteTemplate(selectedTemplateId.value); selectedTemplateId.value = null; nodes.value = []; await library.loadTemplates() } catch (e) { error.value = (e as Error).message }
}

// ── tree ───────────────────────────────────────────────────
async function reload() {
  if (selectedTemplateId.value) nodes.value = await listNodes('template', selectedTemplateId.value)
}
async function addPrompt(definitionId: string) {
  if (!selectedTemplateId.value) return
  try { await createNode('template', selectedTemplateId.value, { parent_id: null, kind: 'ref', definition_id: definitionId }); await reload() } catch (e) { error.value = (e as Error).message }
}
async function addContainer(typeId: string) {
  if (!selectedTemplateId.value) return
  try { await createNode('template', selectedTemplateId.value, { parent_id: null, kind: 'folder', tag: typeId }); await reload() } catch (e) { error.value = (e as Error).message }
}
async function addRefToContainer(parentId: string, definitionId: string) {
  if (!selectedTemplateId.value) return
  try { await createNode('template', selectedTemplateId.value, { parent_id: parentId, kind: 'ref', definition_id: definitionId }); await reload() } catch (e) { error.value = (e as Error).message }
}
async function createNewPrompt() {
  if (!selectedTemplateId.value) return
  try {
    const def = await createDefinition({ type: 'prompt', name: 'New prompt', content: '', meta: {} })
    await library.loadDefinitions()
    await createNode('template', selectedTemplateId.value, { parent_id: null, kind: 'ref', definition_id: def.id })
    await reload()
  } catch (e) { error.value = (e as Error).message }
}
async function createNewInContainer(parentId: string, typeId: string) {
  if (!selectedTemplateId.value) return
  try {
    const def = await createDefinition({ type: typeId, name: `New ${typeId}`, content: '', meta: {} })
    await library.loadDefinitions()
    await createNode('template', selectedTemplateId.value, { parent_id: parentId, kind: 'ref', definition_id: def.id })
    await reload()
  } catch (e) { error.value = (e as Error).message }
}
async function handleToggleEnabled(nodeId: string) {
  const node = nodes.value.find((n) => n.id === nodeId)
  if (!node) return
  try { const updated = await updateNode(nodeId, { enabled: !node.enabled }); const i = nodes.value.findIndex((n) => n.id === nodeId); if (i !== -1) nodes.value = [...nodes.value.slice(0, i), updated, ...nodes.value.slice(i + 1)] } catch (e) { error.value = (e as Error).message }
}
async function handleUpdateContent(definitionId: string, content: string) {
  try { await updateDefinition(definitionId, { content }); await library.loadDefinitions() } catch (e) { error.value = (e as Error).message }
}
async function handleDeleteNode(nodeId: string) {
  const node = nodes.value.find((n) => n.id === nodeId)
  if (!node) return
  const childCount = nodes.value.filter((n) => n.parent_id === nodeId).length
  if (node.kind === 'folder' && childCount > 0
      && !confirm(`Delete this container and its ${childCount} item(s)?`)) return
  try { await deleteNode(nodeId); await reload() } catch (e) { error.value = (e as Error).message }
}
async function handleReorder(orderedIds: string[]) {
  if (!selectedTemplateId.value) return
  try { await reorderNodes('template', selectedTemplateId.value, orderedIds); await reload() } catch (e) { error.value = (e as Error).message }
}
async function handleUpdateTrigger(definitionId: string, trigger: Trigger) {
  const def = library.definitions.find((d) => d.id === definitionId)
  if (!def) return
  try { await updateDefinition(definitionId, { meta: { ...def.meta, trigger } }); await library.loadDefinitions() } catch (e) { error.value = (e as Error).message }
}

// ── definition editor ──────────────────────────────────────
function selectDefinition(id: string) {
  if (!id) { loadDef(blankDef()); return }
  const found = library.definitions.find((d) => d.id === id)
  if (found) loadDef(found)
}
async function saveDefinition() {
  try {
    if (editDef.id) { await updateDefinition(editDef.id, { type: editDef.type, name: editDef.name, content: editDef.content, meta: editDef.meta }) }
    else { const created = await createDefinition({ type: editDef.type, name: editDef.name || 'Untitled', content: editDef.content, meta: editDef.meta }); editDef.id = created.id }
    await library.loadDefinitions()
  } catch (e) { error.value = (e as Error).message }
}
async function deleteDef() {
  if (!editDef.id) { loadDef(blankDef()); return }
  try { await deleteDefinition(editDef.id); loadDef(blankDef()); await library.loadDefinitions() } catch (e) { error.value = (e as Error).message }
}
async function duplicateDef() {
  try { const created = await createDefinition({ type: editDef.type, name: `${editDef.name || 'Untitled'} copy`, content: editDef.content, meta: editDef.meta }); await library.loadDefinitions(); loadDef(created) } catch (e) { error.value = (e as Error).message }
}
</script>

<template>
  <div class="max-w-[480px] mx-auto px-5 pt-6 pb-12">
    <p v-if="loading" class="text-muted text-sm text-center pt-12">Loading…</p>
    <template v-else>
      <!-- template picker + ops -->
      <div class="flex items-center gap-2">
        <select :value="selectedTemplateId ?? ''" class="field flex-1" @change="selectTemplate(($event.target as HTMLSelectElement).value)">
          <option value="" disabled>Select a template…</option>
          <option value="__new__">+ New template</option>
          <option v-for="t in library.templates" :key="t.id" :value="t.id">{{ t.name }}</option>
        </select>
        <div class="flex items-center">
          <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" title="Import" disabled><Upload :size="16" /></button>
          <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" title="Export" disabled><Download :size="16" /></button>
          <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40" title="Duplicate" :disabled="!selectedTemplateId" @click="dupTemplate"><Copy :size="16" /></button>
          <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-coral rounded-lg disabled:opacity-40" title="Delete" :disabled="!selectedTemplateId" @click="delTemplate"><Trash2 :size="16" /></button>
        </div>
      </div>
      <div v-if="selectedTemplateId" class="flex items-center gap-1.5 mt-2 mb-3.5 ml-0.5 text-primary">
        <Check :size="13" :stroke-width="2.4" />
        <span class="text-[11.5px] text-muted">Saved</span>
      </div>

      <PromptTree v-if="selectedTemplateId" :nodes="nodes" :definitions="library.definitions" :types="library.containerTypes"
        @toggle-enabled="handleToggleEnabled"
        @add-prompt="addPrompt" @add-container="addContainer" @add-ref-to-container="addRefToContainer"
        @create-new-prompt="createNewPrompt" @create-new-in-container="createNewInContainer"
        @update-content="handleUpdateContent" @update-trigger="handleUpdateTrigger" @delete-node="handleDeleteNode" @reorder="handleReorder" />
      <p v-else class="text-muted text-[13px] py-4">Select or create a template to edit its node tree.</p>

      <div class="h-px bg-line my-6" />

      <DefinitionEditor
        :definition="editDef"
        :all-definitions="library.definitions"
        :types="library.containerTypes"
        @select-definition="selectDefinition"
        @update:name="editDef.name = $event"
        @update:type="editDef.type = $event as Definition['type']"
        @update:content="editDef.content = $event"
        @update:meta="editDef.meta = $event"
        @save="saveDefinition"
        @delete="deleteDef"
        @duplicate="duplicateDef"
      />

      <p v-if="error" class="text-coral text-sm mt-4">{{ error }}</p>
    </template>
  </div>
</template>
