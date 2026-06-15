<script setup lang="ts">
import { computed } from 'vue'
import type { Message } from '../api/types'
import { siblings } from '../utils/tree'
import MessageItem from './MessageItem.vue'

const props = defineProps<{
  messages: Message[]        // the active path (displayed)
  allMessages?: Message[]    // full set, for sibling counts (defaults to messages)
  style: 'bubble' | 'flat'
  isStreaming?: boolean
  streamingText?: string
  streamingError?: string | null
}>()

const emit = defineEmits<{
  copy: [text: string]
  regenerate: [id: string]
  fork: [id: string]
  'edit-save': [id: string, text: string]
  'toggle-hidden': [id: string]
  swipe: [id: string, delta: -1 | 1]
}>()

function sibInfo(msg: Message) {
  const sibs = siblings(props.allMessages ?? props.messages, msg)
  return { index: sibs.findIndex((s) => s.id === msg.id), count: sibs.length }
}

const streamingMsg = computed<Message | null>(() => {
  if (!props.isStreaming && !props.streamingText) return null
  return {
    id: '__streaming__',
    session_id: '',
    parent_id: null,
    role: 'assistant',
    raw_content: props.streamingText || '',
    display_content: null,
    is_hidden: false,
    snapshot_state: {},
    created_at: '',
  }
})
</script>

<template>
  <div class="flex-1 overflow-y-auto px-5 py-4">
    <p v-if="messages.length === 0 && !streamingMsg && !streamingError" class="text-muted text-sm text-center pt-12">
      No messages yet.
    </p>

    <MessageItem
      v-for="msg in messages"
      :key="msg.id"
      :message="msg"
      :style="style"
      :sibling-index="sibInfo(msg).index"
      :sibling-count="sibInfo(msg).count"
      @copy="emit('copy', $event)"
      @regenerate="emit('regenerate', msg.id)"
      @fork="emit('fork', msg.id)"
      @edit-save="(t) => emit('edit-save', msg.id, t)"
      @toggle-hidden="emit('toggle-hidden', msg.id)"
      @swipe="(d) => emit('swipe', msg.id, d)"
    />

    <MessageItem
      v-if="streamingMsg"
      :message="streamingMsg"
      :style="style"
      :is-streaming="true"
    />

    <p v-if="streamingError" class="text-coral text-sm text-center py-2">
      {{ streamingError }}
    </p>

    <div ref="bottom" />
  </div>
</template>
