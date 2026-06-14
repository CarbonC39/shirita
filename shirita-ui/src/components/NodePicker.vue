<script setup lang="ts">
import { ref, computed } from 'vue'
import { Search, Plus, LayoutGrid, ChevronRight } from 'lucide-vue-next'
import type { Definition, DefType } from '../api/types'

const props = defineProps<{ definitions: Definition[]; filterType: string | null; types: DefType[] }>()
const emit = defineEmits<{ select: [definitionId: string]; createNew: [type: string] }>()

const query = ref('')
const showTypes = ref(false)
// Local active type so "Other type" can switch the picker without round-tripping the parent.
const activeType = ref<string | null>(props.filterType)

// Friendly tint per definition type (keeps the picker playful, not technical).
const typeTint: Record<string, string> = {
  char: 'bg-sky/40', persona: 'bg-coral/40', world: 'bg-mauve/40',
  prompt: 'bg-muted/30',
}

const filtered = computed(() => {
  let defs = props.definitions
  if (activeType.value) defs = defs.filter((d) => d.type === activeType.value)
  if (query.value.trim()) {
    const q = query.value.toLowerCase()
    defs = defs.filter((d) => d.name.toLowerCase().includes(q))
  }
  return defs
})

const visible = computed(() => filtered.value.slice(0, 4))
const moreCount = computed(() => Math.max(0, filtered.value.length - visible.value.length))
const newLabel = computed(() => (activeType.value ? `New ${activeType.value}…` : 'New definition…'))
</script>

<template>
  <div data-test="node-picker" class="border border-line rounded-[10px] bg-surface/60 overflow-hidden">
    <!-- search -->
    <div class="flex items-center gap-2 px-3 py-2 border-b border-line">
      <Search :size="15" class="text-muted shrink-0" />
      <input
        v-model="query"
        type="text"
        :placeholder="activeType ? `Search ${activeType}…` : 'Search…'"
        class="flex-1 text-[13px] bg-transparent outline-none placeholder:text-muted/60"
      />
    </div>

    <!-- existing definitions of this type -->
    <button
      v-for="def in visible"
      :key="def.id"
      class="w-full flex items-center gap-2.5 px-3 py-2 text-left hover:bg-white transition-colors"
      @click="emit('select', def.id)"
    >
      <span :class="['w-5 h-5 rounded-full shrink-0', typeTint[def.type] || 'bg-muted/30']" />
      <span class="flex-1 text-[13.5px] text-ink truncate">{{ def.name }}</span>
    </button>
    <div v-if="moreCount > 0" class="px-3 pb-1.5 -mt-0.5 text-[11px] text-muted/70">+{{ moreCount }} more</div>
    <p v-if="filtered.length === 0" class="px-3 py-2 text-[12px] text-muted/70">No matching definitions</p>

    <!-- new -->
    <button
      data-test="picker-new"
      class="w-full flex items-center gap-2.5 px-3 py-2 text-left border-t border-line hover:bg-white transition-colors text-ink"
      @click="emit('createNew', activeType || 'prompt')"
    >
      <Plus :size="15" class="shrink-0" />
      <span class="text-[13.5px]">{{ newLabel }}</span>
    </button>

    <!-- other type -->
    <button
      class="w-full flex items-center gap-2.5 px-3 py-2 text-left border-t border-line hover:bg-white transition-colors text-muted"
      @click="showTypes = !showTypes"
    >
      <LayoutGrid :size="15" class="shrink-0" />
      <span class="flex-1 text-[13.5px]">Other type</span>
      <ChevronRight :size="15" :class="showTypes ? 'rotate-90' : ''" class="transition-transform" />
    </button>
    <div v-if="showTypes" class="flex flex-wrap gap-1.5 px-3 py-2 border-t border-line bg-white/50">
      <button
        v-for="t in types"
        :key="t.id"
        :class="['px-2.5 py-1 text-[12px] rounded-full border transition-colors',
                 activeType === t.id ? 'bg-primary/10 text-primary border-primary/30' : 'text-muted border-line hover:text-ink']"
        @click="activeType = t.id; showTypes = false"
      >{{ t.label }}</button>
    </div>
  </div>
</template>
