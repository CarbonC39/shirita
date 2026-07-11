<script setup lang="ts">
import { ref, watch, nextTick, onBeforeUnmount } from 'vue'
import { X } from 'lucide-vue-next'

const props = defineProps<{ modelValue: string; open: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: string]; close: [] }>()

const textareaEl = ref<HTMLTextAreaElement | null>(null)
const closeBtnEl = ref<HTMLButtonElement | null>(null)
// Element that had focus before the dialog opened, so we can return to it.
let previouslyFocused: HTMLElement | null = null

function onInput(e: Event) { emit('update:modelValue', (e.target as HTMLTextAreaElement).value) }

// Trap Tab between the two focusable controls and let Escape close from
// anywhere (not only when the textarea itself is focused).
function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') { e.preventDefault(); emit('close'); return }
  if (e.key !== 'Tab') return
  const focusables = [closeBtnEl.value, textareaEl.value].filter(Boolean) as HTMLElement[]
  if (focusables.length < 2) return
  const [first, last] = focusables
  if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus() }
  else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus() }
}

watch(() => props.open, (isOpen) => {
  if (isOpen) {
    previouslyFocused = document.activeElement as HTMLElement
    nextTick(() => textareaEl.value?.focus())
  } else if (previouslyFocused) {
    previouslyFocused.focus()
    previouslyFocused = null
  }
})
// If the component unmounts while open, still return focus.
onBeforeUnmount(() => { if (props.open && previouslyFocused) previouslyFocused.focus() })
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      data-test="overlay"
      role="dialog"
      aria-modal="true"
      :aria-label="$t('common.fullscreenEditor')"
      class="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-6"
      @click.self="emit('close')"
      @keydown="onKeydown"
    >
      <div class="w-full max-w-3xl h-[85vh] bg-card rounded-2xl shadow-2xl flex flex-col overflow-hidden">
        <div class="flex items-center justify-between px-5 py-3 border-b border-line">
          <span class="text-[13px] text-muted">{{ $t('common.fullscreenEditor') }}</span>
          <button ref="closeBtnEl" class="text-muted hover:text-ink" :aria-label="$t('common.close')" @click="emit('close')"><X :size="18" /></button>
        </div>
        <textarea ref="textareaEl" :value="modelValue" class="flex-1 w-full resize-none p-5 text-[15px] leading-relaxed font-mono bg-card outline-none" :placeholder="$t('common.startTyping')" @input="onInput" />
      </div>
    </div>
  </Teleport>
</template>
