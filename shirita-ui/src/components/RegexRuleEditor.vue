<script setup lang="ts">
import { nextTick, ref } from 'vue'
import { ChevronDown, Trash2, AlertTriangle } from 'lucide-vue-next'
import type { RegexRule } from '../api/types'
import ToggleSwitch from './ToggleSwitch.vue'

const props = defineProps<{
  rule: RegexRule
  scope: 'global' | 'template'
  sourceNames: string[]
  patternError: string | null
  open: boolean
}>()
const emit = defineEmits<{
  'update:enabled': [enabled: boolean]; 'update:name': [name: string]
  'update:pattern': [pattern: string]; 'update:replacement': [replacement: string]
  'update:scope': [scope: RegexRule['scope']]; toggleOpen: []; delete: []
}>()

// Renaming happens in place on the header label (double-click to start),
// rather than via a permanently-visible name field in the expanded panel.
const renaming = ref(false)
const nameDraft = ref('')
const nameInput = ref<HTMLInputElement>()
async function startRename() {
  nameDraft.value = props.rule.name
  renaming.value = true
  await nextTick()
  nameInput.value?.focus()
  nameInput.value?.select()
}
function commitRename() {
  renaming.value = false
  emit('update:name', nameDraft.value)
}
</script>

<template>
  <div
    :class="[
      'rounded-lg mb-1.5 border',
      scope === 'global'
        ? 'bg-primary/5 border-primary/20'
        : 'bg-surface/60 border-line',
    ]"
  >
    <div
      class="flex items-center gap-2.5 px-3 py-2"
      :class="{ 'opacity-50': !rule.enabled }"
    >
      <ToggleSwitch :model-value="rule.enabled" @update:model-value="emit('update:enabled', $event)" />
      <input
        v-if="renaming"
        ref="nameInput"
        v-model="nameDraft"
        data-test="regex-name-input"
        type="text"
        class="flex-1 min-w-0 border border-line rounded-md px-1.5 py-0.5 text-[14px] outline-none focus:border-primary/50"
        @keyup.enter="commitRename"
        @keyup.escape="renaming = false"
        @blur="commitRename"
      />
      <span
        v-else
        data-test="regex-name-label"
        class="text-[14px] truncate cursor-text shrink-0 max-w-[40%]"
        :title="$t('settings.regexRenameHint')"
        @dblclick="startRename"
      >{{ rule.name || $t('settings.regexUnnamed') }}</span>
      <span
        v-if="scope === 'template' && sourceNames.length"
        class="text-[11px] text-mauve/80 truncate"
        :title="sourceNames.join(', ')"
      >{{ sourceNames.join(', ') }}</span>
      <span class="flex-1" />
      <span class="text-[10px] text-muted/70 uppercase tracking-wide shrink-0 hidden sm:inline">
        <template v-if="rule.scope.ai_output">AI</template>
        <template v-if="rule.scope.ai_output && rule.scope.user_input">·</template>
        <template v-if="rule.scope.user_input">{{ $t('settings.regexUserShort') }}</template>
        · {{ $t('settings.regexPhase_' + rule.scope.phase) }}
      </span>
      <span
        v-if="patternError"
        class="flex items-center gap-0.5 text-[11px] text-coral shrink-0"
        :title="patternError"
      ><AlertTriangle :size="12" /> {{ $t('settings.regexInvalid') }}</span>
      <button class="text-muted hover:text-ink shrink-0" @click="emit('toggleOpen')">
        <ChevronDown :size="16" :class="open ? '' : '-rotate-90'" class="transition-transform" />
      </button>
    </div>
    <div v-if="open" class="px-3 pb-3 border-t border-line pt-3 space-y-3">
      <div><label class="text-[11px] text-muted uppercase tracking-wide block mb-1">{{ $t('settings.regexFind') }}</label>
        <input :value="rule.pattern" type="text" class="field w-full !py-1.5 text-[13px] font-mono" :placeholder="$t('settings.regexPatternPlaceholder')" @input="emit('update:pattern', ($event.target as HTMLInputElement).value)" /></div>
      <div><label class="text-[11px] text-muted uppercase tracking-wide block mb-1">{{ $t('settings.regexReplace') }}</label>
        <input :value="rule.replacement" type="text" class="field w-full !py-1.5 text-[13px] font-mono" :placeholder="$t('settings.regexReplacementPlaceholder')" @input="emit('update:replacement', ($event.target as HTMLInputElement).value)" /></div>
      <div><label class="text-[11px] text-muted uppercase tracking-wide block mb-1.5">{{ $t('settings.regexApplyTo') }}</label>
        <div class="flex flex-wrap gap-3 items-center">
          <label class="flex items-center gap-1 text-[13px]"><input type="checkbox" :checked="rule.scope.ai_output" class="w-3 h-3 rounded accent-primary" @change="emit('update:scope', { ...rule.scope, ai_output: ($event.target as HTMLInputElement).checked })" /> {{ $t('settings.regexAiOutput') }}</label>
          <label class="flex items-center gap-1 text-[13px]"><input type="checkbox" :checked="rule.scope.user_input" class="w-3 h-3 rounded accent-primary" @change="emit('update:scope', { ...rule.scope, user_input: ($event.target as HTMLInputElement).checked })" /> {{ $t('settings.regexUserInput') }}</label>
          <select
            :value="rule.scope.phase"
            class="field !py-1 text-[13px]"
            @change="emit('update:scope', { ...rule.scope, phase: ($event.target as HTMLSelectElement).value as 'display'|'both'|'prompt' })"
          >
            <option value="display">{{ $t('settings.regexPhaseDisplay') }}</option>
            <option value="both">{{ $t('settings.regexPhaseBoth') }}</option>
            <option value="prompt">{{ $t('settings.regexPhasePrompt') }}</option>
          </select>
        </div>
      </div>
      <button class="flex items-center gap-1 text-[12px] text-muted hover:text-coral" @click="emit('delete')"><Trash2 :size="13" /> {{ $t('settings.regexDelete') }}</button>
    </div>
  </div>
</template>
