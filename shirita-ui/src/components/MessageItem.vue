<script setup lang="ts">
import { computed } from 'vue'
import { Copy, RefreshCw, GitFork, Pencil, ChevronLeft, ChevronRight } from 'lucide-vue-next'
import type { Message } from '../api/types'

const props = defineProps<{
  message: Message
  style: 'bubble' | 'flat'
  isStreaming?: boolean
}>()

const emit = defineEmits<{
  copy: [text: string]
  regenerate: []
  fork: []
  edit: []
}>()

const isAssistant = computed(() => props.message.role === 'assistant')
const isUser = computed(() => props.message.role === 'user')
const label = computed(() => (props.message.role === 'assistant' ? 'Assistant' : 'You'))
</script>

<template>
  <!-- Bubble mode -->
  <div
    v-if="style === 'bubble'"
    data-test="msg-row"
    :class="['flex gap-2.5 mb-4', isUser ? 'justify-end' : 'justify-start']"
  >
    <div v-if="isAssistant" data-test="assistant-avatar" class="w-8 h-8 rounded-full bg-sky/40 shrink-0 mt-0.5" />
    <div :class="['max-w-[78%]', isUser ? 'order-first' : '']">
      <div
        :class="[
          'px-3.5 py-2.5 text-[15px] leading-relaxed whitespace-pre-wrap',
          isUser
            ? 'bg-coral text-[#1b1b1b] rounded-[16px] rounded-br-[4px]'
            : 'bg-card border border-line text-ink rounded-[16px] rounded-bl-[4px]',
        ]"
      >
        {{ message.raw_content }}<span
          v-if="isStreaming"
          data-test="streaming-cursor"
          class="inline-block w-[7px] h-[15px] bg-primary align-[-3px] ml-0.5 rounded-[1px] animate-pulse"
        />
      </div>

      <div
        v-if="isAssistant"
        data-test="message-actions"
        class="flex items-center gap-1.5 mt-1.5 ml-1 text-muted"
      >
        <span data-test="swipe-indicator" class="flex items-center gap-1 text-[12px]">
          <ChevronLeft :size="14" :stroke-width="2.2" />
          <span>1/1</span>
          <ChevronRight :size="14" :stroke-width="2.2" />
        </span>
        <span class="w-px h-3.5 bg-line" />
        <button data-test="regenerate-btn" class="hover:text-ink" title="Regenerate" @click="emit('regenerate')">
          <RefreshCw :size="15" :stroke-width="1.8" />
        </button>
        <button class="hover:text-ink" title="Fork" @click="emit('fork')">
          <GitFork :size="15" :stroke-width="1.8" />
        </button>
        <button data-test="copy-btn" class="hover:text-ink" title="Copy" @click="emit('copy', message.raw_content)">
          <Copy :size="15" :stroke-width="1.8" />
        </button>
        <button data-test="edit-btn" class="hover:text-ink" title="Edit" @click="emit('edit')">
          <Pencil :size="15" :stroke-width="1.8" />
        </button>
      </div>
    </div>
  </div>

  <!-- Flat mode -->
  <div v-else data-test="msg-row" class="px-1 py-3.5 border-b border-line/70 last:border-b-0">
    <div class="flex items-center gap-2.5 mb-1.5">
      <div :class="['w-6 h-6 rounded-full shrink-0', isAssistant ? 'bg-sky/40' : 'bg-mauve/30']" />
      <span class="text-[13px] font-semibold text-ink">{{ label }}</span>
    </div>
    <div class="text-[15px] leading-relaxed whitespace-pre-wrap pl-[34px] text-ink">
      {{ message.raw_content }}<span
        v-if="isStreaming"
        data-test="streaming-cursor"
        class="inline-block w-[7px] h-[15px] bg-primary align-[-3px] ml-0.5 rounded-[1px] animate-pulse"
      />
    </div>
    <div v-if="isAssistant" data-test="message-actions" class="flex items-center gap-1.5 mt-2 pl-[34px] text-muted">
      <span data-test="swipe-indicator" class="flex items-center gap-1 text-[12px]">
        <ChevronLeft :size="14" :stroke-width="2.2" />
        <span>1/1</span>
        <ChevronRight :size="14" :stroke-width="2.2" />
      </span>
      <span class="w-px h-3.5 bg-line" />
      <button data-test="regenerate-btn" class="hover:text-ink" title="Regenerate" @click="emit('regenerate')">
        <RefreshCw :size="15" :stroke-width="1.8" />
      </button>
      <button class="hover:text-ink" title="Fork" @click="emit('fork')">
        <GitFork :size="15" :stroke-width="1.8" />
      </button>
      <button data-test="copy-btn" class="hover:text-ink" title="Copy" @click="emit('copy', message.raw_content)">
        <Copy :size="15" :stroke-width="1.8" />
      </button>
      <button data-test="edit-btn" class="hover:text-ink" title="Edit" @click="emit('edit')">
        <Pencil :size="15" :stroke-width="1.8" />
      </button>
    </div>
  </div>
</template>
