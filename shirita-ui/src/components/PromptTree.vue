<script setup lang="ts">
import { ref, computed } from 'vue'
import { Plus, FileText, Folder } from 'lucide-vue-next'
import type { Definition, DefType, PromptNode } from '../api/types'
import NodeRow from './NodeRow.vue'
import NodePicker from './NodePicker.vue'

const props = defineProps<{ nodes: PromptNode[]; definitions: Definition[]; types: DefType[] }>()
const emit = defineEmits<{
  toggleEnabled: [nodeId: string]
  addPrompt: [definitionId: string]
  addRefToContainer: [parentId: string, definitionId: string]
  addContainer: [typeId: string]
  createNewInContainer: [parentId: string, typeId: string]
  createNewPrompt: []
  updateContent: [definitionId: string, content: string]
  deleteNode: [nodeId: string]
  reorder: [orderedIds: string[]]
}>()

const expanded = ref<Set<string>>(new Set())
const activePickerParent = ref<string | undefined>(undefined)
const rootMenu = ref<'closed' | 'menu' | 'addPrompt' | 'addContainer'>('closed')

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
function containerDefs(tag: string | null) { return props.definitions.filter((d) => d.type === tag) }

function isExpanded(id: string) { return expanded.value.has(id) }
function toggleExpand(id: string) {
  if (expanded.value.has(id)) expanded.value.delete(id)
  else expanded.value.add(id)
}
function openPicker(parentId: string) {
  activePickerParent.value = activePickerParent.value === parentId ? undefined : parentId
}
function toggleRootMenu() { rootMenu.value = rootMenu.value === 'closed' ? 'menu' : 'closed' }
</script>

<template>
  <div data-test="prompt-tree" class="border border-line rounded-xl bg-white p-1.5">
    <template v-for="node in rootNodes" :key="node.id">
      <NodeRow
        :node="node"
        :definitions="defMap"
        :depth="0"
        :is-expanded="isExpanded(node.id)"
        @toggle-enabled="emit('toggleEnabled', node.id)"
        @toggle-expand="toggleExpand(node.id)"
        @update-content="(c) => node.definition_id && emit('updateContent', node.definition_id, c)"
        @delete="emit('deleteNode', node.id)"
      />

      <!-- folder children + contextual add -->
      <template v-if="node.kind === 'folder' && isExpanded(node.id)">
        <NodeRow
          v-for="child in getChildren(node.id)"
          :key="child.id"
          :node="child"
          :definitions="defMap"
          :depth="1"
          :is-expanded="isExpanded(child.id)"
          @toggle-enabled="emit('toggleEnabled', child.id)"
          @toggle-expand="toggleExpand(child.id)"
          @update-content="(c) => child.definition_id && emit('updateContent', child.definition_id, c)"
          @delete="emit('deleteNode', child.id)"
        />
        <button
          class="flex items-center gap-2 py-1.5 pl-[34px] text-[13px] text-muted hover:text-primary"
          @click="openPicker(node.id)"
        >
          <Plus :size="15" /> Add to {{ node.tag }}
        </button>
        <div v-if="activePickerParent === node.id" class="pl-[34px] pr-2 pb-2">
          <NodePicker
            :definitions="containerDefs(node.tag)"
            :filter-type="node.tag"
            :types="types"
            @select="(id) => { emit('addRefToContainer', node.id, id); activePickerParent = undefined }"
            @create-new="() => { emit('createNewInContainer', node.id, node.tag as string); activePickerParent = undefined }"
          />
        </div>
      </template>
    </template>

    <!-- root add -->
    <button data-test="root-add" class="flex items-center gap-2 py-1.5 pl-2 mt-0.5 text-[13.5px] text-muted hover:text-primary" @click="toggleRootMenu">
      <Plus :size="16" /> Add node
    </button>

    <div v-if="rootMenu === 'menu'" class="px-2 pb-1 flex gap-1.5">
      <button
        data-test="add-prompt"
        class="flex items-center gap-1.5 px-2.5 py-1 text-[12.5px] rounded-lg border border-line text-muted hover:text-primary hover:border-primary/30"
        @click="rootMenu = 'addPrompt'"
      ><FileText :size="14" /> Add prompt</button>
      <button
        data-test="add-container"
        class="flex items-center gap-1.5 px-2.5 py-1 text-[12.5px] rounded-lg border border-line text-muted hover:text-primary hover:border-primary/30"
        @click="rootMenu = 'addContainer'"
      ><Folder :size="14" /> Add container</button>
    </div>

    <div v-if="rootMenu === 'addPrompt'" class="px-2 pb-1">
      <NodePicker
        :definitions="promptDefs"
        :filter-type="'prompt'"
        :types="types"
        @select="(id) => { emit('addPrompt', id); rootMenu = 'closed' }"
        @create-new="() => { emit('createNewPrompt'); rootMenu = 'closed' }"
      />
    </div>

    <div v-if="rootMenu === 'addContainer'" class="px-2 pb-2 flex flex-wrap gap-1.5">
      <button
        v-for="t in availableTypes"
        :key="t.id"
        data-test="container-type-option"
        class="px-2.5 py-1 text-[12px] rounded-full border border-line text-muted hover:text-primary hover:border-primary/30"
        @click="emit('addContainer', t.id); rootMenu = 'closed'"
      >{{ t.label }}</button>
      <span v-if="availableTypes.length === 0" class="text-[12px] text-muted/70">All container types added</span>
    </div>
  </div>
</template>
