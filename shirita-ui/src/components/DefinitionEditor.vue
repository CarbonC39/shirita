<script setup lang="ts">
import { ref, computed } from 'vue'
import { Maximize2, Trash2, Upload, Download, Copy, Search, ChevronDown } from 'lucide-vue-next'
import type { Definition } from '../api/types'
import { triggerFromMeta } from '../api/types'
import FullscreenEditor from './FullscreenEditor.vue'
import TriggerEditor from './TriggerEditor.vue'

const props = defineProps<{ definition: Definition; allDefinitions: Definition[] }>()
const emit = defineEmits<{
  'select-definition': [id: string]
  'update:content': [content: string]
  'update:name': [name: string]
  'update:type': [type: string]
  'update:meta': [meta: Record<string, unknown>]
  save: []
  delete: []
  duplicate: []
  import: []
  export: []
}>()

const fullscreenOpen = ref(false)
const open = ref(false)

const typeChips = ['char', 'persona', 'world', 'item', 'prompt']

const matches = computed(() => {
  const q = props.definition.name.trim().toLowerCase()
  const list = q ? props.allDefinitions.filter((d) => d.name.toLowerCase().includes(q)) : props.allDefinitions
  return list.slice(0, 6)
})

function pick(id: string) {
  emit('select-definition', id)
  open.value = false
}
function startNew() {
  emit('select-definition', '')
  open.value = false
}
</script>

<template>
  <div>
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2.5 px-0.5">Definition</h3>

    <!-- merged search + name combobox + ops -->
    <div class="flex items-center gap-2 mb-3">
      <div class="flex-1 relative" @focusout="open = false">
        <div class="flex items-center gap-2.5 border border-line rounded-[10px] bg-white px-3 py-2.5 focus-within:border-primary/50">
          <Search :size="16" class="text-muted shrink-0" />
          <input
            :value="definition.name"
            type="text"
            placeholder="Search a definition, or type a new name…"
            class="flex-1 bg-transparent outline-none text-[14px] text-ink placeholder:text-muted/60"
            @focus="open = true"
            @input="emit('update:name', ($event.target as HTMLInputElement).value); open = true"
          />
          <button class="text-muted shrink-0" tabindex="-1" @mousedown.prevent="open = !open"><ChevronDown :size="16" /></button>
        </div>
        <div v-if="open" class="absolute left-0 right-0 top-full mt-1 bg-white border border-line rounded-[10px] shadow-lg overflow-hidden z-20">
          <button class="w-full text-left px-3 py-2 text-[13.5px] text-primary hover:bg-surface" @mousedown.prevent="startNew">+ New definition</button>
          <button
            v-for="d in matches"
            :key="d.id"
            class="w-full flex items-center gap-2 px-3 py-2 text-left text-[13.5px] hover:bg-surface border-t border-line"
            @mousedown.prevent="pick(d.id)"
          >
            <span class="flex-1 truncate text-ink">{{ d.name }}</span>
            <span class="text-[11px] text-muted uppercase">{{ d.type }}</span>
          </button>
        </div>
      </div>
      <div class="flex items-center">
        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" title="Import" @click="emit('import')"><Upload :size="16" /></button>
        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" title="Export" @click="emit('export')"><Download :size="16" /></button>
        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" title="Duplicate" @click="emit('duplicate')"><Copy :size="16" /></button>
        <button data-test="delete-btn" class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-coral rounded-lg" title="Delete" @click="emit('delete')"><Trash2 :size="16" /></button>
      </div>
    </div>

    <!-- type chips -->
    <div class="flex items-center gap-2 flex-wrap mb-3">
      <span class="text-[12px] text-muted">Type</span>
      <button
        v-for="t in typeChips"
        :key="t"
        :class="['text-[12px] rounded-full px-3 py-1 border transition-colors',
                 definition.type === t ? 'bg-line/60 text-ink border-line' : 'text-muted border-line hover:text-ink']"
        @click="emit('update:type', t)"
      >{{ t }}</button>
    </div>

    <!-- world-book trigger (container types only) -->
    <div v-if="!['prompt','regex_rule','tool'].includes(definition.type)" class="mb-3">
      <TriggerEditor
        :model-value="triggerFromMeta(definition.meta)"
        @update:model-value="emit('update:meta', { ...definition.meta, trigger: $event })"
      />
    </div>

    <!-- content -->
    <div class="relative">
      <textarea
        :value="definition.content"
        rows="5"
        class="w-full border border-line rounded-[9px] bg-white px-3 py-2.5 pr-9 text-[13px] leading-relaxed text-[#5c6166] resize-y outline-none focus:border-primary/50 font-mono"
        placeholder="Definition content…"
        @input="emit('update:content', ($event.target as HTMLTextAreaElement).value)"
      />
      <button data-test="fullscreen-btn" class="absolute top-2 right-2 p-1 text-muted/70 hover:text-ink" title="Fullscreen" @click="fullscreenOpen = true"><Maximize2 :size="15" /></button>
    </div>

    <div class="flex justify-end mt-3">
      <button data-test="save-btn" class="px-5 py-2 text-[13px] font-medium bg-primary text-white rounded-[9px] hover:bg-primary-strong transition-colors" @click="emit('save')">Save</button>
    </div>

    <FullscreenEditor :model-value="definition.content" :open="fullscreenOpen" @close="fullscreenOpen = false" @update:model-value="emit('update:content', $event)" />
  </div>
</template>
