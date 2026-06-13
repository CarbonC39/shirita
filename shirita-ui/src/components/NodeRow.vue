<script setup lang="ts">
import { computed } from 'vue'
import { ChevronDown, GripVertical, Plus } from 'lucide-vue-next'
import type { Definition, PromptNode } from '../api/types'

const props = defineProps<{
  node: PromptNode
  definitions: Record<string, Definition>
  depth: number
  isExpanded: boolean
}>()

const emit = defineEmits<{ toggleEnabled: []; toggleExpand: []; addChild: [] }>()

const label = computed(() => {
  if (props.node.kind === 'folder') return props.node.tag || '(folder)'
  const def = props.node.definition_id ? props.definitions[props.node.definition_id] : null
  return def ? def.name : '(missing)'
})

const isFolder = computed(() => props.node.kind === 'folder')
</script>

<template>
  <div data-test="node-row" :style="{ paddingLeft: `${depth * 20}px` }" class="flex items-center gap-1.5 py-1.5 group text-[14px]">
    <GripVertical :size="14" class="text-muted/40 shrink-0" />
    <input type="checkbox" :checked="node.enabled" class="w-3.5 h-3.5 rounded accent-primary shrink-0" data-test="enable-checkbox" @change="emit('toggleEnabled')" />
    <button v-if="isFolder" data-test="expand-btn" class="text-muted hover:text-ink shrink-0" @click="emit('toggleExpand')">
      <ChevronDown :size="14" :class="isExpanded ? '' : '-rotate-90'" class="transition-transform" />
    </button>
    <span v-else class="w-[14px] shrink-0" />
    <span :class="['truncate flex-1', isFolder ? 'font-semibold text-mauve' : 'text-ink', !node.enabled ? 'line-through text-muted/50' : '']">{{ label }}</span>
    <button v-if="isFolder" data-test="add-child-btn" class="text-muted hover:text-primary opacity-0 group-hover:opacity-100 transition-opacity shrink-0" @click="emit('addChild')">
      <Plus :size="16" />
    </button>
    <span v-else class="w-[24px] shrink-0" />
  </div>
</template>
