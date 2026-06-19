<script setup lang="ts">
import { ref, computed } from 'vue'
import { Maximize2, Pencil, Trash2, Upload, Download, Copy, Search, ChevronDown, X } from 'lucide-vue-next'
import type { Definition, DefType } from '../api/types'
import { triggerFromMeta } from '../api/types'
import { estimateTokens, formatTokens } from '../utils/tokens'
import FullscreenEditor from './FullscreenEditor.vue'
import TriggerEditor from './TriggerEditor.vue'
import ToggleSwitch from './ToggleSwitch.vue'
import AssetPicker from './AssetPicker.vue'

const props = withDefaults(
  defineProps<{ definition: Definition; allDefinitions: Definition[]; types?: DefType[]; active?: boolean; headerActions?: boolean }>(),
  { types: () => [], active: false, headerActions: true },
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

// Per-definition world-info scan settings (live on meta.scan now, not in global
// Settings). Defaults mirror the backend: depth 4, recursive on.
const scan = computed(() => {
  const s = (props.definition.meta as Record<string, unknown>).scan as { depth?: number; recursive?: boolean } | undefined
  return { depth: s?.depth ?? 4, recursive: s?.recursive ?? true }
})
function updateScan(patch: { depth?: number; recursive?: boolean }) {
  emit('update:meta', { ...props.definition.meta, scan: { ...scan.value, ...patch } })
}

// World-info trigger + scan settings only make sense for container (lore) types,
// not for prompt/regex_rule/tool/first_message refs.
const isContainerType = computed(() => !['prompt', 'regex_rule', 'tool', 'first_message'].includes(props.definition.type))
// wrap_in_tag affects rendering, so it applies to anything that renders into the
// prompt (i.e. everything except regex_rule and first_message, neither of
// which render as a plain prompt fragment).
const showWrapInTag = computed(() => !['regex_rule', 'first_message'].includes(props.definition.type))

// Registered container types + the reserved `prompt`/`first_message`, tinted
// per the palette. Builtin types can't be deleted; custom ones can.
const typeChips = computed(() => [
  ...props.types.map((t) => ({ id: t.id, label: t.label, builtin: t.builtin })),
  { id: 'prompt', label: 'Prompt', builtin: true },
  { id: 'first_message', label: 'Message', builtin: true },
])

// `meta.depth` unset = a session-start greeting (seeded once when the chat is
// created, with alternates as swipes). Set = a depth_prompt-style insert,
// spliced into chat history every turn at that distance from the end.
function updateDepth(raw: string) {
  const meta = { ...(props.definition.meta as Record<string, unknown>) }
  if (raw === '') {
    delete meta.depth
  } else {
    const n = parseInt(raw, 10)
    if (!Number.isNaN(n)) meta.depth = Math.max(0, n)
  }
  emit('update:meta', meta)
}

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
  first_message: 'bg-line/60 border-line',
}

const search = ref('')
const renaming = ref(false)

const matches = computed(() => {
  const q = search.value.trim().toLowerCase()
  const list = q ? props.allDefinitions.filter((d) => d.name.toLowerCase().includes(q)) : props.allDefinitions
  // Exclude the currently selected definition so it doesn't appear as a duplicate
  // (it's already shown in the editor body).
  return list.filter((d) => d.id !== props.definition.id).slice(0, 6)
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
    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2.5 px-0.5">{{ $t('definition.heading') }}</h3>

    <!-- rename inline input: replaces the heading area when active -->
    <div v-if="renaming" class="flex items-center gap-2 mb-2.5 px-0.5">
      <input
        :value="definition.name"
        type="text"
        data-test="def-name-input"
        class="field flex-1"
        placeholder="Name"
        @input="emit('update:name', ($event.target as HTMLInputElement).value)"
        @blur="renaming = false"
        @keydown.enter="renaming = false"
      />
      <button class="text-muted hover:text-ink text-[12px] shrink-0" @click="renaming = false">{{ $t('common.done') }}</button>
    </div>

    <!-- search + definition picker + action buttons -->
    <div class="flex items-center gap-2 mb-3">
      <div class="flex-1 relative" @focusout="open = false">
        <div class="flex items-center gap-2.5 border border-line rounded-[10px] bg-card px-3 py-2.5 focus-within:border-primary/50">
          <Search :size="16" class="text-muted shrink-0" />
          <input
            v-model="search"
            type="text"
            data-test="def-search"
            :placeholder="$t('definition.searchPlaceholder')"
            class="flex-1 bg-transparent outline-none text-[14px] text-ink placeholder:text-muted/60"
            @focus="open = true"
          />
          <button class="text-muted shrink-0" tabindex="-1" @mousedown.prevent="open = !open"><ChevronDown :size="16" /></button>
        </div>
        <transition name="expand">
        <div v-if="open" class="absolute left-0 right-0 top-full mt-1 bg-card border border-line rounded-[10px] shadow-lg overflow-hidden z-20">
          <button class="w-full text-left px-3 py-2 text-[13.5px] text-primary hover:bg-surface" @mousedown.prevent="startNew">{{ $t('definition.newDefinition') }}</button>
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
      <div v-if="headerActions" class="flex items-center">
        <button data-test="rename-btn" class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" :title="$t('common.rename')" @click="renaming = !renaming"><Pencil :size="15" /></button>
        <button data-test="import-btn" class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" :title="$t('common.import')" @click="emit('import')"><Upload :size="16" /></button>
        <button data-test="export-btn" class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" :title="$t('common.export')" @click="emit('export')"><Download :size="16" /></button>
        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg" :title="$t('common.duplicate')" @click="emit('duplicate')"><Copy :size="16" /></button>
        <button data-test="delete-btn" class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-coral rounded-lg" :title="$t('common.delete')" @click="emit('delete')"><Trash2 :size="16" /></button>
      </div>
    </div>

    <!-- editor body: revealed only once a definition is picked or a new one started -->
    <template v-if="active">
    <!-- type chips (with create / delete custom types) -->
    <div class="flex items-center gap-2 flex-wrap mb-3">
      <span class="text-[12px] text-muted">{{ $t('definition.typeLabel') }}</span>
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
          :title="$t('definition.deleteTypeTitle')"
          @click.stop="emit('delete-type', t.id)"
        ><X :size="13" /></button>
      </span>

      <button
        v-if="!addingType"
        data-test="type-new"
        class="text-[12px] rounded-full px-2.5 py-1 border border-dashed border-line text-muted hover:text-primary hover:border-primary/40 transition-colors"
        @click="addingType = true"
      >{{ $t('definition.addType') }}</button>
      <span v-else class="inline-flex items-center gap-1">
        <input
          v-model="newTypeName"
          data-test="type-new-input"
          type="text"
          :placeholder="$t('definition.newTypePlaceholder')"
          class="field w-[120px] !py-1 text-[12px]"
          @keyup.enter="confirmNewType"
        />
        <button class="btn btn-primary !px-2.5 !py-1 text-[12px]" @click="confirmNewType">{{ $t('common.add') }}</button>
        <button class="text-muted hover:text-ink" :title="$t('common.cancel')" @click="addingType = false; newTypeName = ''"><X :size="14" /></button>
      </span>
    </div>

    <!-- persona avatar (user identity) -->
    <div v-if="definition.type === 'persona'" data-test="persona-avatar" class="mb-3">
      <label class="text-[12px] text-muted block mb-1.5">{{ $t('definition.avatar') }}</label>
      <AssetPicker
        shape="circle"
        kind="avatar"
        :model-value="(definition.meta as any).avatar || ''"
        @update:model-value="emit('update:meta', { ...definition.meta, avatar: $event })"
      />
    </div>

    <!-- message type: greeting (no depth) vs. depth-inserted note -->
    <div v-if="definition.type === 'first_message'" data-test="message-type-fields" class="mb-3 space-y-2">
      <p class="text-[12px] text-muted">{{ $t('definition.messageTypeHint') }}</p>
      <div class="flex items-center gap-4 flex-wrap">
        <label class="flex items-center gap-2 text-[13px] text-ink">
          {{ $t('definition.depth') }}
          <input
            data-test="message-depth"
            :value="(definition.meta as Record<string, unknown>).depth ?? ''"
            type="number" min="0"
            class="field !py-1 w-[64px] text-right tabular-nums"
            :placeholder="$t('definition.depthPlaceholder')"
            @input="updateDepth(($event.target as HTMLInputElement).value)"
          />
        </label>
        <label class="flex items-center gap-2 text-[13px] text-ink">
          {{ $t('definition.role') }}
          <select
            data-test="message-role"
            :value="(definition.meta as Record<string, unknown>).role || 'system'"
            class="field !py-1 text-[12px]"
            @change="emit('update:meta', { ...definition.meta, role: ($event.target as HTMLSelectElement).value })"
          >
            <option value="system">{{ $t('definition.roleSystem') }}</option>
            <option value="user">{{ $t('definition.roleUser') }}</option>
            <option value="assistant">{{ $t('definition.roleAssistant') }}</option>
          </select>
        </label>
      </div>
    </div>

    <!-- world-book trigger + scan settings (container types only) -->
    <div v-if="isContainerType" class="mb-3 space-y-2.5">
      <TriggerEditor
        :model-value="triggerFromMeta(definition.meta)"
        @update:model-value="emit('update:meta', { ...definition.meta, trigger: $event })"
      />
      <div class="flex items-center gap-4 flex-wrap">
        <label class="flex items-center gap-2 text-[13px] text-ink">
          {{ $t('definition.scanDepth') }}
          <input
            data-test="scan-depth"
            :value="scan.depth"
            type="number" min="1" max="20"
            class="field !py-1 w-[64px] text-right tabular-nums"
            @input="updateScan({ depth: parseInt(($event.target as HTMLInputElement).value) || 1 })"
          />
        </label>
        <label class="flex items-center gap-2 text-[13px] text-ink">
          {{ $t('definition.recursive') }}
          <ToggleSwitch :model-value="scan.recursive" @update:model-value="updateScan({ recursive: $event })" />
        </label>
        <label v-if="showWrapInTag" class="flex items-center gap-2 text-[13px] text-ink" :title="$t('definition.wrapInTagHint')">
          {{ $t('definition.wrapInTag') }}
          <ToggleSwitch
            data-test="wrap-in-tag"
            :model-value="(definition.meta as Record<string, unknown>).wrap_in_tag === true"
            @update:model-value="emit('update:meta', { ...definition.meta, wrap_in_tag: $event })"
          />
        </label>
      </div>
    </div>

    <!-- content -->
    <div class="relative">
      <textarea
        :value="definition.content"
        rows="5"
        class="w-full border border-line rounded-[9px] bg-card px-3 py-2.5 pr-9 text-[13px] leading-relaxed text-ink/75 resize-y outline-none focus:border-primary/50 font-mono"
        :placeholder="$t('definition.contentPlaceholder')"
        @input="emit('update:content', ($event.target as HTMLTextAreaElement).value)"
      />
      <button data-test="fullscreen-btn" class="absolute top-2 right-2 p-1 text-muted/70 hover:text-ink" :title="$t('settings.fullscreen')" @click="fullscreenOpen = true"><Maximize2 :size="15" /></button>
    </div>

    <label v-if="!isContainerType && showWrapInTag" class="flex items-center gap-2 mt-3 text-[13px] text-ink" :title="$t('definition.wrapInTagHint')">
      {{ $t('definition.wrapInTag') }}
      <ToggleSwitch
        data-test="wrap-in-tag"
        :model-value="(definition.meta as Record<string, unknown>).wrap_in_tag === true"
        @update:model-value="emit('update:meta', { ...definition.meta, wrap_in_tag: $event })"
      />
    </label>

    <div class="flex items-center justify-between mt-3">
      <span class="text-[11.5px] text-muted tabular-nums">{{ $t('common.tokensEstimate', { tokens: formatTokens(contentTokens) }, contentTokens) }}</span>
      <button data-test="save-btn" class="px-5 py-2 text-[13px] font-medium bg-primary text-white rounded-[9px] hover:bg-primary-strong transition-colors" @click="emit('save')">{{ $t('common.save') }}</button>
    </div>

    <FullscreenEditor :model-value="definition.content" :open="fullscreenOpen" @close="fullscreenOpen = false" @update:model-value="emit('update:content', $event)" />
    </template>
  </div>
</template>
