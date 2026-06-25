<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useLibraryStore } from '../stores/library'
import {
  updatePack, listNodes, createNode, updateNode, deleteNode, reorderNodes,
  createDefinition, updateDefinition,
} from '../api/client'
import { selectOneSiblingsToDisable } from '../utils/tree'
import type { Pack, PromptNode, VarDecl, Trigger } from '../api/types'
import PromptTree from './PromptTree.vue'
import VariablesEditor from './VariablesEditor.vue'
import AssetPicker from './AssetPicker.vue'

const props = defineProps<{ pack: Pack }>()
const emit = defineEmits<{ changed: [] }>()

const library = useLibraryStore()
const nodes = ref<PromptNode[]>([])
const error = ref<string | null>(null)

async function reload() {
  try { nodes.value = await listNodes('pack', props.pack.id) } catch { nodes.value = [] }
}
watch(() => props.pack.id, reload, { immediate: true })

// ── identity + variables: persist via updatePack (name is required) ──
async function save(patch: { identity?: Pack['identity']; meta?: Record<string, unknown> }) {
  try {
    await updatePack(props.pack.id, {
      name: props.pack.name,
      identity: patch.identity ?? props.pack.identity,
      meta: patch.meta ?? (props.pack.meta as Record<string, unknown>),
    })
    emit('changed')
  } catch (e) { error.value = (e as Error).message }
}
function updateDisplayName(name: string) {
  void save({ identity: { ...props.pack.identity, display_name: name.trim() || null } })
}
function updateAvatar(avatar: string) {
  void save({ identity: { ...props.pack.identity, avatar: avatar || null } })
}
const packVars = computed<VarDecl[]>(
  () => ((props.pack.meta as Record<string, unknown>).variables as VarDecl[]) ?? [],
)
function saveVars(vars: VarDecl[]) {
  void save({ meta: { ...(props.pack.meta as Record<string, unknown>), variables: vars } })
}

// ── content tree (owner_kind = 'pack'), mirrors the template-tree wiring ──
function slugifyType(name: string) {
  const slug = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '')
  return slug || `type-${Date.now().toString(36)}`
}
async function addPrompt(definitionId: string) {
  try { await createNode('pack', props.pack.id, { parent_id: null, kind: 'ref', definition_id: definitionId }); await reload() } catch (e) { error.value = (e as Error).message }
}
async function addContainer(typeId: string) {
  try { await createNode('pack', props.pack.id, { parent_id: null, kind: 'folder', tag: typeId }); await reload() } catch (e) { error.value = (e as Error).message }
}
async function addRefToContainer(parentId: string, definitionId: string) {
  try { await createNode('pack', props.pack.id, { parent_id: parentId, kind: 'ref', definition_id: definitionId }); await reload() } catch (e) { error.value = (e as Error).message }
}
async function createNewPrompt(name: string) {
  try {
    const def = await createDefinition({ type: 'prompt', name: name?.trim() || 'New prompt', content: '', meta: {} })
    await library.loadDefinitions()
    await createNode('pack', props.pack.id, { parent_id: null, kind: 'ref', definition_id: def.id })
    await reload()
  } catch (e) { error.value = (e as Error).message }
}
async function createNewInContainer(parentId: string | null, typeId: string) {
  try {
    const def = await createDefinition({ type: typeId, name: `New ${typeId}`, content: '', meta: {} })
    await library.loadDefinitions()
    await createNode('pack', props.pack.id, { parent_id: parentId, kind: 'ref', definition_id: def.id })
    await reload()
  } catch (e) { error.value = (e as Error).message }
}
async function addPanel() {
  try {
    const html = await createDefinition({ type: 'html', name: 'Panel HTML', content: '', meta: {} })
    const css = await createDefinition({ type: 'css', name: 'Panel CSS', content: '', meta: {} })
    await library.loadDefinitions()
    // createNode's body has no `meta` field (folder name/caps land in meta), so
    // the folder is created bare and then immediately given its meta via updateNode.
    const folder = await createNode('pack', props.pack.id, { parent_id: null, kind: 'folder', tag: 'panel' })
    await updateNode(folder.id, { meta: { name: 'Panel', caps: {} } })
    await createNode('pack', props.pack.id, { parent_id: folder.id, kind: 'ref', definition_id: html.id })
    await createNode('pack', props.pack.id, { parent_id: folder.id, kind: 'ref', definition_id: css.id })
    await reload()
  } catch (e) { error.value = (e as Error).message }
}
async function createType(name: string) {
  if (!name.trim()) return
  try { const created = await library.addType(slugifyType(name), name.trim()); await addContainer(created.id) } catch (e) { error.value = (e as Error).message }
}
async function toggleEnabled(nodeId: string) {
  const node = nodes.value.find((n) => n.id === nodeId)
  if (!node) return
  const enabling = !node.enabled
  try {
    await updateNode(nodeId, { enabled: enabling })
    if (enabling) {
      for (const sib of selectOneSiblingsToDisable(nodes.value, nodeId)) {
        await updateNode(sib, { enabled: false })
      }
    }
    await reload()
  } catch (e) { error.value = (e as Error).message }
}
async function updateNodeMeta(nodeId: string, meta: Record<string, unknown>) {
  try { await updateNode(nodeId, { meta }); await reload() } catch (e) { error.value = (e as Error).message }
}
async function handleDelete(nodeId: string) {
  try { await deleteNode(nodeId); await reload() } catch (e) { error.value = (e as Error).message }
}
async function reorder(orderedIds: string[]) {
  try { await reorderNodes('pack', props.pack.id, orderedIds); await reload() } catch (e) { error.value = (e as Error).message }
}
async function updateContent(definitionId: string, content: string) {
  try { await updateDefinition(definitionId, { content }); await library.loadDefinitions() } catch (e) { error.value = (e as Error).message }
}
async function updateTrigger(definitionId: string, trigger: Trigger) {
  const def = library.definitions.find((d) => d.id === definitionId)
  if (!def) return
  try { await updateDefinition(definitionId, { meta: { ...def.meta, trigger } }); await library.loadDefinitions() } catch (e) { error.value = (e as Error).message }
}
</script>

<template>
  <div data-test="pack-editor">
    <!-- identity -->
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2.5">{{ $t('pack.identity') }}</h3>
    <div class="flex items-start gap-4 mb-4">
      <AssetPicker
        shape="circle"
        kind="avatar"
        :model-value="pack.identity.avatar || ''"
        @update:model-value="updateAvatar"
      />
      <label class="flex-1">
        <span class="text-[12px] text-muted block mb-1.5">{{ $t('pack.displayName') }}</span>
        <input
          data-test="pack-display-name"
          :value="pack.identity.display_name || ''"
          type="text"
          class="field w-full"
          :placeholder="$t('pack.displayNamePlaceholder')"
          @change="updateDisplayName(($event.target as HTMLInputElement).value)"
        />
      </label>
    </div>

    <!-- content tree -->
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2">{{ $t('pack.contentTree') }}</h3>
    <PromptTree
      :nodes="nodes"
      :definitions="library.definitions"
      :types="library.containerTypes"
      :allow-panel="true"
      @toggle-enabled="toggleEnabled"
      @add-prompt="addPrompt"
      @add-container="addContainer"
      @add-ref-to-container="addRefToContainer"
      @create-new-prompt="createNewPrompt"
      @create-new-in-container="createNewInContainer"
      @create-type="createType"
      @update-content="updateContent"
      @update-trigger="updateTrigger"
      @update-node-meta="updateNodeMeta"
      @delete-node="handleDelete"
      @reorder="reorder"
      @add-panel="addPanel"
    />

    <!-- variables -->
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mt-4 mb-2">{{ $t('pack.variables') }}</h3>
    <VariablesEditor :model-value="packVars" @update:model-value="saveVars" />

    <p v-if="error" class="text-coral text-sm mt-3">{{ error }}</p>
  </div>
</template>
