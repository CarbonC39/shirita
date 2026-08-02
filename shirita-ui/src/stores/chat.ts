import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { Message } from '../api/types'
import type { SseEvent } from '../api/client'
import {
  listMessages, getSession, sendMessage, regenerateMessage,
  editMessage, deleteMessage, setActiveLeaf, forkSession, abortSession,
} from '../api/client'
import { activePath } from '../utils/tree'
import { notifyReplyDone } from '../utils/notify'
import { useSettingsStore } from './settings'

export type AgentRunPhase = 'running' | 'finished' | 'stopped' | 'failed'
export interface AgentActivityEvent {
  kind: 'round' | 'tool' | 'workspace' | 'status'
  message: string
}
export interface AgentRunView {
  runId: string | null
  phase: AgentRunPhase
  round: number
  responseRevision: number
  events: AgentActivityEvent[]
}

/** Bounded transient event history per run; older events are collapsed away. */
export const MAX_AGENT_EVENTS = 50

export const useChatStore = defineStore('chat', () => {
  const messages = ref<Message[]>([])
  const activeLeafId = ref<string | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)
  const isStreaming = ref(false)
  const streamingText = ref('')
  const streamingError = ref<string | null>(null)
  const generationUsage = ref<{ input_tokens: number; output_tokens: number } | null>(null)
  const activeSessionId = ref<string | null>(null)

  // Run-scoped transient Agent view: rebuilt on every send/regenerate from
  // structured SSE events, never persisted, and never fed from localized strings.
  const agentRun = ref<AgentRunView>({ runId: null, phase: 'running', round: 0, responseRevision: 0, events: [] })
  function pushAgentEvent(kind: AgentActivityEvent['kind'], message: string) {
    agentRun.value.events = [...agentRun.value.events, { kind, message }].slice(-MAX_AGENT_EVENTS)
  }
  // Display derives from the structured run view; there is no second source of
  // truth. `agentStatus` is the most recent user-visible status line;
  // `agentActivity` is the latest tool/round/workspace line.
  const agentStatus = computed(() => {
    const events = agentRun.value.events
    for (let i = events.length - 1; i >= 0; i--) {
      if (events[i].kind === 'status') return events[i].message
    }
    return null
  })
  const agentActivity = computed(() => {
    const last = agentRun.value.events[agentRun.value.events.length - 1]
    return last ? last.message : null
  })
  // Track which message is being regenerated so we can hide it from the
  // active path while the new sibling streams in.
  const regeneratingMsgId = ref<string | null>(null)

  // Abort handle for the in-flight SSE stream. Set by send/regenerate, cleared
  // on settle. `stop()` asks the backend to persist partial text then aborts;
  // `abortActive()` (navigate-away) hard-aborts without surfacing an error.
  let activeAbort: AbortController | null = null

  function addUsage(inputTokens: number, outputTokens: number) {
    const previous = generationUsage.value
    generationUsage.value = {
      input_tokens: (previous?.input_tokens ?? 0) + inputTokens,
      output_tokens: (previous?.output_tokens ?? 0) + outputTokens,
    }
  }

  const displayed = computed(() =>
    activePath(messages.value.filter((m) => m.id !== regeneratingMsgId.value), activeLeafId.value),
  )

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
    stream: AsyncGenerator<SseEvent>,
    sessionId: string,
  ) {
    isStreaming.value = true
    streamingText.value = ''
    streamingError.value = null
    generationUsage.value = null
    agentRun.value = { runId: null, phase: 'running', round: 0, responseRevision: 0, events: [] }
    try {
      for await (const event of stream) {
        if (event.type === 'delta') streamingText.value += event.text
        else if (event.type === 'run_start') {
          agentRun.value.runId = event.run_id
          agentRun.value.phase = 'running'
          agentRun.value.round = 0
          agentRun.value.responseRevision = 0
          agentRun.value.events = []
        }
        else if (event.type === 'activity') {
          agentRun.value.round = event.round
          agentRun.value.phase = 'running'
          pushAgentEvent('round', `Round ${event.round}`)
        }
        else if (event.type === 'tool_start') {
          pushAgentEvent('tool', event.name)
        }
        else if (event.type === 'tool_result') {
          pushAgentEvent('tool', `${event.name}: ${event.status}`)
        }
        else if (event.type === 'workspace_mutation') {
          agentRun.value.responseRevision = event.revision
          pushAgentEvent('workspace', `response revised to revision ${event.revision}`)
        }
        else if (event.type === 'status') {
          pushAgentEvent('status', event.message)
        }
        else if (event.type === 'usage') addUsage(event.input_tokens, event.output_tokens)
        else if (event.type === 'finish') agentRun.value.phase = 'finished'
        else if (event.type === 'done') {
          // Success clears the activity surface deterministically; usage stays
          // observable. The run keeps its identity for the store but nothing is
          // rendered while phase === 'finished'.
          agentRun.value.phase = 'finished'
          agentRun.value.events = []
          streamingText.value = ''
          await loadMessages(sessionId)
          const s = useSettingsStore()
          if (s.data.notify_enabled) {
            const last = messages.value[messages.value.length - 1]
            notifyReplyDone(document.title || 'Shirita', (last?.display_content ?? last?.raw_content ?? '').slice(0, 120))
          }
        }
        else if (event.type === 'stopped') {
          // The user explicitly stopped generation; the backend persisted the
          // partial reply, so reload to reveal it — without flagging an error.
          agentRun.value.phase = 'stopped'
          streamingText.value = ''
          await loadMessages(sessionId)
        }
        else if (event.type === 'error') {
          agentRun.value.phase = 'failed'
          streamingError.value = event.message ?? null
          isStreaming.value = false
          return
        }
      }
    } catch (e) {
      // A client-side abort (Stop button / navigate-away) is expected and means
      // the partial text has been/will be persisted server-side — never show it
      // as a "load failed" error bubble.
      if ((e as Error).name === 'AbortError') {
        streamingText.value = ''
        streamingError.value = null
      } else {
        streamingError.value = (e as Error).message
      }
    } finally {
      isStreaming.value = false
      activeAbort = null
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

  /** Retry the last load (initial failure or a failed refresh) in place.
   *  Reloads `activeSessionId` through the normal load path so the view never
   *  navigates away or reloads the browser to recover. */
  async function retryLoad() {
    if (activeSessionId.value) await loadMessages(activeSessionId.value)
  }

  /** Dismiss a surfaced generation error without starting a new turn. */
  function clearStreamingError() {
    streamingError.value = null
  }

  async function send(sessionId: string, text: string, attachments: string[] = []) {
    // A stale error from a previous failed turn must not persist into this one.
    streamingError.value = null
    const prevLeaf = activeLeafId.value
    const optimistic = makeOptimisticUserMessage(sessionId, prevLeaf, text, attachments)
    messages.value = [...messages.value, optimistic]
    activeLeafId.value = optimistic.id
    activeAbort = new AbortController()
    await consume(sendMessage(sessionId, text, attachments, activeAbort.signal), sessionId)
    // A successful turn replaces `messages` wholesale via the `done` reload;
    // if we still see the placeholder, the stream errored before that happened.
    if (streamingError.value && messages.value.some((m) => m.id === optimistic.id)) {
      messages.value = messages.value.filter((m) => m.id !== optimistic.id)
      activeLeafId.value = prevLeaf
    }
  }
  async function regenerate(sessionId: string, msgId: string) {
    // Same contract as send: a dismissed/cleared error must not resurface.
    streamingError.value = null
    activeAbort = new AbortController()
    regeneratingMsgId.value = msgId
    try {
      await consume(regenerateMessage(sessionId, msgId, activeAbort.signal), sessionId)
    } finally {
      regeneratingMsgId.value = null
    }
  }

  // User-initiated Stop: ask the backend to end the stream and persist whatever
  // it has so far, then hard-abort the fetch in case the server is slow to yield.
  async function stop() {
    if (!activeSessionId.value) return
    const sid = activeSessionId.value
    await abortSession(sid)
    activeAbort?.abort()
    // The backend persists the partial reply when it honors the stop (ordinary
    // mode) or discards the run's workspace (Agent mode); the hard abort above
    // may discard the in-flight `stopped` event that would have reloaded the
    // transcript. Reload explicitly so the user sees the actual saved state.
    await loadMessages(sid)
  }

  // Unmount / navigate-away: hard-abort the fetch so a stale stream can't fire a
  // late `done` reload into the now-stale store (which left a duplicate / "load
  // failed" bubble). We do NOT call the abort endpoint — the server stream is
  // dropped when the client disconnects, and we don't want to persist a partial
  // reply the user never asked to keep.
  function abortActive() {
    activeAbort?.abort()
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
  // Delete a message and its subtree. The active leaf is the server's source of
  // truth, so we trust the returned activeLeafId (reset to the deleted root's
  // parent when the active branch was inside the subtree) rather than guessing
  // client-side. The subtree can be arbitrarily deep, so reload the message
  // list instead of filtering client-side (a shallow filter would leave orphan
  // grandchildren behind).
  async function remove(msgId: string) {
    if (!activeSessionId.value) return
    const { activeLeafId: newLeaf } = await deleteMessage(activeSessionId.value, msgId)
    activeLeafId.value = newLeaf
    await loadMessages(activeSessionId.value)
  }

  return {
    messages, activeLeafId, displayed, loading, error,
    isStreaming, streamingText, streamingError, agentActivity, agentStatus, generationUsage, agentRun, activeSessionId,
    loadMessages, retryLoad, clearStreamingError,
    send, regenerate, switchLeaf, editMsg, toggleHidden, fork, remove, stop, abortActive,
  }
})
