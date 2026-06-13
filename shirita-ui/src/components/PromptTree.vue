<script setup lang="ts">
import { ref, computed } from 'vue'
import type { Definition, PromptNode } from '../api/types'
import NodeRow from './NodeRow.vue'
import NodePicker from './NodePicker.vue'

const props = defineProps<{ nodes: PromptNode[]; definitions: Definition[] }>()
const emit = defineEmits<{ toggleEnabled: [nodeId: string]; addNode: [parentId: string | null, definitionId: string] }>()

const expanded = ref<Set<string>>(new Set())
const activePickerParent = ref<string | null>(null)

const defMap = computed<Record<string, Definition>>(() => {
  const m: Record<string, Definition> = {}
  for (const d of props.definitions) m[d.id] = d
  return m
})

function getChildren(parentId: string | null): PromptNode[] {
  return props.nodes.filter(n => n.parent_id === parentId).sort((a, b) => a.sort_order - b.sort_order)
}

function isExpanded(nodeId: string) { return expanded.value.has(nodeId) }
function toggleExpand(nodeId: string) { if (expanded.value.has(nodeId)) expanded.value.delete(nodeId); else expanded.value.add(nodeId) }
function openPicker(parentId: string | null) { activePickerParent.value = parentId }

function handleSelectDef(definitionId: string) {
  emit('addNode', activePickerParent.value, definitionId)
  activePickerParent.value = null
}
</script>

<template>
  <div data-test="prompt-tree" class="border border-line rounded-xl p-3 bg-white">
    <template v-for="node in getChildren(null)" :key="node.id">
      <NodeRow :node="node" :definitions="defMap" :depth="0" :is-expanded="isExpanded(node.id)"
        @toggle-enabled="emit('toggleEnabled', node.id)" @toggle-expand="toggleExpand(node.id)" @add-child="openPicker(node.id)" />
      <template v-if="node.kind === 'folder' && isExpanded(node.id)">
        <template v-for="child in getChildren(node.id)" :key="child.id">
          <NodeRow :node="child" :definitions="defMap" :depth="1" :is-expanded="false"
            @toggle-enabled="emit('toggleEnabled', child.id)" />
        </template>
      </template>
      <div v-if="activePickerParent === node.id" class="ml-5 mt-1 mb-2">
        <NodePicker :definitions="definitions" :filter-type="node.tag" @select="handleSelectDef" @create-new="() => {}" @change-type="() => {}" />
      </div>
    </template>
    <div class="mt-2">
      <button data-test="root-add-btn" class="flex items-center gap-1 text-[13px] text-muted hover:text-primary" @click="openPicker(null)">+ Add node</button>
      <div v-if="activePickerParent === null" class="mt-1">
        <NodePicker :definitions="definitions" :filter-type="null" @select="handleSelectDef" @create-new="() => {}" @change-type="() => {}" />
      </div>
    </div>
  </div>
</template>
