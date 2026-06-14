<script setup lang="ts">
import { ref, computed } from 'vue'
import { Maximize2, Trash2, Upload, Download, Copy, Search, ChevronDown, X } from 'lucide-vue-next'
import type { Definition, DefType } from '../api/types'
import { triggerFromMeta } from '../api/types'
import { estimateTokens, formatTokens } from '../utils/tokens'
import FullscreenEditor from './FullscreenEditor.vue'
import TriggerEditor from './TriggerEditor.vue'

const props = withDefaults(
  defineProps<{ definition: Definition; allDefinitions: Definition[]; types?: DefType[] }>(),
  { types: () => [] },
)
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
  'create-type': [name: string]
  'delete-type': [id: string]
}>()

const fullscreenOpen = ref(false)
const open = ref(false)
const contentTokens = computed(() => estimateTokens(props.definition.content))

// Registered container types + the reserved `prompt`, tinted per the palette.
// Builtin types can't be deleted; custom ones can.
const typeChips = computed(() => [
  ...props.types.map((t) => ({ id: t.id, label: t.label, builtin: t.builtin })),
  { id: 'prompt', label: 'Prompt', builtin: true },
])

const addingType = ref(false)
const newTypeName = ref('')
function confirmNewType() {
  const name = newTypeName.value.trim()
  if (!name) return
  emit('create-type', name)
  newTypeName.value = ''
  addingType.value = false
}
const chipTint: Record<string, string> = {
  char: 'bg-sky/30 border-sky/40', persona: 'bg-coral/30 border-coral/40',
  world: 'bg-mauve/25 border-mauve/40', prompt: 'bg-line/60 border-line',
}

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
        <div class="flex items-center gap-2.5 border border-line rounded-[10px] bg-card px-3 py-2.5 focus-within:border-primary/50">
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
        <transition name="expand">
        <div v-if="open" class="absolute left-0 right-0 top-full mt-1 bg-card border border-line rounded-[10px] shadow-lg overflow-hidden z-20">
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
        </transition>
      </div>
      <div class="flex items-center">
        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" title="Import" @click="emit('import')"><Upload :size="16" /></button>
        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" title="Export" @click="emit('export')"><Download :size="16" /></button>
        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" title="Duplicate" @click="emit('duplicate')"><Copy :size="16" /></button>
        <button data-test="delete-btn" class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-coral rounded-lg" title="Delete" @click="emit('delete')"><Trash2 :size="16" /></button>
      </div>
    </div>

    <!-- type chips (with create / delete custom types) -->
    <div class="flex items-center gap-2 flex-wrap mb-3">
      <span class="text-[12px] text-muted">Type</span>
      <span v-for="t in typeChips" :key="t.id" class="inline-flex items-center">
        <button
          data-test="type-chip"
          :class="['text-[12px] rounded-full px-3 py-1 border transition-colors',
                   definition.type === t.id ? (chipTint[t.id] || 'bg-line/60 border-line') + ' text-ink'
                                            : 'text-muted border-line hover:text-ink']"
          @click="emit('update:type', t.id)"
        >{{ t.label }}</button>
        <button
          v-if="!t.builtin"
          data-test="type-delete"
          class="ml-0.5 text-muted/60 hover:text-coral transition-colors"
          title="Delete type"
          @click.stop="emit('delete-type', t.id)"
        ><X :size="13" /></button>
      </span>

      <button
        v-if="!addingType"
        data-test="type-new"
        class="text-[12px] rounded-full px-2.5 py-1 border border-dashed border-line text-muted hover:text-primary hover:border-primary/40 transition-colors"
        @click="addingType = true"
      >+ Type</button>
      <span v-else class="inline-flex items-center gap-1">
        <input
          v-model="newTypeName"
          data-test="type-new-input"
          type="text"
          placeholder="New type…"
          class="field w-[120px] !py-1 text-[12px]"
          @keyup.enter="confirmNewType"
        />
        <button class="btn btn-primary !px-2.5 !py-1 text-[12px]" @click="confirmNewType">Add</button>
        <button class="text-muted hover:text-ink" title="Cancel" @click="addingType = false; newTypeName = ''"><X :size="14" /></button>
      </span>
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
        class="w-full border border-line rounded-[9px] bg-card px-3 py-2.5 pr-9 text-[13px] leading-relaxed text-ink/75 resize-y outline-none focus:border-primary/50 font-mono"
        placeholder="Definition content…"
        @input="emit('update:content', ($event.target as HTMLTextAreaElement).value)"
      />
      <button data-test="fullscreen-btn" class="absolute top-2 right-2 p-1 text-muted/70 hover:text-ink" title="Fullscreen" @click="fullscreenOpen = true"><Maximize2 :size="15" /></button>
    </div>

    <div class="flex items-center justify-between mt-3">
      <span class="text-[11.5px] text-muted tabular-nums">~{{ formatTokens(contentTokens) }} tokens</span>
      <button data-test="save-btn" class="px-5 py-2 text-[13px] font-medium bg-primary text-white rounded-[9px] hover:bg-primary-strong transition-colors" @click="emit('save')">Save</button>
    </div>

    <FullscreenEditor :model-value="definition.content" :open="fullscreenOpen" @close="fullscreenOpen = false" @update:model-value="emit('update:content', $event)" />
  </div>
</template>
