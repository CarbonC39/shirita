<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { Maximize2, X } from 'lucide-vue-next'
import type { Definition, DefType, VarDecl, VariablesMeta } from '../api/types'
import { triggerFromMeta } from '../api/types'
import { estimateTokens, formatTokens } from '../utils/tokens'
import FullscreenEditor from './FullscreenEditor.vue'
import TriggerEditor from './TriggerEditor.vue'
import ToggleSwitch from './ToggleSwitch.vue'
import AssetPicker from './AssetPicker.vue'
import PanelView from './PanelView.vue'
import VariablesEditor from './VariablesEditor.vue'

const props = withDefaults(
  defineProps<{
    definition: Definition
    types?: DefType[]
    active?: boolean
    hideHeading?: boolean
    savedTick?: number
  }>(),
  { types: () => [], active: false, hideHeading: false, savedTick: 0 },
)

const emit = defineEmits<{
  'update:content': [content: string]
  'update:name': [name: string]
  'update:type': [type: string]
  'update:meta': [meta: Record<string, unknown>]
  save: []
  'create-type': [name: string]
  'delete-type': [id: string]
}>()

// ── saved indicator ──
const showSaved = ref(false)
let savedTimer: ReturnType<typeof setTimeout> | undefined
function flashSaved() {
  showSaved.value = true
  if (savedTimer) clearTimeout(savedTimer)
  savedTimer = setTimeout(() => { showSaved.value = false }, 1500)
}
watch(() => props.savedTick, (t) => { if (t) flashSaved() })

const fullscreenOpen = ref(false)

const contentTokens = computed(() => estimateTokens(props.definition.content))

// ── type chips ──
const typeChips = computed<DefType[]>(() => {
  const builtins: DefType[] = [
    { id: 'char', label: 'Character', sort: 0, builtin: true, created_at: '' },
    { id: 'persona', label: 'Persona', sort: 1, builtin: true, created_at: '' },
    { id: 'world', label: 'World', sort: 2, builtin: true, created_at: '' },
    { id: 'prompt', label: 'Prompt', sort: 3, builtin: true, created_at: '' },
    { id: 'first_message', label: 'Message', sort: 4, builtin: true, created_at: '' },
    { id: 'html', label: 'HTML', sort: 5, builtin: true, created_at: '' },
    { id: 'css', label: 'CSS', sort: 6, builtin: true, created_at: '' },
    { id: 'variables', label: 'Variables', sort: 7, builtin: true, created_at: '' },
  ]
  const custom = (props.types ?? []).filter((t) => !builtins.some((b) => b.id === t.id))
  return [...builtins, ...custom]
})

const isContainerType = computed(() =>
  !['prompt', 'regex_rule', 'tool', 'first_message', 'html', 'css', 'variables'].includes(props.definition.type),
)

const triggerMode = computed(() => {
  const t = triggerFromMeta(props.definition.meta)
  return t.mode === 'keyword' ? 'keyword' : 'always'
})

function scan(): { depth: number; recursive: boolean } {
  const m = props.definition.meta as Record<string, unknown> | undefined
  const s = m?.scan as Record<string, unknown> | undefined
  return {
    depth: typeof s?.depth === 'number' ? s.depth : 4,
    recursive: s?.recursive !== false,
  }
}
function updateScan(s: { depth?: number; recursive?: boolean }) {
  const cur = scan()
  const next: Record<string, unknown> = { depth: s.depth ?? cur.depth, recursive: s.recursive ?? cur.recursive }
  emit('update:meta', { ...props.definition.meta, scan: next })
}

const showWrapInTag = computed(() =>
  ['char', 'persona', 'world', 'variables'].includes(props.definition.type),
)

// ── custom type creation ──
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

// ── variables brick ──
const decls = computed<VarDecl[]>(() => (props.definition.meta as unknown as VariablesMeta).decls ?? [])
function saveDecls(next: VarDecl[]) {
  emit('update:meta', { ...props.definition.meta, decls: next })
}

// ── Ctrl+S ──
function onKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key === 's' && props.active) {
    e.preventDefault()
    emit('save')
  }
}
onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div>
    <h3 v-if="!hideHeading" class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2.5 px-0.5">{{ $t('definition.heading') }}</h3>

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

      <!-- persona avatar -->
      <div v-if="definition.type === 'persona'" data-test="persona-avatar" class="mb-3">
        <label class="text-[12px] text-muted block mb-1.5">{{ $t('definition.avatar') }}</label>
        <AssetPicker
          shape="circle"
          kind="avatar"
          :model-value="(definition.meta as any).avatar || ''"
          @update:model-value="emit('update:meta', { ...definition.meta, avatar: $event })"
        />
      </div>

      <!-- first_message greeting -->
      <div v-if="definition.type === 'first_message'" data-test="message-type-fields" class="mb-3 space-y-2">
        <p class="text-[12px] text-muted">{{ $t('definition.messageTypeHint') }}</p>
      </div>

      <!-- world-book trigger + scan settings -->
      <div v-if="isContainerType" class="mb-3 space-y-2.5">
        <TriggerEditor
          :model-value="triggerFromMeta(definition.meta)"
          @update:model-value="emit('update:meta', { ...definition.meta, trigger: $event })"
        />
        <div class="flex items-center gap-4 flex-wrap">
          <template v-if="triggerMode === 'keyword'">
            <label class="flex items-center gap-2 text-[13px] text-ink">
              {{ $t('definition.scanDepth') }}
              <input
                data-test="scan-depth"
                :value="scan().depth"
                type="number" min="1" max="20"
                class="field !py-1 w-[64px] text-right tabular-nums"
                @input="updateScan({ depth: parseInt(($event.target as HTMLInputElement).value) || 1 })"
              />
            </label>
            <label class="flex items-center gap-2 text-[13px] text-ink">
              {{ $t('definition.recursive') }}
              <ToggleSwitch :model-value="scan().recursive" @update:model-value="updateScan({ recursive: $event })" />
            </label>
          </template>
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
      <div v-if="definition.type !== 'variables'" class="relative">
        <textarea
          :value="definition.content"
          rows="5"
          class="w-full border border-line rounded-[9px] bg-card px-3 py-2.5 pr-9 text-[13px] leading-relaxed text-ink/75 resize-y outline-none focus:border-primary/50 font-mono"
          :placeholder="$t('definition.contentPlaceholder')"
          @input="emit('update:content', ($event.target as HTMLTextAreaElement).value)"
        />
        <button data-test="fullscreen-btn" class="absolute top-2 right-2 p-1 text-muted/70 hover:text-ink" :title="$t('settings.fullscreen')" @click="fullscreenOpen = true"><Maximize2 :size="15" /></button>
      </div>

      <!-- variables brick -->
      <div v-if="definition.type === 'variables'" data-test="variables-editor" class="mt-1">
        <span class="text-[12px] text-muted block mb-1">{{ $t('definition.variablesDecls') }}</span>
        <VariablesEditor :model-value="decls" @update:model-value="saveDecls" />
      </div>

      <!-- html preview -->
      <div v-if="definition.type === 'html'" data-test="html-preview" class="mt-3">
        <span class="text-[12px] text-muted block mb-1">{{ $t('definition.htmlPreview') }}</span>
        <PanelView :html="definition.content" :css="''" :values="{}" />
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
        <div class="flex items-center gap-2">
          <span v-if="showSaved" class="text-[11.5px] text-emerald">{{ $t('common.saved') }}</span>
          <button data-test="save-btn" class="px-5 py-2 text-[13px] font-medium bg-primary text-white rounded-[9px] hover:bg-primary-strong transition-colors" @click="emit('save')">{{ $t('common.save') }}</button>
        </div>
      </div>

      <FullscreenEditor :model-value="definition.content" :open="fullscreenOpen" @close="fullscreenOpen = false" @update:model-value="emit('update:content', $event)" />
    </template>
  </div>
</template>
