<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { useLibraryStore } from '../stores/library'
import { createSession, createNode, updateNode, deleteNode, reorderNodes, listNodes, updateDefinition, createDefinition } from '../api/client'
import PromptTree from '../components/PromptTree.vue'
import type { PromptNode, Trigger } from '../api/types'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const library = useLibraryStore()

const sessionName = (route.query.name as string) || t('prompt.untitled')
const sessionAvatar = (route.query.avatar as string) || null
const selectedTemplateId = ref<string | null>(null)
const nodes = ref<PromptNode[]>([])
const creating = ref(false)
const error = ref<string | null>(null)

onMounted(async () => { await library.loadAll() })

async function selectTemplate(templateId: string) {
  selectedTemplateId.value = templateId
  try { nodes.value = await listNodes('template', templateId) } catch { nodes.value = [] }
}

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
  const node = nodes.value.find(n => n.id === nodeId)
  if (!node) return
  try {
    const updated = await updateNode(nodeId, { enabled: !node.enabled })
    const idx = nodes.value.findIndex(n => n.id === nodeId)
    if (idx !== -1) nodes.value = [...nodes.value.slice(0, idx), updated, ...nodes.value.slice(idx + 1)]
  } catch (e) { error.value = (e as Error).message }
}

async function handleUpdateContent(definitionId: string, content: string) {
  try {
    await updateDefinition(definitionId, { content })
    await library.loadDefinitions()
  } catch (e) { error.value = (e as Error).message }
}

async function handleDeleteNode(nodeId: string) {
  const node = nodes.value.find(n => n.id === nodeId)
  if (!node) return
  const childCount = nodes.value.filter(n => n.parent_id === nodeId).length
  if (node.kind === 'folder' && childCount > 0
      && !confirm(t('prompt.deleteContainerConfirm', childCount))) return
  try { await deleteNode(nodeId); await reload() } catch (e) { error.value = (e as Error).message }
}

async function handleReorder(orderedIds: string[]) {
  if (!selectedTemplateId.value) return
  try { await reorderNodes('template', selectedTemplateId.value, orderedIds); await reload() } catch (e) { error.value = (e as Error).message }
}

async function handleUpdateTrigger(definitionId: string, trigger: Trigger) {
  const def = library.definitions.find(d => d.id === definitionId)
  if (!def) return
  try { await updateDefinition(definitionId, { meta: { ...def.meta, trigger } }); await library.loadDefinitions() } catch (e) { error.value = (e as Error).message }
}

async function createChat() {
  creating.value = true; error.value = null
  try {
    const session = await createSession(sessionName, selectedTemplateId.value, sessionAvatar)
    router.push(`/chat/${session.id}`)
  } catch (e) { error.value = (e as Error).message }
  finally { creating.value = false }
}
</script>

<template>
  <div class="pt-6 pb-12">
    <h2 class="text-lg font-semibold mb-1">{{ sessionName }}</h2>
    <p class="text-[13px] text-muted mb-6">{{ $t('prompt.subtitle') }}</p>
    <div class="mb-4">
      <label class="text-[13px] text-muted mb-1.5 block">{{ $t('prompt.template') }}</label>
      <select :value="selectedTemplateId" class="field w-full" @change="selectTemplate(($event.target as HTMLSelectElement).value)">
        <option :value="null">{{ $t('prompt.none') }}</option>
        <option v-for="t in library.templates" :key="t.id" :value="t.id">{{ t.name }}</option>
      </select>
    </div>
    <PromptTree v-if="selectedTemplateId" :nodes="nodes" :definitions="library.definitions" :types="library.containerTypes"
      @toggle-enabled="handleToggleEnabled"
      @add-prompt="addPrompt" @add-container="addContainer" @add-ref-to-container="addRefToContainer"
      @create-new-prompt="createNewPrompt" @create-new-in-container="createNewInContainer" @create-type="createType"
      @update-content="handleUpdateContent" @update-trigger="handleUpdateTrigger" @delete-node="handleDeleteNode" @reorder="handleReorder" />
    <p v-if="error" class="text-coral text-sm mt-3">{{ error }}</p>
    <div class="mt-8">
      <button :disabled="creating" class="w-full py-2.5 rounded-full font-medium bg-primary text-white hover:bg-primary-strong transition-colors disabled:opacity-50" @click="createChat">{{ creating ? $t('prompt.creating') : $t('prompt.create') }}</button>
    </div>
  </div>
</template>
