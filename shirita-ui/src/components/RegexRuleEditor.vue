<script setup lang="ts">
import { ref } from 'vue'
import { ChevronDown, Trash2 } from 'lucide-vue-next'
import type { RegexRule } from '../api/types'
import ToggleSwitch from './ToggleSwitch.vue'

defineProps<{ rule: RegexRule }>()
const emit = defineEmits<{
  'update:enabled': [enabled: boolean]; 'update:name': [name: string]
  'update:pattern': [pattern: string]; 'update:replacement': [replacement: string]
  'update:scope': [scope: RegexRule['scope']]; delete: []
}>()

const expanded = ref(false)
function toggleExpand() { expanded.value = !expanded.value }
</script>

<template>
  <div class="border border-line rounded-lg bg-card mb-2">
    <div class="flex items-center gap-2.5 px-3 py-2.5">
      <ToggleSwitch :model-value="rule.enabled" @update:model-value="emit('update:enabled', $event)" />
      <span class="flex-1 text-[14px] truncate">{{ rule.name || $t('settings.regexUnnamed') }}</span>
      <button class="text-muted hover:text-ink shrink-0" @click="toggleExpand">
        <ChevronDown :size="16" :class="expanded ? '' : '-rotate-90'" class="transition-transform" />
      </button>
    </div>
    <div v-if="expanded" class="px-3 pb-3 border-t border-line pt-3 space-y-3">
      <div><label class="text-[11px] text-muted uppercase tracking-wide block mb-1">{{ $t('settings.regexName') }}</label>
        <input :value="rule.name" type="text" class="w-full border border-line rounded-md px-2.5 py-1.5 text-[13px] outline-none focus:border-primary/50" :placeholder="$t('settings.regexNamePlaceholder')" @input="emit('update:name', ($event.target as HTMLInputElement).value)" /></div>
      <div><label class="text-[11px] text-muted uppercase tracking-wide block mb-1">{{ $t('settings.regexFind') }}</label>
        <input :value="rule.pattern" type="text" class="w-full border border-line rounded-md px-2.5 py-1.5 text-[13px] font-mono outline-none focus:border-primary/50" :placeholder="$t('settings.regexPatternPlaceholder')" @input="emit('update:pattern', ($event.target as HTMLInputElement).value)" /></div>
      <div><label class="text-[11px] text-muted uppercase tracking-wide block mb-1">{{ $t('settings.regexReplace') }}</label>
        <input :value="rule.replacement" type="text" class="w-full border border-line rounded-md px-2.5 py-1.5 text-[13px] font-mono outline-none focus:border-primary/50" :placeholder="$t('settings.regexReplacementPlaceholder')" @input="emit('update:replacement', ($event.target as HTMLInputElement).value)" /></div>
      <div><label class="text-[11px] text-muted uppercase tracking-wide block mb-1.5">{{ $t('settings.regexApplyTo') }}</label>
        <div class="flex flex-wrap gap-3">
          <label class="flex items-center gap-1 text-[13px]"><input type="checkbox" :checked="rule.scope.ai_output" class="w-3 h-3 rounded accent-primary" @change="emit('update:scope', { ...rule.scope, ai_output: ($event.target as HTMLInputElement).checked })" /> {{ $t('settings.regexAiOutput') }}</label>
          <label class="flex items-center gap-1 text-[13px]"><input type="checkbox" :checked="rule.scope.user_input" class="w-3 h-3 rounded accent-primary" @change="emit('update:scope', { ...rule.scope, user_input: ($event.target as HTMLInputElement).checked })" /> {{ $t('settings.regexUserInput') }}</label>
          <label class="flex items-center gap-1 text-[13px]"><input type="checkbox" :checked="rule.scope.display_only" class="w-3 h-3 rounded accent-primary" @change="emit('update:scope', { ...rule.scope, display_only: ($event.target as HTMLInputElement).checked })" /> {{ $t('settings.regexDisplayOnly') }}</label>
        </div>
      </div>
      <button class="flex items-center gap-1 text-[12px] text-muted hover:text-coral" @click="emit('delete')"><Trash2 :size="13" /> {{ $t('settings.regexDelete') }}</button>
    </div>
  </div>
</template>
