<script setup lang="ts">
import { onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useChatStore } from '../stores/chat'
import { useUiStore } from '../stores/ui'
import MessageList from '../components/MessageList.vue'
import Composer from '../components/Composer.vue'
import { ArrowLeft } from 'lucide-vue-next'

const route = useRoute()
const chat = useChatStore()
const ui = useUiStore()

const sessionId = route.params.id as string

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
    <div class="flex items-center justify-between px-5 pt-4 pb-2">
      <div class="flex items-center gap-2 min-w-0">
        <router-link to="/" class="text-muted hover:text-ink shrink-0" aria-label="Back"><ArrowLeft :size="18" /></router-link>
        <span class="font-semibold text-ink truncate">Chat</span>
      </div>
      <button
        class="text-[12.5px] text-muted hover:text-ink border border-line rounded-full px-3 py-1 shrink-0 transition-colors"
        :title="`Switch to ${ui.messageStyle === 'bubble' ? 'flat' : 'bubble'} style`"
        @click="ui.setMessageStyle(ui.messageStyle === 'bubble' ? 'flat' : 'bubble')"
      >
        {{ ui.messageStyle === 'bubble' ? 'Flat' : 'Bubble' }}
      </button>
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
