import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Message } from '../api/types'
import { listMessages, sendMessage } from '../api/client'

export const useChatStore = defineStore('chat', () => {
  const messages = ref<Message[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const isStreaming = ref(false)
  const streamingText = ref('')
  const streamingError = ref<string | null>(null)
  const activeSessionId = ref<string | null>(null)

  async function loadMessages(sessionId: string) {
    loading.value = true
    error.value = null
    activeSessionId.value = sessionId
    try {
      messages.value = await listMessages(sessionId)
    } catch (e) {
      error.value = (e as Error).message
    } finally {
      loading.value = false
    }
  }

  async function send(sessionId: string, text: string) {
    isStreaming.value = true
    streamingText.value = ''
    streamingError.value = null

    try {
      const stream = sendMessage(sessionId, text)
      for await (const event of stream) {
        if (event.type === 'delta') {
          streamingText.value += event.text
        } else if (event.type === 'done') {
          streamingText.value = ''
          await loadMessages(sessionId)
        } else if (event.type === 'error') {
          streamingError.value = event.message
          isStreaming.value = false
          return
        }
      }
    } catch (e) {
      streamingError.value = (e as Error).message
    } finally {
      isStreaming.value = false
    }
  }

  function clearStreaming() {
    isStreaming.value = false
    streamingText.value = ''
    streamingError.value = null
  }

  return {
    messages, loading, error,
    isStreaming, streamingText, streamingError, activeSessionId,
    loadMessages, send, clearStreaming,
  }
})
