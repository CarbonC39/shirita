<script setup lang="ts">
import { ref, computed } from 'vue'
import { Plus, FileText, Folder, Search } from 'lucide-vue-next'
import type { Definition, DefType, PromptNode, Trigger } from '../api/types'
import NodeRow from './NodeRow.vue'
import NodePicker from './NodePicker.vue'

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
}>()

const expanded = ref<Set<string>>(new Set())
const activePickerParent = ref<string | undefined>(undefined)

// Root add: a single search-first omnibox over prompts + container types.
const rootOpen = ref(false)
const omniQuery = ref('')

const defMap = computed<Record<string, Definition>>(() => {
  const m: Record<string, Definition> = {}
  for (const d of props.definitions) m[d.id] = d
  return m
})

function getChildren(parentId: string | null): PromptNode[] {
  return props.nodes.filter((n) => n.parent_id === parentId).sort((a, b) => a.sort_order - b.sort_order)
}

const rootNodes = computed(() => getChildren(null))

// placement rules: one container per type; prompt-refs at root; typed refs inside containers.
const existingContainerTags = computed(() =>
  new Set(props.nodes.filter((nd) => nd.kind === 'folder' && nd.parent_id === null).map((nd) => nd.tag)))
const availableTypes = computed(() => props.types.filter((t) => !existingContainerTags.value.has(t.id)))
const promptDefs = computed(() => props.definitions.filter((d) => d.type === 'prompt'))
// A panel folder's children are html/css bricks (two types, not one container
// type), so it needs its own definitions/creatable-type list instead of the
// single-tag filter every other container tag uses.
const panelBrickTypes = ['html', 'css', 'variables']
function containerDefs(tag: string | null) {
  if (tag === 'panel') return props.definitions.filter((d) => panelBrickTypes.includes(d.type))
  return props.definitions.filter((d) => d.type === tag)
}
// Lets NodePicker's "other type" selector switch between html/css when adding
// to a panel folder, instead of the regular container-type list.
const panelPickerTypes = computed<DefType[]>(() =>
  panelBrickTypes.map((id) => ({ id, label: id.toUpperCase(), sort: 0, builtin: true, created_at: '' })),
)

// Combined, ranked omnibox list: container types first, then bricks, then prompt definitions.
type OmniItem = { kind: 'container' | 'prompt' | 'brick'; id: string; name: string }
const omniItems = computed<OmniItem[]>(() => {
  const containers: OmniItem[] = availableTypes.value.map((t) => ({ kind: 'container', id: t.id, name: t.label }))
  const bricks: OmniItem[] = [
    { kind: 'brick', id: 'variables', name: 'Variables' },
    { kind: 'brick', id: 'regex_rule', name: 'Regex' },
  ]
  const prompts: OmniItem[] = promptDefs.value.map((d) => ({ kind: 'prompt', id: d.id, name: d.name }))
  let items = [...containers, ...bricks, ...prompts]
  const q = omniQuery.value.trim().toLowerCase()
  if (q) {
    items = items
      .filter((i) => i.name.toLowerCase().includes(q))
      .sort((a, b) => Number(b.name.toLowerCase().startsWith(q)) - Number(a.name.toLowerCase().startsWith(q)))
  }
  return items
})
const trimmedQuery = computed(() => omniQuery.value.trim())

function openRoot() {
  rootOpen.value = !rootOpen.value
  if (!rootOpen.value) omniQuery.value = ''
}
function closeRoot() { rootOpen.value = false; omniQuery.value = '' }
function pickOmni(item: OmniItem) {
  if (item.kind === 'prompt') emit('addPrompt', item.id)
  else if (item.kind === 'brick') emit('createNewInContainer', null, item.id)
  else emit('addContainer', item.id)
  closeRoot()
}
function newPrompt() { emit('createNewPrompt', trimmedQuery.value); closeRoot() }
function newType() { emit('createType', trimmedQuery.value); closeRoot() }

function isExpanded(id: string) { return expanded.value.has(id) }
function toggleExpand(id: string) {
  if (expanded.value.has(id)) expanded.value.delete(id)
  else expanded.value.add(id)
}
function onFolderAdd(id: string) {
  expanded.value.add(id)
  activePickerParent.value = activePickerParent.value === id ? undefined : id
}

// native HTML5 drag-reorder, restricted to siblings of the same parent. A drag
// only counts if it began on a row's grip handle, so clicks/selection on the
// rest of the row don't accidentally start a drag.
const dragId = ref<string | null>(null)
const grabbedHandle = ref(false)
function onMouseDown(e: MouseEvent) {
  grabbedHandle.value = !!(e.target as HTMLElement).closest('[data-test="drag-handle"]')
}
function onDragStart(id: string, e: DragEvent) {
  if (!grabbedHandle.value) { e.preventDefault(); return }
  dragId.value = id
  // Required for native HTML5 DnD to actually continue past the source
  // element: without calling setData, Firefox (and some Chromium paths)
  // never deliver dragover/drop to other elements, so the row looks
  // draggable but nothing ever drops.
  e.dataTransfer?.setData('text/plain', id)
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move'
}
function siblingsOf(parentId: string | null) { return getChildren(parentId).map((nd) => nd.id) }
function parentOf(id: string): string | null {
  return props.nodes.find((nd) => nd.id === id)?.parent_id ?? null
}
function onDrop(targetId: string) {
  const src = dragId.value
  dragId.value = null
  grabbedHandle.value = false
  if (!src || src === targetId) return
  if (parentOf(src) !== parentOf(targetId)) return // only reorder within a level
  const ids = siblingsOf(parentOf(targetId))
  const from = ids.indexOf(src)
  const to = ids.indexOf(targetId)
  if (from === -1 || to === -1) return
  ids.splice(to, 0, ids.splice(from, 1)[0])
  emit('reorder', ids)
}
</script>

<template>
  <div data-test="prompt-tree" class="border border-line rounded-xl bg-card p-1.5">
    <div
      v-for="node in rootNodes"
      :key="node.id"
      data-test="row-wrap"
      draggable="true"
      @mousedown="onMouseDown"
      @dragstart="onDragStart(node.id, $event)"
      @dragover.prevent
      @drop="onDrop(node.id)"
    >
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
      />

      <!-- folder children + contextual picker (opened from the row's + button) -->
      <template v-if="node.kind === 'folder' && isExpanded(node.id)">
        <div
          v-for="child in getChildren(node.id)"
          :key="child.id"
          data-test="row-wrap"
          draggable="true"
          @mousedown="onMouseDown"
          @dragstart.stop="onDragStart(child.id, $event)"
          @dragover.prevent
          @drop.stop="onDrop(child.id)"
        >
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
          />
        </div>
        <transition name="expand">
          <div v-if="activePickerParent === node.id" class="pl-[34px] pr-2 pb-2 pt-1">
            <NodePicker
              :definitions="containerDefs(node.tag)"
              :filter-type="node.tag === 'panel' ? 'html' : node.tag"
              :types="node.tag === 'panel' ? panelPickerTypes : types"
              @select="(id) => { emit('addRefToContainer', node.id, id); activePickerParent = undefined }"
              @create-new="(typeId) => { emit('createNewInContainer', node.id, typeId); activePickerParent = undefined }"
            />
          </div>
        </transition>
      </template>
    </div>

    <!-- root add: one omnibox for prompts + containers -->
    <button data-test="root-add" class="flex items-center gap-2 py-1.5 pl-2 mt-0.5 text-[13.5px] text-muted hover:text-primary transition-colors" @click="openRoot">
      <Plus :size="16" /> {{ $t('prompt.addNode') }}
    </button>

    <!-- add-panel: scaffolds a panel folder + blank html/css bricks in one click -->
    <button
      v-if="allowPanel"
      data-test="add-panel"
      class="flex items-center gap-2 py-1.5 pl-2 text-[13.5px] text-muted hover:text-primary transition-colors"
      @click="emit('addPanel')"
    >
      <Plus :size="16" /> {{ $t('pack.addPanel') }}
    </button>

    <transition name="expand">
      <div v-if="rootOpen" data-test="root-omnibox" class="px-2 pb-2">
        <div class="border border-line rounded-[10px] bg-surface/60 overflow-hidden">
          <div class="flex items-center gap-2 px-3 py-2 border-b border-line">
            <Search :size="15" class="text-muted shrink-0" />
            <input
              v-model="omniQuery"
              data-test="omni-input"
              type="text"
              :placeholder="$t('prompt.omniPlaceholder')"
              class="flex-1 text-[13px] bg-transparent outline-none placeholder:text-muted/60"
            />
          </div>
          <button
            v-for="item in omniItems"
            :key="item.kind + ':' + item.id"
            :data-test="item.kind === 'brick' ? 'create-' + item.id : 'omni-item'"
            class="w-full flex items-center gap-2.5 px-3 py-2 text-left hover:bg-card transition-colors"
            @click="pickOmni(item)"
          >
            <Folder v-if="item.kind === 'container'" :size="15" class="text-mauve shrink-0" :stroke-width="1.8" />
            <FileText v-else :size="15" class="text-muted shrink-0" :stroke-width="1.8" />
            <span class="flex-1 text-[13.5px] text-ink truncate">{{ item.name }}</span>
            <span class="text-[11px] text-muted/70 lowercase">{{ item.kind }}</span>
          </button>
          <p v-if="omniItems.length === 0 && !trimmedQuery" class="px-3 py-2 text-[12px] text-muted/70">{{ $t('prompt.omniEmpty') }}</p>

          <template v-if="trimmedQuery">
            <button data-test="omni-new-prompt" class="w-full flex items-center gap-2.5 px-3 py-2 text-left border-t border-line hover:bg-card transition-colors text-ink" @click="newPrompt">
              <Plus :size="15" class="shrink-0" /><span class="text-[13.5px]">{{ $t('prompt.omniNewPrompt', { name: trimmedQuery }) }}</span>
            </button>
            <button data-test="omni-new-type" class="w-full flex items-center gap-2.5 px-3 py-2 text-left border-t border-line hover:bg-card transition-colors text-ink" @click="newType">
              <Plus :size="15" class="shrink-0" /><span class="text-[13.5px]">{{ $t('prompt.omniNewType', { name: trimmedQuery }) }}</span>
            </button>
          </template>
        </div>
      </div>
    </transition>
  </div>
</template>
