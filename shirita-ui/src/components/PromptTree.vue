<script setup lang="ts">
import { ref, computed } from 'vue'
import { Plus, Folder, FileText } from 'lucide-vue-next'
import type { Definition, DefType, PromptNode, Trigger } from '../api/types'
import NodeRow from './NodeRow.vue'
import NodePicker from './NodePicker.vue'
import EntityPicker from './EntityPicker.vue'

const props = defineProps<{ nodes: PromptNode[]; definitions: Definition[]; types: DefType[]; allowPanel?: boolean }>()
const emit = defineEmits<{
  toggleEnabled: [nodeId: string]
  addPrompt: [definitionId: string]
  addRefToContainer: [parentId: string, definitionId: string]
  addContainer: [typeId: string]
  createNewInContainer: [parentId: string | null, typeId: string]
  createNewPrompt: [name: string]
  createType: [name: string]
  updateContent: [definitionId: string, content: string]
  updateTrigger: [definitionId: string, trigger: Trigger]
  updateNodeMeta: [nodeId: string, meta: Record<string, unknown>]
  updateDefMeta: [definitionId: string, meta: Record<string, unknown>]
  updateDefName: [definitionId: string, name: string]
  deleteNode: [nodeId: string]
  reorder: [orderedIds: string[]]
  addPanel: []
  openDefinition: [definitionId: string]
}>()

const expanded = ref<Set<string>>(new Set())
const activePickerParent = ref<string | undefined>(undefined)

const defMap = computed<Record<string, Definition>>(() => {
  const m: Record<string, Definition> = {}
  for (const d of props.definitions) m[d.id] = d
  return m
})

function getChildren(parentId: string | null): PromptNode[] {
  return props.nodes.filter((n) => n.parent_id === parentId).sort((a, b) => a.sort_order - b.sort_order)
}

const rootNodes = computed(() => getChildren(null))

// ── container def filtering ──
function containerDefs(tag: string): Definition[] {
  return props.definitions.filter((d) => d.type === tag)
}

const panelBrickTypes = ['html', 'css']
const panelPickerTypes = computed<DefType[]>(() =>
  panelBrickTypes.map((id) => ({ id, label: id.toUpperCase(), sort: 0, builtin: true, created_at: '' })),
)

// ── available container types (exclude ones already used) ──
const availableTypes = computed<DefType[]>(() => {
  const used = new Set(rootNodes.value.filter((n) => n.kind === 'folder').map((n) => n.tag))
  return props.types.filter((t) => !used.has(t.id))
})

const promptDefs = computed(() =>
  props.definitions.filter((d) => ['char', 'persona', 'world', 'prompt', 'first_message'].includes(d.type)),
)

// ── root omni-search via EntityPicker ──
// Composite id: "container:<typeId>" | "prompt:<defId>" | "brick:<typeId>"
type OmniKind = 'container' | 'prompt' | 'brick'
const omniItems = computed(() => {
  const items: { id: string; name: string }[] = []
  for (const t of availableTypes.value) {
    items.push({ id: `container:${t.id}`, name: t.label })
  }
  items.push(
    { id: 'brick:variables', name: 'Variables' },
    { id: 'brick:regex_rule', name: 'Regex' },
  )
  for (const d of promptDefs.value) {
    items.push({ id: `prompt:${d.id}`, name: d.name })
  }
  return items
})

const rootOpen = ref(false)
const creating = ref(false)
const createName = ref('')

function openRoot() { rootOpen.value = !rootOpen.value }
function closeRootAndCreate() { rootOpen.value = false; creating.value = false; createName.value = '' }

function onOmniSelect(compositeId: string) {
  const [kind, id] = compositeId.split(':')
  if (kind === 'prompt') emit('addPrompt', id)
  else if (kind === 'brick') emit('createNewInContainer', null, id)
  else emit('addContainer', id)
  rootOpen.value = false
}

function onIntentCreate(draft: string) {
  createName.value = draft
  creating.value = true
}

function confirmCreate() {
  const name = createName.value.trim()
  if (!name) return
  emit('createNewPrompt', name)
  closeRootAndCreate()
}

function cancelCreate() { closeRootAndCreate() }

// ── folder expand / folder-add picker ──
function isExpanded(id: string) { return expanded.value.has(id) }
function toggleExpand(id: string) {
  if (expanded.value.has(id)) expanded.value.delete(id)
  else expanded.value.add(id)
}
function onFolderAdd(id: string) {
  expanded.value.add(id)
  activePickerParent.value = activePickerParent.value === id ? undefined : id
}

// ── drag-reorder within same parent ──
const dragId = ref<string | null>(null)
const grabbedHandle = ref(false)
const dropTarget = ref<{ id: string; position: 'before' | 'after' } | null>(null)
function onMouseDown(e: MouseEvent) {
  grabbedHandle.value = !!(e.target as HTMLElement).closest('[data-test="drag-handle"]')
}
function onDragStart(id: string, e: DragEvent) {
  if (!grabbedHandle.value) { e.preventDefault(); return }
  dragId.value = id
  e.dataTransfer?.setData('text/plain', id)
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move'
}
function siblingsOf(parentId: string | null) { return getChildren(parentId).map((nd) => nd.id) }
function parentOf(id: string): string | null {
  return props.nodes.find((nd) => nd.id === id)?.parent_id ?? null
}
function onDragOver(id: string, e: DragEvent) {
  const src = dragId.value
  if (!src || src === id || parentOf(src) !== parentOf(id)) {
    dropTarget.value = null; return
  }
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  dropTarget.value = { id, position: (e.clientY - rect.top) < rect.height / 2 ? 'before' : 'after' }
}
function onDrop(targetId: string) {
  const src = dragId.value
  const target = dropTarget.value
  dragId.value = null; grabbedHandle.value = false; dropTarget.value = null
  if (!src || src === targetId) return
  if (parentOf(src) !== parentOf(targetId)) return
  const without = siblingsOf(parentOf(targetId)).filter((x) => x !== src)
  const targetIdx = without.indexOf(targetId)
  if (targetIdx === -1) return
  const insertAt = target?.id === targetId
    ? (target.position === 'before' ? targetIdx : targetIdx + 1)
    : targetIdx + 1
  without.splice(insertAt, 0, src)
  emit('reorder', without)
}
function onDragEnd() { dragId.value = null; grabbedHandle.value = false; dropTarget.value = null }
</script>

<template>
  <div data-test="prompt-tree" class="border border-line rounded-xl bg-card p-1.5">
    <div
      v-for="node in rootNodes"
      :key="node.id"
      data-test="row-wrap"
      :class="['relative', dragId === node.id ? 'opacity-40' : '']"
      draggable="true"
      @mousedown="onMouseDown"
      @dragstart="onDragStart(node.id, $event)"
      @dragover="onDragOver(node.id, $event)"
      @drop="onDrop(node.id)"
      @dragend="onDragEnd"
    >
      <span
        v-if="dropTarget?.id === node.id"
        data-test="drop-indicator"
        class="pointer-events-none absolute left-2 right-2 h-[2px] bg-primary rounded-full z-10"
        :class="dropTarget.position === 'before' ? 'top-0' : 'bottom-0'"
      />
      <NodeRow
        :node="node"
        :definitions="defMap"
        :depth="0"
        :is-expanded="isExpanded(node.id)"
        @toggle-enabled="emit('toggleEnabled', node.id)"
        @toggle-expand="toggleExpand(node.id)"
        @add="onFolderAdd(node.id)"
        @update-content="(c) => node.definition_id && emit('updateContent', node.definition_id, c)"
        @update-trigger="(t) => node.definition_id && emit('updateTrigger', node.definition_id, t)"
        @update-node-meta="(m) => emit('updateNodeMeta', node.id, m)"
        @update-def-meta="(m) => node.definition_id && emit('updateDefMeta', node.definition_id, m)"
        @update-def-name="(n) => node.definition_id && emit('updateDefName', node.definition_id, n)"
        @delete="emit('deleteNode', node.id)"
        @open-definition="(id) => emit('openDefinition', id)"
      />

      <template v-if="node.kind === 'folder' && isExpanded(node.id)">
        <div
          v-for="child in getChildren(node.id)"
          :key="child.id"
          data-test="row-wrap"
          :class="['relative', dragId === child.id ? 'opacity-40' : '']"
          draggable="true"
          @mousedown="onMouseDown"
          @dragstart.stop="onDragStart(child.id, $event)"
          @dragover="onDragOver(child.id, $event)"
          @drop.stop="onDrop(child.id)"
          @dragend="onDragEnd"
        >
          <span
            v-if="dropTarget?.id === child.id"
            data-test="drop-indicator"
            class="pointer-events-none absolute left-2 right-2 h-[2px] bg-primary rounded-full z-10"
            :class="dropTarget.position === 'before' ? 'top-0' : 'bottom-0'"
          />
          <NodeRow
            :node="child"
            :definitions="defMap"
            :depth="1"
            :is-expanded="isExpanded(child.id)"
            :single-select="(node.meta as Record<string, unknown>).select === 'one'"
            @toggle-enabled="emit('toggleEnabled', child.id)"
            @toggle-expand="toggleExpand(child.id)"
            @update-content="(c) => child.definition_id && emit('updateContent', child.definition_id, c)"
            @update-trigger="(t) => child.definition_id && emit('updateTrigger', child.definition_id, t)"
            @update-node-meta="(m) => emit('updateNodeMeta', child.id, m)"
            @update-def-meta="(m) => child.definition_id && emit('updateDefMeta', child.definition_id, m)"
            @update-def-name="(n) => child.definition_id && emit('updateDefName', child.definition_id, n)"
            @delete="emit('deleteNode', child.id)"
            @open-definition="(id) => emit('openDefinition', id)"
          />
        </div>
        <transition name="expand">
          <div v-if="activePickerParent === node.id" class="pl-[34px] pr-2 pb-2 pt-1">
            <NodePicker
              :definitions="containerDefs(node.tag ?? '')"
              :filter-type="(node.tag ?? '') === 'panel' ? 'html' : (node.tag ?? '')"
              :types="node.tag === 'panel' ? panelPickerTypes : types"
              @select="(id) => { emit('addRefToContainer', node.id, id); activePickerParent = undefined }"
              @create-new="(typeId) => { emit('createNewInContainer', node.id, typeId); activePickerParent = undefined }"
            />
          </div>
        </transition>
      </template>
    </div>

    <!-- root add: omnibox via EntityPicker -->
    <button
      data-test="root-add"
      class="flex items-center gap-2 py-1.5 pl-2 mt-0.5 text-[13.5px] text-muted hover:text-primary transition-colors"
      @click="openRoot"
    >
      <Plus :size="16" /> {{ $t('prompt.addNode') }}
    </button>

    <!-- add-panel (pack only) -->
    <button
      v-if="allowPanel"
      data-test="add-panel"
      class="flex items-center gap-2 py-1.5 pl-2 text-[13.5px] text-muted hover:text-primary transition-colors"
      @click="emit('addPanel')"
    >
      <Plus :size="16" /> {{ $t('pack.addPanel') }}
    </button>

    <!-- root omni-search (expanded) -->
    <transition name="expand">
      <div v-if="rootOpen" data-test="root-omnibox" class="px-2 pb-2">
        <template v-if="!creating">
          <EntityPicker
            :items="omniItems"
            :placeholder="$t('prompt.omniPlaceholder')"
            :create-label="$t('prompt.omniNewPrompt', { name: '' })"
            @select="onOmniSelect"
            @intent-create="onIntentCreate"
          />
        </template>
        <div v-else class="flex items-center gap-2 mt-1">
          <input
            v-model="createName"
            type="text"
            data-test="omni-create-input"
            :placeholder="$t('prompt.omniNewPrompt', { name: '' })"
            class="flex-1 bg-transparent border border-line rounded-lg px-3 py-2 text-[13px] text-ink placeholder:text-muted/60 outline-none focus:border-primary/50"
            @keydown.enter="confirmCreate"
            @keydown.escape="cancelCreate"
          />
          <button class="text-muted hover:text-ink" @click="cancelCreate">&#x2715;</button>
          <button
            class="text-emerald-400 hover:text-emerald-300"
            :class="createName.trim() ? '' : 'opacity-30 pointer-events-none'"
            @click="confirmCreate"
          >&#x2713;</button>
        </div>
      </div>
    </transition>
  </div>
</template>
