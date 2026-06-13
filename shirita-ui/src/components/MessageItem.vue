<script setup lang="ts">
import { computed } from 'vue'
import { Copy, RefreshCw, GitFork, ChevronLeft, ChevronRight } from 'lucide-vue-next'
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
}>()

const isAssistant = computed(() => props.message.role === 'assistant')
const isUser = computed(() => props.message.role === 'user')
const label = computed(() =>
  props.message.role === 'assistant' ? 'Assistant' : 'User',
)
</script>

<template>
  <!-- Bubble mode -->
  <div
    v-if="style === 'bubble'"
    data-test="msg-row"
    :class="['flex gap-2.5 mb-4', isUser ? 'justify-end' : 'justify-start']"
  >
    <div v-if="isAssistant" data-test="assistant-avatar" class="w-8 h-8 rounded-full bg-sky/30 shrink-0 mt-1" />
    <div :class="['max-w-[75%]', isUser ? 'order-first' : '']">
      <div
        :class="[
          'px-4 py-2.5 rounded-2xl text-[15px] leading-relaxed whitespace-pre-wrap',
          isUser
            ? 'bg-coral/20 text-ink rounded-br-md'
            : 'bg-white border border-line rounded-bl-md',
        ]"
      >
        {{ message.raw_content }}
        <span
          v-if="isStreaming"
          data-test="streaming-cursor"
          class="inline-block w-1.5 h-4 bg-primary animate-pulse ml-0.5 align-text-bottom"
        />
      </div>
      <div
        v-if="isAssistant"
        data-test="message-actions"
        class="flex items-center gap-1 mt-1.5 ml-1 text-[12px] text-muted"
      >
        <span data-test="swipe-indicator" class="flex items-center gap-0.5">
          <ChevronLeft :size="12" />
          <span>1/1</span>
          <ChevronRight :size="12" />
        </span>
        <button data-test="copy-btn" class="hover:text-ink px-1" title="Copy" @click="emit('copy', message.raw_content)">
          <Copy :size="13" />
        </button>
        <button data-test="regenerate-btn" class="hover:text-ink px-1" title="Regenerate" @click="emit('regenerate')">
          <RefreshCw :size="13" />
        </button>
        <button class="hover:text-ink px-1" title="Fork" @click="emit('fork')">
          <GitFork :size="13" />
        </button>
      </div>
    </div>
    <div v-if="isUser" class="w-8 h-8 rounded-full bg-coral/20 shrink-0 mt-1" />
  </div>

  <!-- Flat mode -->
  <div v-else data-test="msg-row" class="mb-4">
    <div class="flex items-center gap-2 mb-1">
      <div :class="['w-6 h-6 rounded-full shrink-0', isAssistant ? 'bg-sky/30' : 'bg-coral/20']" />
      <span class="text-[13px] font-semibold text-muted capitalize">{{ label }}</span>
    </div>
    <div class="text-[15px] leading-relaxed whitespace-pre-wrap pl-8">
      {{ message.raw_content }}
      <span v-if="isStreaming" data-test="streaming-cursor" class="inline-block w-1.5 h-4 bg-primary animate-pulse ml-0.5 align-text-bottom" />
    </div>
    <div v-if="isAssistant" data-test="message-actions" class="flex items-center gap-1 mt-1.5 pl-8 text-[12px] text-muted">
      <span data-test="swipe-indicator" class="flex items-center gap-0.5">
        <ChevronLeft :size="12" />
        <span>1/1</span>
        <ChevronRight :size="12" />
      </span>
      <button data-test="copy-btn" class="hover:text-ink px-1" title="Copy" @click="emit('copy', message.raw_content)">
        <Copy :size="13" />
      </button>
      <button data-test="regenerate-btn" class="hover:text-ink px-1" title="Regenerate" @click="emit('regenerate')">
        <RefreshCw :size="13" />
      </button>
    </div>
    <div class="border-t border-line mt-3" />
  </div>
</template>
