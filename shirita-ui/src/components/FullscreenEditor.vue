<script setup lang="ts">
import { X } from 'lucide-vue-next'

defineProps<{ modelValue: string; open: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: string]; close: [] }>()

function onInput(e: Event) { emit('update:modelValue', (e.target as HTMLTextAreaElement).value) }
function onKeydown(e: KeyboardEvent) { if (e.key === 'Escape') emit('close') }
</script>

<template>
  <Teleport to="body">
    <div v-if="open" data-test="overlay" class="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-6" @click.self="emit('close')">
      <div class="w-full max-w-3xl h-[85vh] bg-card rounded-2xl shadow-2xl flex flex-col overflow-hidden">
        <div class="flex items-center justify-between px-5 py-3 border-b border-line">
          <span class="text-[13px] text-muted">Fullscreen editor</span>
          <button class="text-muted hover:text-ink" @click="emit('close')"><X :size="18" /></button>
        </div>
        <textarea :value="modelValue" class="flex-1 w-full resize-none p-5 text-[15px] leading-relaxed font-mono bg-card outline-none" placeholder="Start typing…" @input="onInput" @keydown="onKeydown" />
      </div>
    </div>
  </Teleport>
</template>
