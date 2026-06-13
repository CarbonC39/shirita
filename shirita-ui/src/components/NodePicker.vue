<script setup lang="ts">
import { ref, computed } from 'vue'
import { Search } from 'lucide-vue-next'
import type { Definition } from '../api/types'

const props = defineProps<{ definitions: Definition[]; filterType: string | null }>()
const emit = defineEmits<{ select: [definitionId: string]; createNew: []; changeType: [type: string] }>()

const query = ref('')
const isOpen = ref(false)
const typeOptions = ['char', 'world', 'persona', 'item', 'prompt']

const filtered = computed(() => {
  let defs = props.definitions
  if (props.filterType) defs = defs.filter(d => d.type === props.filterType)
  if (query.value.trim()) { const q = query.value.toLowerCase(); defs = defs.filter(d => d.name.toLowerCase().includes(q)) }
  return defs.slice(0, 8)
})

function open() { isOpen.value = true; query.value = '' }
function close() { isOpen.value = false }
defineExpose({ open, close })
</script>

<template>
  <div v-if="isOpen" data-test="node-picker" class="bg-white border border-line rounded-xl shadow-lg p-3 w-72 z-20">
    <div class="flex items-center gap-2 pb-2 mb-2 border-b border-line">
      <Search :size="14" class="text-muted shrink-0" />
      <input v-model="query" type="text" placeholder="Search definitions…" class="flex-1 text-[13px] bg-transparent outline-none placeholder:text-muted/50" />
    </div>
    <div class="max-h-40 overflow-y-auto">
      <button v-for="def in filtered" :key="def.id" class="w-full text-left px-2 py-1.5 text-[13px] hover:bg-surface rounded-md flex items-center gap-2" @click="emit('select', def.id); close()">
        <span class="text-[11px] text-muted uppercase w-12 shrink-0">{{ def.type }}</span>
        <span class="truncate">{{ def.name }}</span>
      </button>
    </div>
    <p v-if="filtered.length === 0" class="text-muted text-xs py-2 text-center">No matching definitions</p>
    <div class="border-t border-line my-2" />
    <button class="w-full text-left px-2 py-1.5 text-[13px] text-primary hover:bg-surface rounded-md" @click="emit('createNew'); close()">+ New definition</button>
    <div class="mt-1 text-[11px] text-muted px-2">Other type:</div>
    <div class="flex flex-wrap gap-1 mt-1">
      <button v-for="t in typeOptions" :key="t" :class="['px-2 py-0.5 text-[11px] rounded-full', props.filterType === t ? 'bg-primary/10 text-primary' : 'text-muted hover:text-ink bg-line/30']" @click="emit('changeType', t)">{{ t }}</button>
    </div>
  </div>
</template>
