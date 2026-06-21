<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useLibraryStore } from '../stores/library'
import {
  updatePack, listNodes, createNode, updateNode, deleteNode, reorderNodes,
  createDefinition, updateDefinition,
} from '../api/client'
import { selectOneSiblingsToDisable } from '../utils/tree'
import type { Pack, PromptNode, VarDecl, Trigger, Panel, PanelCaps } from '../api/types'
import PromptTree from './PromptTree.vue'
import VariablesEditor from './VariablesEditor.vue'
import AssetPicker from './AssetPicker.vue'
import PanelView from './PanelView.vue'

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

// ── panel: local editable copy seeded from meta.panel, persisted via save() ──
const panelHtml = ref('')
const panelCss = ref('')
const panelCaps = ref<PanelCaps>({})
watch(
  () => props.pack.id,
  () => {
    const p = (props.pack.meta as { panel?: Panel }).panel
    panelHtml.value = p?.html ?? ''
    panelCss.value = p?.css ?? ''
    panelCaps.value = { ...(p?.caps ?? {}) }
  },
  { immediate: true },
)
function savePanel() {
  void save({
    meta: {
      ...(props.pack.meta as Record<string, unknown>),
      panel: { html: panelHtml.value, css: panelCss.value, caps: panelCaps.value },
    },
  })
}
function toggleCap(cap: 'write' | 'insert' | 'send') {
  panelCaps.value = { ...panelCaps.value, [cap]: !panelCaps.value[cap] }
  savePanel()
}
// Preview binds the pack's declared variables at their initial values.
const previewValues = computed<Record<string, unknown>>(
  () => Object.fromEntries(packVars.value.map((v) => [v.name, v.initial])),
)

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
async function createNewInContainer(parentId: string, typeId: string) {
  try {
    const def = await createDefinition({ type: typeId, name: `New ${typeId}`, content: '', meta: {} })
    await library.loadDefinitions()
    await createNode('pack', props.pack.id, { parent_id: parentId, kind: 'ref', definition_id: def.id })
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
    />

    <!-- variables -->
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mt-4 mb-2">{{ $t('pack.variables') }}</h3>
    <VariablesEditor :model-value="packVars" @update:model-value="saveVars" />

    <!-- panel -->
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mt-4 mb-2">{{ $t('pack.panel') }}</h3>
    <div data-test="pack-panel" class="space-y-2">
      <label class="block">
        <span class="text-[12px] text-muted block mb-1">{{ $t('pack.panelHtml') }}</span>
        <textarea
          data-test="panel-html"
          v-model="panelHtml"
          rows="6"
          class="field w-full font-mono text-[12px]"
          :placeholder="$t('pack.panelHtmlPlaceholder')"
          @change="savePanel"
        />
      </label>
      <label class="block">
        <span class="text-[12px] text-muted block mb-1">{{ $t('pack.panelCss') }}</span>
        <textarea
          data-test="panel-css"
          v-model="panelCss"
          rows="5"
          class="field w-full font-mono text-[12px]"
          :placeholder="$t('pack.panelCssPlaceholder')"
          @change="savePanel"
        />
      </label>
      <div class="flex items-center flex-wrap gap-x-4 gap-y-1.5 text-[12px]">
        <span class="text-muted">{{ $t('pack.panelCaps') }}</span>
        <label class="flex items-center gap-1.5"><input type="checkbox" data-test="cap-write" :checked="panelCaps.write" @change="toggleCap('write')" />{{ $t('pack.capWrite') }}</label>
        <label class="flex items-center gap-1.5"><input type="checkbox" data-test="cap-insert" :checked="panelCaps.insert" @change="toggleCap('insert')" />{{ $t('pack.capInsert') }}</label>
        <label class="flex items-center gap-1.5"><input type="checkbox" data-test="cap-send" :checked="panelCaps.send" @change="toggleCap('send')" />{{ $t('pack.capSend') }}</label>
      </div>
      <div>
        <span class="text-[12px] text-muted block mb-1">{{ $t('pack.panelPreview') }}</span>
        <PanelView :html="panelHtml" :css="panelCss" :values="previewValues" />
      </div>
    </div>

    <p v-if="error" class="text-coral text-sm mt-3">{{ error }}</p>
  </div>
</template>
