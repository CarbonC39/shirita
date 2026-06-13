<script setup lang="ts">
import { ref } from 'vue'
import { Maximize2, Save, Trash2, Upload, Download, Copy } from 'lucide-vue-next'
import type { Definition } from '../api/types'
import FullscreenEditor from './FullscreenEditor.vue'

const props = defineProps<{ definition: Definition; allDefinitions: Definition[] }>()
const emit = defineEmits<{
  'select-definition': [id: string]; 'update:content': [content: string]; 'update:name': [name: string]; 'update:type': [type: string]
  save: []; delete: []; duplicate: []; import: []; export: []
}>()

const fullscreenOpen = ref(false)

function onSelect(e: Event) {
  const id = (e.target as HTMLSelectElement).value
  emit('select-definition', id === '__new__' ? '' : id)
}

defineExpose({ fullscreenOpen })
</script>

<template>
  <div class="border border-line rounded-xl bg-white p-4">
    <h3 class="text-[13px] font-semibold text-muted uppercase tracking-wide mb-3">Definition</h3>
    <div class="flex items-center gap-2 mb-3">
      <div class="flex-1 relative">
        <select :value="definition.id || '__new__'" class="w-full border border-line rounded-lg px-3 py-2 text-[14px] bg-white outline-none focus:border-primary/50 appearance-none" @change="onSelect">
          <option value="__new__">+ New definition</option>
          <option v-for="d in allDefinitions" :key="d.id" :value="d.id">{{ d.name }} ({{ d.type }})</option>
        </select>
      </div>
      <select :value="definition.type" class="border border-line rounded-lg px-2.5 py-2 text-[13px] bg-white outline-none focus:border-primary/50" @change="emit('update:type', ($event.target as HTMLSelectElement).value)">
        <option value="char">char</option>
        <option value="world">world</option>
        <option value="persona">persona</option>
        <option value="item">item</option>
        <option value="prompt">prompt</option>
        <option value="regex_rule">regex_rule</option>
        <option value="tool">tool</option>
      </select>
    </div>
    <div class="flex items-center gap-1 mb-3">
      <button class="p-1.5 text-muted hover:text-ink rounded-md" title="Import" @click="emit('import')"><Upload :size="15" /></button>
      <button class="p-1.5 text-muted hover:text-ink rounded-md" title="Export" @click="emit('export')"><Download :size="15" /></button>
      <button class="p-1.5 text-muted hover:text-ink rounded-md" title="Duplicate" @click="emit('duplicate')"><Copy :size="15" /></button>
      <button data-test="delete-btn" class="p-1.5 text-muted hover:text-coral rounded-md" title="Delete" @click="emit('delete')"><Trash2 :size="15" /></button>
    </div>
    <div class="relative">
      <textarea :value="definition.content" rows="6" class="w-full border border-line rounded-lg px-3.5 py-2.5 text-[14px] leading-relaxed resize-y focus:outline-none focus:border-primary/50 font-mono" placeholder="Definition content…" @input="emit('update:content', ($event.target as HTMLTextAreaElement).value)" />
      <button data-test="fullscreen-btn" class="absolute top-2 right-2 p-1 text-muted hover:text-ink bg-white/80 rounded" title="Fullscreen" @click="fullscreenOpen = true"><Maximize2 :size="15" /></button>
    </div>
    <div class="flex items-center justify-between mt-3">
      <span class="text-[11px] text-muted italic">Auto-saved</span>
      <button data-test="save-btn" class="px-4 py-1.5 text-[13px] font-medium bg-primary text-white rounded-full hover:bg-primary-strong transition-colors" @click="emit('save')">
        <Save :size="13" class="inline mr-1" /> Save
      </button>
    </div>
    <FullscreenEditor :model-value="definition.content" :open="fullscreenOpen" @close="fullscreenOpen = false" @update:model-value="emit('update:content', $event)" />
  </div>
</template>
