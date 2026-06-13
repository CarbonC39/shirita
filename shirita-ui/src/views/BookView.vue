<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useLibraryStore } from '../stores/library'
import { listNodes, createNode, updateNode, deleteNode, createDefinition, updateDefinition, deleteDefinition } from '../api/client'
import type { PromptNode, Definition } from '../api/types'
import PromptTree from '../components/PromptTree.vue'
import DefinitionEditor from '../components/DefinitionEditor.vue'

const library = useLibraryStore()
const loading = ref(true)
const error = ref<string | null>(null)
const selectedTemplateId = ref<string | null>(null)
const nodes = ref<PromptNode[]>([])
const selectedDefinitionId = ref<string | null>(null)

const selectedDefinition = computed<Definition>(() => {
  const found = library.definitions.find(d => d.id === selectedDefinitionId.value)
  return found || { id: '', type: 'char', name: '', content: '', meta: {} }
})

onMounted(async () => {
  try { await Promise.all([library.loadTemplates(), library.loadDefinitions()]) } catch (e) { error.value = (e as Error).message }
  finally { loading.value = false }
})

async function selectTemplate(id: string) {
  selectedTemplateId.value = id
  if (id) { try { nodes.value = await listNodes('template', id) } catch { nodes.value = [] } }
  else { nodes.value = [] }
}

async function handleAddNode(parentId: string | null, definitionId: string) {
  if (!selectedTemplateId.value) return
  try { const node = await createNode('template', selectedTemplateId.value, { parent_id: parentId, kind: 'ref', definition_id: definitionId }); nodes.value = [...nodes.value, node] } catch (e) { error.value = (e as Error).message }
}

async function handleToggleEnabled(nodeId: string) {
  const node = nodes.value.find(n => n.id === nodeId)
  if (!node) return
  try { const updated = await updateNode(nodeId, { enabled: !node.enabled }); const idx = nodes.value.findIndex(n => n.id === nodeId); if (idx !== -1) nodes.value = [...nodes.value.slice(0, idx), updated, ...nodes.value.slice(idx + 1)] } catch (e) { error.value = (e as Error).message }
}

function selectDefinition(id: string) { selectedDefinitionId.value = id || '' }

async function handleSaveDefinition() {
  const def = selectedDefinition.value
  try {
    if (def.id) { await updateDefinition(def.id, { type: def.type, name: def.name, content: def.content, meta: def.meta }) }
    else { const created = await createDefinition({ type: def.type, name: def.name || 'Untitled', content: def.content, meta: {} }); selectedDefinitionId.value = created.id }
    await library.loadDefinitions()
  } catch (e) { error.value = (e as Error).message }
}

async function handleDeleteDefinition() {
  if (!selectedDefinition.value.id) return
  try { await deleteDefinition(selectedDefinition.value.id); selectedDefinitionId.value = null; await library.loadDefinitions() } catch (e) { error.value = (e as Error).message }
}
</script>

<template>
  <div class="max-w-[560px] mx-auto px-5 pt-8 pb-12">
    <p v-if="loading" class="text-muted text-sm text-center pt-12">Loading…</p>
    <template v-else>
      <section class="mb-6">
        <div class="flex items-center gap-2 mb-3">
          <select :value="selectedTemplateId" class="flex-1 border border-line rounded-lg px-3 py-2 text-[14px] bg-white outline-none focus:border-primary/50" @change="selectTemplate(($event.target as HTMLSelectElement).value)">
            <option :value="null">Select a template…</option>
            <option v-for="t in library.templates" :key="t.id" :value="t.id">{{ t.name }}</option>
          </select>
          <span class="text-[11px] text-muted italic">Saved</span>
        </div>
        <PromptTree v-if="selectedTemplateId" :nodes="nodes" :definitions="library.definitions" @add-node="handleAddNode" @toggle-enabled="handleToggleEnabled" />
      </section>
      <section>
        <DefinitionEditor v-if="selectedDefinitionId !== null || library.definitions.length > 0" :definition="selectedDefinition" :all-definitions="library.definitions"
          @select-definition="selectDefinition" @save="handleSaveDefinition" @delete="handleDeleteDefinition"
          @update:content="() => {}" @update:name="() => {}" @update:type="() => {}" @duplicate="() => {}" @import="() => {}" @export="() => {}" />
        <button v-else class="w-full py-8 border-2 border-dashed border-line rounded-xl text-muted text-sm hover:text-primary hover:border-primary/30 transition-colors" @click="selectedDefinitionId = ''">+ Select or create a definition</button>
      </section>
      <p v-if="error" class="text-coral text-sm mt-4">{{ error }}</p>
    </template>
  </div>
</template>
