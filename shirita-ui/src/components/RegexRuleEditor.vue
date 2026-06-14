<script setup lang="ts">
import { ref } from 'vue'
import { ChevronDown, Trash2 } from 'lucide-vue-next'
import type { RegexRule } from '../api/types'
import ToggleSwitch from './ToggleSwitch.vue'

defineProps<{ rule: RegexRule }>()
const emit = defineEmits<{
  'update:enabled': [enabled: boolean]; 'update:pattern': [pattern: string]; 'update:replacement': [replacement: string]
  'update:scope': [scope: RegexRule['scope']]; delete: []
}>()

const expanded = ref(false)
function toggleExpand() { expanded.value = !expanded.value }
</script>

<template>
  <div class="border border-line rounded-lg bg-white mb-2">
    <div class="flex items-center gap-2.5 px-3 py-2.5">
      <ToggleSwitch :model-value="rule.enabled" @update:model-value="emit('update:enabled', $event)" />
      <span class="flex-1 text-[14px] truncate">{{ rule.name || 'Unnamed rule' }}</span>
      <button class="text-muted hover:text-ink shrink-0" @click="toggleExpand">
        <ChevronDown :size="16" :class="expanded ? '' : '-rotate-90'" class="transition-transform" />
      </button>
    </div>
    <div v-if="expanded" class="px-3 pb-3 border-t border-line pt-3 space-y-3">
      <div><label class="text-[11px] text-muted uppercase tracking-wide block mb-1">Find</label>
        <input :value="rule.pattern" type="text" class="w-full border border-line rounded-md px-2.5 py-1.5 text-[13px] font-mono outline-none focus:border-primary/50" placeholder="regex pattern" @input="emit('update:pattern', ($event.target as HTMLInputElement).value)" /></div>
      <div><label class="text-[11px] text-muted uppercase tracking-wide block mb-1">Replace</label>
        <input :value="rule.replacement" type="text" class="w-full border border-line rounded-md px-2.5 py-1.5 text-[13px] font-mono outline-none focus:border-primary/50" placeholder="replacement text" @input="emit('update:replacement', ($event.target as HTMLInputElement).value)" /></div>
      <div><label class="text-[11px] text-muted uppercase tracking-wide block mb-1.5">Apply to</label>
        <div class="flex flex-wrap gap-3">
          <label class="flex items-center gap-1 text-[13px]"><input type="checkbox" :checked="rule.scope.ai_output" class="w-3 h-3 rounded accent-primary" @change="emit('update:scope', { ...rule.scope, ai_output: ($event.target as HTMLInputElement).checked })" /> AI output</label>
          <label class="flex items-center gap-1 text-[13px]"><input type="checkbox" :checked="rule.scope.user_input" class="w-3 h-3 rounded accent-primary" @change="emit('update:scope', { ...rule.scope, user_input: ($event.target as HTMLInputElement).checked })" /> User input</label>
          <label class="flex items-center gap-1 text-[13px]"><input type="checkbox" :checked="rule.scope.display_only" class="w-3 h-3 rounded accent-primary" @change="emit('update:scope', { ...rule.scope, display_only: ($event.target as HTMLInputElement).checked })" /> Display only</label>
        </div>
      </div>
      <button class="flex items-center gap-1 text-[12px] text-muted hover:text-coral" @click="emit('delete')"><Trash2 :size="13" /> Delete rule</button>
    </div>
  </div>
</template>
