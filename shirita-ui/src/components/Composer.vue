<script setup lang="ts">
import { ref, computed } from 'vue'
import { ArrowUp, Plus } from 'lucide-vue-next'
import { estimateTokens, formatTokens } from '../utils/tokens'

const props = defineProps<{ disabled: boolean }>()

const emit = defineEmits<{
  send: [text: string]
}>()

const text = ref('')
const hasText = computed(() => text.value.trim().length > 0)
const draftTokens = computed(() => estimateTokens(text.value))

function submit() {
  const trimmed = text.value.trim()
  if (!trimmed || props.disabled) return
  emit('send', trimmed)
  text.value = ''
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    submit()
  }
}
</script>

<template>
  <div class="border-t border-line bg-card px-4 py-3">
    <div class="max-w-[600px] mx-auto flex items-end gap-2.5">
      <button type="button" class="text-muted hover:text-ink p-1.5 shrink-0 mb-0.5" :title="$t('composer.attach')">
        <Plus :size="20" />
      </button>
      <textarea
        v-model="text"
        :disabled="disabled"
        rows="1"
        :placeholder="$t('composer.placeholder')"
        class="flex-1 resize-none rounded-xl border border-line px-3.5 py-2.5 text-[15px] leading-relaxed
               focus:outline-none focus:border-primary/50 placeholder:text-muted/60
               disabled:bg-surface disabled:text-muted/50"
        @keydown="onKeydown"
      />
      <button
        data-test="send-btn"
        :disabled="disabled || !hasText"
        :class="[
          'w-10 h-10 rounded-full flex items-center justify-center shrink-0 transition-colors',
          hasText && !disabled ? 'bg-primary text-white' : 'bg-line text-muted',
        ]"
        @click="submit"
      >
        <ArrowUp :size="18" />
      </button>
    </div>
    <div v-if="hasText" class="max-w-[600px] mx-auto pl-[46px] pr-[50px] pt-1">
      <span class="text-[11px] text-muted tabular-nums">{{ $t('common.tokensEstimate', { tokens: formatTokens(draftTokens) }, draftTokens) }}</span>
    </div>
  </div>
</template>
