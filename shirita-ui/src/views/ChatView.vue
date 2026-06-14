<script setup lang="ts">
import { onMounted, watch, computed } from 'vue'
import { useRoute } from 'vue-router'
import { useChatStore } from '../stores/chat'
import { useUiStore } from '../stores/ui'
import { estimateTokens, formatTokens } from '../utils/tokens'
import MessageList from '../components/MessageList.vue'
import Composer from '../components/Composer.vue'
import { ArrowLeft } from 'lucide-vue-next'

const route = useRoute()
const chat = useChatStore()
const ui = useUiStore()

const sessionId = route.params.id as string

// Rough running total of the transcript, for context budgeting.
const convoTokens = computed(() =>
  chat.messages.reduce((sum, m) => sum + estimateTokens(m.raw_content), 0),
)

onMounted(() => {
  chat.loadMessages(sessionId)
})

watch(
  () => route.params.id,
  (newId) => {
    if (newId && newId !== sessionId) {
      chat.loadMessages(newId as string)
    }
  },
)

function handleSend(text: string) {
  chat.send(sessionId, text)
}

function handleCopy(text: string) {
  navigator.clipboard.writeText(text).catch(() => {})
}

async function handleRegenerate() {
  const lastUser = [...chat.messages].reverse().find((m) => m.role === 'user')
  if (lastUser) {
    await chat.send(sessionId, lastUser.raw_content)
  }
}
</script>

<template>
  <div class="flex flex-col h-full max-w-[600px] mx-auto">
    <div class="flex items-center gap-2 px-5 pt-4 pb-2 min-w-0">
      <router-link to="/" class="text-muted hover:text-ink shrink-0" aria-label="Back"><ArrowLeft :size="18" /></router-link>
      <span class="font-semibold text-ink truncate">Chat</span>
      <span v-if="chat.messages.length" class="ml-auto text-[11.5px] text-muted tabular-nums shrink-0">~{{ formatTokens(convoTokens) }} tokens</span>
    </div>

    <p v-if="chat.error" class="text-coral text-sm px-5 py-4">{{ chat.error }}</p>
    <p v-else-if="chat.loading && chat.messages.length === 0" class="text-muted text-sm px-5 pt-12 text-center">Loading…</p>

    <MessageList
      v-else
      :messages="chat.messages"
      :style="ui.messageStyle"
      :is-streaming="chat.isStreaming"
      :streaming-text="chat.streamingText"
      :streaming-error="chat.streamingError"
      @copy="handleCopy"
      @regenerate="handleRegenerate"
    />

    <Composer :disabled="chat.isStreaming" @send="handleSend" />
  </div>
</template>
