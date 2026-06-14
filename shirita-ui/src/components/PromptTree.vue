<script setup lang="ts">
import { ref, computed } from 'vue'
import { Plus } from 'lucide-vue-next'
import type { Definition, PromptNode } from '../api/types'
import NodeRow from './NodeRow.vue'
import NodePicker from './NodePicker.vue'

const props = defineProps<{ nodes: PromptNode[]; definitions: Definition[] }>()
const emit = defineEmits<{
  toggleEnabled: [nodeId: string]
  addNode: [parentId: string | null, definitionId: string]
  updateContent: [definitionId: string, content: string]
  createNew: [parentId: string | null, type: string]
}>()

const expanded = ref<Set<string>>(new Set())
const activePickerParent = ref<string | null | undefined>(undefined)

const defMap = computed<Record<string, Definition>>(() => {
  const m: Record<string, Definition> = {}
  for (const d of props.definitions) m[d.id] = d
  return m
})

function getChildren(parentId: string | null): PromptNode[] {
  return props.nodes.filter((n) => n.parent_id === parentId).sort((a, b) => a.sort_order - b.sort_order)
}

const rootNodes = computed(() => getChildren(null))

function isExpanded(id: string) { return expanded.value.has(id) }
function toggleExpand(id: string) {
  if (expanded.value.has(id)) expanded.value.delete(id)
  else expanded.value.add(id)
}
function openPicker(parentId: string | null) {
  activePickerParent.value = activePickerParent.value === parentId ? undefined : parentId
}
function handleSelect(definitionId: string) {
  emit('addNode', (activePickerParent.value ?? null) as string | null, definitionId)
  activePickerParent.value = undefined
}
function handleCreateNew(type: string) {
  emit('createNew', (activePickerParent.value ?? null) as string | null, type)
  activePickerParent.value = undefined
}
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
        />
        <button
          class="flex items-center gap-2 py-1.5 pl-[34px] text-[13px] text-muted hover:text-primary"
          @click="openPicker(node.id)"
        >
          <Plus :size="15" /> Add to {{ node.tag }}
        </button>
        <div v-if="activePickerParent === node.id" class="pl-[34px] pr-2 pb-2">
          <NodePicker :definitions="definitions" :filter-type="node.tag" @select="handleSelect" @create-new="handleCreateNew" />
        </div>
      </template>
    </template>

    <!-- root add -->
    <button data-test="root-add-btn" class="flex items-center gap-2 py-1.5 pl-2 mt-0.5 text-[13.5px] text-muted hover:text-primary" @click="openPicker(null)">
      <Plus :size="16" /> Add node
    </button>
    <div v-if="activePickerParent === null" class="px-2 pb-1">
      <NodePicker :definitions="definitions" :filter-type="null" @select="handleSelect" @create-new="handleCreateNew" />
    </div>
  </div>
</template>
