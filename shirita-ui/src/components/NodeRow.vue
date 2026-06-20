<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronRight, Folder, FileText, History, Package, Check, Maximize2, Trash2, Plus, GripVertical } from 'lucide-vue-next'
import type { Definition, PromptNode, Trigger } from '../api/types'
import { triggerFromMeta } from '../api/types'
import FullscreenEditor from './FullscreenEditor.vue'
import TriggerEditor from './TriggerEditor.vue'
import ToggleSwitch from './ToggleSwitch.vue'

const props = defineProps<{
  node: PromptNode
  definitions: Record<string, Definition>
  depth: number
  isExpanded: boolean
}>()

const emit = defineEmits<{
  toggleEnabled: []
  toggleExpand: []
  updateContent: [content: string]
  delete: []
  updateTrigger: [trigger: Trigger]
  updateNodeMeta: [meta: Record<string, unknown>]
  add: []
}>()

const { t } = useI18n()
const isFolder = computed(() => props.node.kind === 'folder')
const isHistory = computed(() => props.node.kind === 'history')
const isContent = computed(() => props.node.kind === 'content')

const def = computed<Definition | null>(() =>
  props.node.definition_id ? props.definitions[props.node.definition_id] ?? null : null,
)

const label = computed(() => {
  if (isHistory.value) return t('prompt.chatHistory')
  if (isContent.value) return t('prompt.contentMount')
  if (isFolder.value) return props.node.tag || t('prompt.folderFallback')
  return def.value ? def.value.name : t('prompt.missing')
})

// palette tint per definition/container type
const typeTint: Record<string, string> = {
  char: 'text-sky', persona: 'text-coral', world: 'text-mauve', prompt: 'text-muted',
}
const iconColor = computed(() => {
  const t = isFolder.value ? (props.node.tag ?? '') : (def.value?.type ?? '')
  return props.node.enabled ? (typeTint[t] ?? 'text-muted') : 'text-muted/40'
})

// Per-use wrap_in_tag override (this template placement only) takes priority
// over the definition's own setting when present, mirroring assembly.rs's
// maybe_wrap precedence.
const showWrapToggle = computed(
  () => !!def.value && !['regex_rule', 'first_message'].includes(def.value.type),
)
const wrapValue = computed(() => {
  const nodeMeta = props.node.meta as Record<string, unknown>
  if (typeof nodeMeta.wrap_in_tag === 'boolean') return nodeMeta.wrap_in_tag
  return (def.value?.meta as Record<string, unknown> | undefined)?.wrap_in_tag === true
})
function updateWrap(v: boolean) {
  emit('updateNodeMeta', { ...(props.node.meta as Record<string, unknown>), wrap_in_tag: v })
}

// Folder selection policy: 'all' (default) renders every enabled child; 'one'
// renders only the first (a single-select hub). Stored in node.meta.select;
// the backend (assembly.rs pack_pairs) honors it.
const selectMode = computed(() =>
  ((props.node.meta as Record<string, unknown>).select === 'one' ? 'one' : 'all'),
)
function toggleSelectMode() {
  const next = selectMode.value === 'one' ? 'all' : 'one'
  emit('updateNodeMeta', { ...(props.node.meta as Record<string, unknown>), select: next })
}

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
      <!-- drag handle: the row is only draggable when grabbed here (PromptTree
           gates dragstart on this element), so the rest of the row stays clickable -->
      <span
        data-test="drag-handle"
        class="shrink-0 -ml-1 cursor-grab active:cursor-grabbing text-muted/35 group-hover:text-muted/70 transition-colors"
        :title="$t('chat.dragReorder')"
      ><GripVertical :size="15" /></span>

      <!-- enable checkbox: rounded square, teal when on -->
      <button
        data-test="enable-checkbox"
        :aria-pressed="node.enabled"
        :class="['w-[18px] h-[18px] rounded-[5px] grid place-items-center shrink-0 transition-colors',
                 node.enabled ? 'bg-primary' : 'bg-card border border-[#d4d6da]']"
        @click="emit('toggleEnabled')"
      >
        <Check v-if="node.enabled" :size="12" class="text-white" :stroke-width="3" />
      </button>

      <!-- type icon -->
      <History v-if="isHistory" :size="16" class="text-primary shrink-0" :stroke-width="1.8" />
      <Package v-else-if="isContent" :size="16" class="text-primary shrink-0" :stroke-width="1.8" />
      <Folder v-else-if="isFolder" :size="17" :class="iconColor" class="shrink-0" :stroke-width="1.8" />
      <FileText v-else :size="16" :class="iconColor" class="shrink-0" :stroke-width="1.8" />

      <span :class="['truncate flex-1', isFolder ? 'font-semibold' : '', node.enabled ? 'text-ink' : 'text-muted']">{{ label }}</span>

      <!-- folder selection policy: all vs single-select -->
      <button
        v-if="isFolder"
        data-test="select-mode"
        class="shrink-0 text-[11px] px-1.5 py-0.5 rounded-md border border-line text-muted hover:text-ink transition-colors"
        :title="$t('prompt.selectModeHint')"
        @click.stop="toggleSelectMode"
      >{{ selectMode === 'one' ? $t('prompt.selectOne') : $t('prompt.selectAll') }}</button>

      <!-- add-to-container: lives beside delete, no extra row (containers only) -->
      <button
        v-if="isFolder"
        data-test="node-add"
        class="text-muted/70 hover:text-primary shrink-0 p-0.5 transition-colors"
        :title="$t('prompt.addToContainer')"
        @click.stop="emit('add')"
      ><Plus :size="15" /></button>

      <!-- delete (history + content rows render none) -->
      <button
        v-if="!isHistory && !isContent"
        data-test="node-delete"
        class="text-muted/0 group-hover:text-muted/70 hover:!text-coral shrink-0 p-0.5 transition-colors"
        :title="$t('common.delete')"
        @click.stop="emit('delete')"
      ><Trash2 :size="15" /></button>

      <!-- trailing expand chevron: folders expand children, refs expand content -->
      <button v-if="!isHistory && !isContent" data-test="expand-btn" class="text-muted/70 hover:text-ink shrink-0 p-0.5" @click="emit('toggleExpand')">
        <ChevronRight :size="16" :class="isExpanded ? 'rotate-90' : ''" class="transition-transform" />
      </button>
    </div>

    <!-- inline content editor for ref nodes -->
    <transition name="expand">
    <div v-if="!isFolder && !isHistory && !isContent && isExpanded" :style="{ paddingLeft: `${8 + (depth + 1) * 26}px` }" class="pr-2 pb-2 pt-0.5">
      <div class="relative">
        <textarea
          v-model="draft"
          rows="3"
          data-test="node-content"
          class="w-full resize-y rounded-[9px] border border-line bg-card px-3 py-2.5 pr-8 text-[13px] leading-relaxed text-ink/75 outline-none focus:border-primary/50"
          :placeholder="$t('definition.contentPlaceholder')"
          @blur="commit"
        />
        <button
          data-test="node-fullscreen"
          class="absolute right-2 top-2 text-muted/70 hover:text-ink"
          :title="$t('settings.fullscreen')"
          @click="fullscreenOpen = true"
        >
          <Maximize2 :size="15" />
        </button>
      </div>

      <!-- inline world-book trigger (container refs only) -->
      <div v-if="def && !['prompt','regex_rule','tool'].includes(def.type)" class="mt-2.5">
        <TriggerEditor
          :model-value="triggerFromMeta(def.meta)"
          @update:model-value="emit('updateTrigger', $event)"
        />
      </div>

      <!-- per-use wrap_in_tag override: this template placement only -->
      <label
        v-if="showWrapToggle"
        data-test="node-wrap-in-tag"
        class="flex items-center gap-2 mt-2.5 text-[13px] text-ink"
        :title="$t('definition.wrapInTagHint')"
      >
        {{ $t('definition.wrapInTag') }}
        <ToggleSwitch :model-value="wrapValue" @update:model-value="updateWrap" />
      </label>
    </div>
    </transition>

    <FullscreenEditor
      :model-value="draft"
      :open="fullscreenOpen"
      @update:model-value="draft = $event"
      @close="closeFullscreen"
    />
  </div>
</template>
