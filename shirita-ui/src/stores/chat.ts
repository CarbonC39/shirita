import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { Message } from '../api/types'
import {
  listMessages, getSession, sendMessage, regenerateMessage,
  editMessage, setActiveLeaf, forkSession,
} from '../api/client'
import { activePath } from '../utils/tree'

export const useChatStore = defineStore('chat', () => {
  const messages = ref<Message[]>([])
  const activeLeafId = ref<string | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)
  const isStreaming = ref(false)
  const streamingText = ref('')
  const streamingError = ref<string | null>(null)
  const activeSessionId = ref<string | null>(null)

  const displayed = computed(() => activePath(messages.value, activeLeafId.value))

  async function loadMessages(sessionId: string) {
    loading.value = true
    error.value = null
    activeSessionId.value = sessionId
    try {
      messages.value = await listMessages(sessionId)
      // The leaf merely chooses which branch is shown (falls back to newest),
      // so a session-read hiccup must not blank the transcript.
      try {
        activeLeafId.value = (await getSession(sessionId)).active_leaf_id ?? null
      } catch {
        activeLeafId.value = null
      }
    } catch (e) {
      error.value = (e as Error).message
    } finally {
      loading.value = false
    }
  }

  async function consume(
    stream: AsyncGenerator<{ type: string; text?: string; message?: string }>,
    sessionId: string,
  ) {
    isStreaming.value = true
    streamingText.value = ''
    streamingError.value = null
    try {
      for await (const event of stream) {
        if (event.type === 'delta') streamingText.value += event.text
        else if (event.type === 'done') { streamingText.value = ''; await loadMessages(sessionId) }
        else if (event.type === 'error') { streamingError.value = event.message ?? null; isStreaming.value = false; return }
      }
    } catch (e) {
      streamingError.value = (e as Error).message
    } finally {
      isStreaming.value = false
    }
  }

  // Show the user's turn the instant it's sent rather than waiting for the
  // assistant's reply + a reload: append a local placeholder under the
  // current leaf and point the leaf at it, then let the eventual `done`
  // reload (or the rollback below on error) reconcile with the server.
  function makeOptimisticUserMessage(sessionId: string, parentId: string | null, text: string, attachments: string[]): Message {
    return {
      id: `__pending-${Date.now()}__`,
      session_id: sessionId,
      parent_id: parentId,
      role: 'user',
      raw_content: text,
      display_content: null,
      is_hidden: false,
      is_anchor: false,
      attachments,
      snapshot_state: {},
      created_at: new Date().toISOString(),
    }
  }

  async function send(sessionId: string, text: string, attachments: string[] = []) {
    const prevLeaf = activeLeafId.value
    const optimistic = makeOptimisticUserMessage(sessionId, prevLeaf, text, attachments)
    messages.value = [...messages.value, optimistic]
    activeLeafId.value = optimistic.id
    await consume(sendMessage(sessionId, text, attachments), sessionId)
    // A successful turn replaces `messages` wholesale via the `done` reload;
    // if we still see the placeholder, the stream errored before that happened.
    if (streamingError.value && messages.value.some((m) => m.id === optimistic.id)) {
      messages.value = messages.value.filter((m) => m.id !== optimistic.id)
      activeLeafId.value = prevLeaf
    }
  }
  async function regenerate(sessionId: string, msgId: string) {
    await consume(regenerateMessage(sessionId, msgId), sessionId)
  }
  async function switchLeaf(messageId: string) {
    if (!activeSessionId.value) return
    const s = await setActiveLeaf(activeSessionId.value, messageId)
    activeLeafId.value = s.active_leaf_id ?? null
  }
  async function editMsg(msgId: string, content: string) {
    if (!activeSessionId.value) return
    const updated = await editMessage(activeSessionId.value, msgId, { content })
    const i = messages.value.findIndex((m) => m.id === msgId)
    if (i !== -1) messages.value = [...messages.value.slice(0, i), updated, ...messages.value.slice(i + 1)]
  }
  async function toggleHidden(msgId: string) {
    if (!activeSessionId.value) return
    const m = messages.value.find((x) => x.id === msgId)
    if (!m) return
    const updated = await editMessage(activeSessionId.value, msgId, { is_hidden: !m.is_hidden })
    const i = messages.value.findIndex((x) => x.id === msgId)
    if (i !== -1) messages.value = [...messages.value.slice(0, i), updated, ...messages.value.slice(i + 1)]
  }
  async function fork(msgId: string): Promise<string | null> {
    if (!activeSessionId.value) return null
    const s = await forkSession(activeSessionId.value, msgId)
    return s.id
  }

  function clearStreaming() {
    isStreaming.value = false
    streamingText.value = ''
    streamingError.value = null
  }

  return {
    messages, activeLeafId, displayed, loading, error,
    isStreaming, streamingText, streamingError, activeSessionId,
    loadMessages, send, regenerate, switchLeaf, editMsg, toggleHidden, fork, clearStreaming,
  }
})
