<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { ChevronRight, Folder, FileText, History, Check, Maximize2, Trash2 } from 'lucide-vue-next'
import type { Definition, PromptNode } from '../api/types'
import FullscreenEditor from './FullscreenEditor.vue'

const props = defineProps<{
  node: PromptNode
  definitions: Record<string, Definition>
  depth: number
  isExpanded: boolean
}>()

const emit = defineEmits<{ toggleEnabled: []; toggleExpand: []; updateContent: [content: string]; delete: [] }>()

const isFolder = computed(() => props.node.kind === 'folder')
const isHistory = computed(() => props.node.kind === 'history')

const def = computed<Definition | null>(() =>
  props.node.definition_id ? props.definitions[props.node.definition_id] ?? null : null,
)

const label = computed(() => {
  if (isHistory.value) return 'Chat history'
  if (isFolder.value) return props.node.tag || '(folder)'
  return def.value ? def.value.name : '(missing)'
})

// palette tint per definition/container type
const typeTint: Record<string, string> = {
  char: 'text-sky', persona: 'text-coral', world: 'text-mauve', prompt: 'text-muted',
}
const iconColor = computed(() => {
  const t = isFolder.value ? (props.node.tag ?? '') : (def.value?.type ?? '')
  return props.node.enabled ? (typeTint[t] ?? 'text-muted') : 'text-muted/40'
})

// Local editable copy of the referenced definition's content; persisted on blur.
const draft = ref(def.value?.content ?? '')
watch(def, (d) => { draft.value = d?.content ?? '' })

const fullscreenOpen = ref(false)
function commit() { if (draft.value !== (def.value?.content ?? '')) emit('updateContent', draft.value) }
function closeFullscreen() { fullscreenOpen.value = false; commit() }
</script>

<template>
  <div>
    <div
      data-test="node-row"
      :style="{ paddingLeft: `${8 + depth * 26}px` }"
      class="flex items-center gap-2.5 py-2 pr-2 rounded-lg hover:bg-surface/70 group text-[14px]"
    >
      <!-- enable checkbox: rounded square, teal when on -->
      <button
        data-test="enable-checkbox"
        :aria-pressed="node.enabled"
        :class="['w-[18px] h-[18px] rounded-[5px] grid place-items-center shrink-0 transition-colors',
                 node.enabled ? 'bg-primary' : 'bg-white border border-[#d4d6da]']"
        @click="emit('toggleEnabled')"
      >
        <Check v-if="node.enabled" :size="12" class="text-white" :stroke-width="3" />
      </button>

      <!-- type icon -->
      <History v-if="isHistory" :size="16" class="text-primary shrink-0" :stroke-width="1.8" />
      <Folder v-else-if="isFolder" :size="17" :class="iconColor" class="shrink-0" :stroke-width="1.8" />
      <FileText v-else :size="16" :class="iconColor" class="shrink-0" :stroke-width="1.8" />

      <span :class="['truncate flex-1', isFolder ? 'font-semibold' : '', node.enabled ? 'text-ink' : 'text-muted']">{{ label }}</span>

      <!-- delete (history rows render none) -->
      <button
        v-if="!isHistory"
        data-test="node-delete"
        class="text-muted/0 group-hover:text-muted/70 hover:!text-coral shrink-0 p-0.5 transition-colors"
        title="Delete"
        @click.stop="emit('delete')"
      ><Trash2 :size="15" /></button>

      <!-- trailing expand chevron: folders expand children, refs expand content -->
      <button data-test="expand-btn" class="text-muted/70 hover:text-ink shrink-0 p-0.5" @click="emit('toggleExpand')">
        <ChevronRight :size="16" :class="isExpanded ? 'rotate-90' : ''" class="transition-transform" />
      </button>
    </div>

    <!-- inline content editor for ref nodes -->
    <div v-if="!isFolder && !isHistory && isExpanded" :style="{ paddingLeft: `${8 + (depth + 1) * 26}px` }" class="pr-2 pb-2 pt-0.5">
      <div class="relative">
        <textarea
          v-model="draft"
          rows="3"
          data-test="node-content"
          class="w-full resize-y rounded-[9px] border border-line bg-white px-3 py-2.5 pr-8 text-[13px] leading-relaxed text-[#5c6166] outline-none focus:border-primary/50"
          placeholder="Definition content…"
          @blur="commit"
        />
        <button
          data-test="node-fullscreen"
          class="absolute right-2 top-2 text-muted/70 hover:text-ink"
          title="Fullscreen"
          @click="fullscreenOpen = true"
        >
          <Maximize2 :size="15" />
        </button>
      </div>
    </div>

    <FullscreenEditor
      :model-value="draft"
      :open="fullscreenOpen"
      @update:model-value="draft = $event"
      @close="closeFullscreen"
    />
  </div>
</template>
