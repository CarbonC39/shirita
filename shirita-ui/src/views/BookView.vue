<script setup lang="ts">
import { ref, reactive, onMounted } from 'vue'
import { Check, Upload, Download, Copy, Trash2 } from 'lucide-vue-next'
import { useLibraryStore } from '../stores/library'
import {
  listNodes, createNode, updateNode, deleteNode, reorderNodes, updateDefinition, createDefinition, deleteDefinition,
  createTemplate, updateTemplate, duplicateTemplate, deleteTemplate,
} from '../api/client'
import type { PromptNode, Definition, Trigger } from '../api/types'
import PromptTree from '../components/PromptTree.vue'
import DefinitionEditor from '../components/DefinitionEditor.vue'

const library = useLibraryStore()
const loading = ref(true)
const error = ref<string | null>(null)
const selectedTemplateId = ref<string | null>(null)
const nodes = ref<PromptNode[]>([])
// A new template is composed as a local draft and only persisted on first
// manual Save — so picking "+ New template" never litters the list.
const isDraft = ref(false)
const templateName = ref('')

function blankDef(): Definition { return { id: '', type: 'char', name: '', content: '', meta: {} } }
const editDef = reactive<Definition>(blankDef())
function loadDef(d: Definition) { Object.assign(editDef, { id: d.id, type: d.type, name: d.name, content: d.content, meta: { ...d.meta } }) }

onMounted(async () => {
  try { await Promise.all([library.loadTemplates(), library.loadDefinitions(), library.loadTypes()]) } catch (e) { error.value = (e as Error).message }
  finally { loading.value = false }
})

// ── templates ──────────────────────────────────────────────
async function selectTemplate(id: string) {
  if (id === '__new__') { startDraft(); return }
  isDraft.value = false
  selectedTemplateId.value = id || null
  templateName.value = library.templates.find((t) => t.id === id)?.name ?? ''
  if (id) { try { nodes.value = await listNodes('template', id) } catch { nodes.value = [] } }
  else { nodes.value = [] }
}
function startDraft() {
  isDraft.value = true
  selectedTemplateId.value = null
  templateName.value = 'New template'
  nodes.value = []
}
async function saveDraft() {
  try {
    const t = await createTemplate(templateName.value.trim() || 'New template')
    await library.loadTemplates()
    isDraft.value = false
    selectedTemplateId.value = t.id
    templateName.value = t.name
    nodes.value = await listNodes('template', t.id)
  } catch (e) { error.value = (e as Error).message }
}
async function renameTemplate() {
  if (!selectedTemplateId.value) return
  const name = templateName.value.trim()
  const current = library.templates.find((t) => t.id === selectedTemplateId.value)
  if (!name || !current || name === current.name) { templateName.value = current?.name ?? name; return }
  try { await updateTemplate(selectedTemplateId.value, name); await library.loadTemplates() }
  catch (e) { error.value = (e as Error).message }
}
async function dupTemplate() {
  if (!selectedTemplateId.value) return
  try { const t = await duplicateTemplate(selectedTemplateId.value); await library.loadTemplates(); await selectTemplate(t.id) } catch (e) { error.value = (e as Error).message }
}
async function delTemplate() {
  if (!selectedTemplateId.value) return
  try { await deleteTemplate(selectedTemplateId.value); selectedTemplateId.value = null; isDraft.value = false; templateName.value = ''; nodes.value = []; await library.loadTemplates() } catch (e) { error.value = (e as Error).message }
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
async function createNewPrompt(name: string) {
  if (!selectedTemplateId.value) return
  try {
    const def = await createDefinition({ type: 'prompt', name: name?.trim() || 'New prompt', content: '', meta: {} })
    await library.loadDefinitions()
    await createNode('template', selectedTemplateId.value, { parent_id: null, kind: 'ref', definition_id: def.id })
    await reload()
  } catch (e) { error.value = (e as Error).message }
}
function slugifyType(name: string) {
  const slug = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '')
  return slug || `type-${Date.now().toString(36)}`
}
async function createType(name: string) {
  if (!selectedTemplateId.value || !name.trim()) return
  try {
    const created = await library.addType(slugifyType(name), name.trim())
    await addContainer(created.id)
  } catch (e) { error.value = (e as Error).message }
}
async function createTypeFromEditor(name: string) {
  if (!name.trim()) return
  try { await library.addType(slugifyType(name), name.trim()) } catch (e) { error.value = (e as Error).message }
}
async function deleteTypeFromEditor(id: string) {
  const inUse = library.definitions.some((d) => d.type === id)
  const msg = inUse ? `Delete type "${id}"? Definitions using it will keep the type id but it won't be selectable.` : `Delete type "${id}"?`
  if (!confirm(msg)) return
  try { await library.removeType(id) } catch (e) { error.value = (e as Error).message }
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
        <select :value="isDraft ? '__new__' : (selectedTemplateId ?? '')" class="field flex-1" @change="selectTemplate(($event.target as HTMLSelectElement).value)">
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

      <!-- name + save/saved state -->
      <div v-if="isDraft || selectedTemplateId" class="flex items-center gap-2 mt-2 mb-3.5">
        <input v-model="templateName" type="text" class="field flex-1" placeholder="Template name"
          @change="renameTemplate" @keydown.enter="(($event.target as HTMLInputElement).blur())" />
        <button v-if="isDraft" class="btn btn-primary shrink-0" @click="saveDraft">Save</button>
        <span v-else class="flex items-center gap-1.5 text-primary shrink-0">
          <Check :size="13" :stroke-width="2.4" />
          <span class="text-[11.5px] text-muted">Saved</span>
        </span>
      </div>

      <p v-if="isDraft" class="text-muted text-[13px] py-4">Save this template to start building its node tree.</p>
      <PromptTree v-if="selectedTemplateId" :nodes="nodes" :definitions="library.definitions" :types="library.containerTypes"
        @toggle-enabled="handleToggleEnabled"
        @add-prompt="addPrompt" @add-container="addContainer" @add-ref-to-container="addRefToContainer"
        @create-new-prompt="createNewPrompt" @create-new-in-container="createNewInContainer" @create-type="createType"
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
        @create-type="createTypeFromEditor"
        @delete-type="deleteTypeFromEditor"
      />

      <p v-if="error" class="text-coral text-sm mt-4">{{ error }}</p>
    </template>
  </div>
</template>
