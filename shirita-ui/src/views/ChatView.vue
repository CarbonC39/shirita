<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { useChatStore } from '../stores/chat'
import { useSettingsStore } from '../stores/settings'
import { useUiStore } from '../stores/ui'
import { estimateTokens } from '../utils/tokens'
import { siblings } from '../utils/tree'
import { getSessionState, getSessionIdentity, getSessionPanels, applyStateUpdates, assetUrl } from '../api/client'
import type { SessionState, Identity, SessionPanel, PanelAction } from '../api/types'
import MessageList from '../components/MessageList.vue'
import Composer from '../components/Composer.vue'
import ChatDetailsDrawer from '../components/ChatDetailsDrawer.vue'
import { ArrowLeft, Info, X } from 'lucide-vue-next'
import { useToast } from '../composables/useToast'

const { t } = useI18n()
const { show: showToast } = useToast()
const route = useRoute()
const router = useRouter()
const chat = useChatStore()
const ui = useUiStore()
const settings = useSettingsStore()

const sessionId = route.params.id as string
const showForkNotice = ref(route.query.forked === '1')

// Session-information drawer (panels + variables). Trigger only when there is
// something to show; focus returns to it when the drawer closes.
const detailsOpen = ref(false)
const detailsTrigger = ref<HTMLButtonElement | null>(null)
function openDetails() {
  detailsOpen.value = true
}
function closeDetails() {
  detailsOpen.value = false
  detailsTrigger.value?.focus()
}

// Rough running total of the active branch, for context budgeting.
const convoTokens = computed(() =>
  chat.displayed.reduce((sum, m) => sum + estimateTokens(m.raw_content), 0),
)

// Active-branch variable state (system + custom), refreshed on load/send/swipe.
const sessionState = ref<SessionState>({ schema: [], values: {} })
async function loadState() {
  try {
    sessionState.value = await getSessionState(sessionId)
  } catch {
    sessionState.value = { schema: [], values: {} }
  }
}
const identity = ref<Identity>({ assistant: { name: null, avatar: null }, user: { name: null, avatar: null } })
async function loadIdentity() {
  try {
    const resolved = await getSessionIdentity(sessionId)
    // Merge the configured default identity as a fallback so messages
    // without a char/persona definition still show the user's chosen name/avatar
    // instead of the generic "You"/"Assistant". Settings win only when the per-session
    // side has no value of its own (a definition or pack overrides defaults).
    const defUserName = (settings.data['user.name'] as string) || null
    const defUserAvatar = (settings.data['user.avatar'] as string) || null
    const defAssistantName = (settings.data['assistant.name'] as string) || null
    const defAssistantAvatar = (settings.data['assistant.avatar'] as string) || null
    identity.value = {
      assistant: {
        name: resolved.assistant.name ?? defAssistantName,
        avatar: resolved.assistant.avatar ?? defAssistantAvatar,
      },
      user: {
        name: resolved.user.name ?? defUserName,
        avatar: resolved.user.avatar ?? defUserAvatar,
      },
    }
  } catch {
    /* keep fallback */
  }
}
// Server-resolved panels for the session (one per `panel` folder), in resolution order.
const panels = ref<SessionPanel[]>([])
// Filters out panels whose author set a min_messages threshold the chat
// hasn't reached yet.
const visiblePanels = computed(() =>
  panels.value.filter((p) => chat.messages.length >= (p.min_messages ?? 0)),
)
// Details trigger shows only when there is panel or variable content to show.
const hasDetails = computed(() => visiblePanels.value.length > 0 || sessionState.value.schema.length > 0)
async function loadPanels() {
  try {
    panels.value = await getSessionPanels(sessionId)
  } catch {
    panels.value = []
  }
}

async function onPanelAction(panel: SessionPanel, action: PanelAction) {
  const caps = panel.caps || {}
  if (action.kind === 'diff') {
    if (!caps.write) return
    try {
      const res = await applyStateUpdates(sessionId, [{ action: action.op, key: action.key, value: action.value }])
      sessionState.value = { ...sessionState.value, values: res.values }
    } catch { /* stay on last good state */ }
  } else if (action.kind === 'insert') {
    if (caps.insert) composerRef.value?.setText(action.text)
  } else if (action.kind === 'send') {
    if (caps.send) await handleSend(action.text, [])
  }
}

const effectiveIdentity = computed<Identity>(() => {
  const v = sessionState.value.values
  const dyn = (k: string) => (typeof v[k] === 'string' && v[k] ? (v[k] as string) : null)
  return {
    assistant: {
      name: dyn('$assistant_name') ?? identity.value.assistant.name,
      avatar: dyn('$avatar') ?? identity.value.assistant.avatar,
    },
    user: identity.value.user,
  }
})
const headerName = computed(() => effectiveIdentity.value.assistant.name || t('chat.title'))
const avatar = computed(() => {
  const a = effectiveIdentity.value.assistant.avatar
  return a ? assetUrl(a) : ''
})
const bg = computed(() => {
  const v = sessionState.value.values['$background']
  return typeof v === 'string' && v ? assetUrl(v) : ''
})
const bgStyle = computed(() => (bg.value ? { backgroundImage: `url(${bg.value})` } : {}))

onMounted(() => {
  // Start the transcript, state, identity, and panel loads in parallel — the
  // reactive view renders loading/error states without waiting on any of them,
  // so a slow transcript request must not stall the other independent fetches.
  void chat.loadMessages(sessionId)
  void loadState()
  // Identity resolution depends on the configured default identity in settings,
  // so chain those two — but they must not wait on the transcript request.
  const resolveIdentity = () => loadIdentity()
  if (Object.keys(settings.data).length === 0) {
    settings.load().then(resolveIdentity).catch(() => {
      /* settings unavailable → identity falls back to defaults */
      resolveIdentity()
    })
  } else {
    resolveIdentity()
  }
  void loadPanels()
})

watch(
  () => route.params.id,
  (newId) => {
    if (newId && newId !== sessionId) {
      chat.loadMessages(newId as string)
    }
  },
)

// React to default-identity changes in settings so a name/avatar edit
// is reflected without a reload. Per-session definitions still win.
watch(
  () => [settings.data['user.name'], settings.data['user.avatar'], settings.data['assistant.name'], settings.data['assistant.avatar']],
  () => {
    if (sessionId) loadIdentity()
  },
)

const composerRef = ref<InstanceType<typeof Composer> | null>(null)

// Strip HTML tags and limit length for content injected from HTML cards.
function sanitizeCardContent(raw: string): string {
  return raw.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim().slice(0, 2000)
}

function onCardMessage(e: MessageEvent) {
  if (e.data?.type === 'shirita-add-input' && typeof e.data.content === 'string') {
    composerRef.value?.setText(sanitizeCardContent(e.data.content))
  }
}

onMounted(() => { window.addEventListener('message', onCardMessage) })
onUnmounted(() => {
  window.removeEventListener('message', onCardMessage)
  // Bug 9: abort any in-flight stream when leaving the chat, so a stale SSE
  // connection can't fire a late `done` reload into this now-unmounted view
  // (which surfaced as duplicate / "load failed" messages).
  chat.abortActive()
})

async function handleSend(text: string, attachments: string[]) {
  await chat.send(sessionId, text, attachments)
  await loadState()
}

async function handleStop() {
  await chat.stop()
  await loadState()
}

function handleCopy(text: string) {
  // Was a silent .catch(() => {}): if the browser blocked the copy (no HTTPS,
  // permissions) the user got neither confirmation nor an error. Surface both.
  navigator.clipboard.writeText(text)
    .then(() => showToast(t('chat.copied')))
    .catch(() => showToast(t('chat.copyFailed'), 'error'))
}

async function handleRegenerate(id: string) {
  await chat.regenerate(sessionId, id)
  await loadState()
}
function handleEditSave(id: string, text: string) {
  chat.editMsg(id, text)
}
function handleToggleHidden(id: string) {
  chat.toggleHidden(id)
}
async function handleSwipe(id: string, delta: -1 | 1) {
  const cur = chat.messages.find((m) => m.id === id)
  if (!cur) return
  const sibs = siblings(chat.messages, cur)
  const i = sibs.findIndex((s) => s.id === id)
  const target = sibs[i + delta]
  if (target) { await chat.switchLeaf(target.id); await loadState() }
}
async function handleFork(id: string) {
  const newId = await chat.fork(id)
  if (newId) router.push(`/chat/${newId}?forked=1`)
}
async function handleDelete(id: string) {
  if (!window.confirm(t('chat.deleteConfirm'))) return
  await chat.remove(id)
  await loadState()
}
</script>

<template>
  <div class="app-chat-column flex flex-col h-full min-h-0">
    <!-- compact chat bar -->
    <div data-test="chat-bar" class="app-chat-bar flex items-center gap-2 px-3 sm:px-5 pt-2 pb-2 min-w-0 shrink-0">
      <router-link to="/" class="text-muted hover:text-ink shrink-0" :aria-label="$t('chat.back')"><ArrowLeft :size="18" /></router-link>
      <img v-if="avatar" :src="avatar" class="w-6 h-6 rounded-full object-cover shrink-0" alt="" />
      <span class="font-semibold text-ink truncate">{{ headerName }}</span>
      <button
        v-if="hasDetails"
        ref="detailsTrigger"
        data-test="details-trigger"
        class="ml-auto shrink-0 text-muted hover:text-ink p-1 -m-1"
        :aria-label="$t('chat.details')"
        :title="$t('chat.details')"
        @click="openDetails"
      >
        <Info :size="18" :stroke-width="1.8" />
      </button>
    </div>

    <!-- transient fork notice (space only while present) -->
    <div v-if="showForkNotice" class="shrink-0 flex items-center justify-between bg-primary/10 border border-primary/30 rounded-lg px-3 py-1.5 mx-3 sm:mx-5 mb-1 text-[13px] text-ink">
      <span>{{ $t('chat.forkNotice') }}</span>
      <button class="text-muted hover:text-ink" :aria-label="$t('common.close')" @click="showForkNotice = false"><X :size="14" /></button>
    </div>

    <!-- transcript: the only flexible, vertically scrolling region -->
    <div data-test="transcript-region" class="app-transcript-region flex-1 min-h-0 flex flex-col">
      <!-- Initial load failure: nothing cached to fall back on, offer a retry. -->
      <div v-if="chat.error && chat.messages.length === 0" data-test="load-error" class="flex flex-col items-center gap-2 py-10 text-center">
        <p class="text-coral text-sm">{{ chat.error }}</p>
        <button class="btn" data-test="retry-load" @click="chat.retryLoad()">{{ $t('chat.retry') }}</button>
      </div>
      <p v-else-if="chat.loading && chat.messages.length === 0" class="text-muted text-sm pt-12 text-center">{{ $t('common.loading') }}</p>

      <template v-else>
        <div v-if="chat.generationUsage || (chat.isStreaming && (chat.agentStatus || chat.agentActivity))" data-test="agent-activity" class="shrink-0 px-3 sm:px-5 pb-1 text-[12px] text-muted" aria-live="polite">
          <span v-if="chat.agentStatus" class="text-ink">{{ chat.agentStatus }}</span>
          <span v-else-if="chat.agentActivity">{{ chat.agentActivity }}</span>
          <span v-if="chat.generationUsage" class="ml-2">{{ chat.generationUsage.input_tokens }} in / {{ chat.generationUsage.output_tokens }} out</span>
        </div>
        <!-- Refresh failure with a cached transcript: keep messages visible. -->
        <div v-if="chat.error" data-test="refresh-error" class="shrink-0 flex items-center justify-between gap-2 rounded-lg border border-coral/30 bg-coral/10 px-3 py-1.5 mx-3 sm:mx-5 mb-1 text-[13px] text-ink">
          <span>{{ chat.error }}</span>
          <button class="shrink-0 text-muted hover:text-ink" data-test="retry-refresh" @click="chat.retryLoad()">{{ $t('chat.retry') }}</button>
        </div>
        <MessageList
          :messages="chat.displayed"
          :all-messages="chat.messages"
          :style="ui.messageStyle"
          :is-streaming="chat.isStreaming"
          :streaming-text="chat.streamingText"
          :streaming-error="chat.streamingError"
          :identity="effectiveIdentity"
          :tokens="convoTokens"
          @copy="handleCopy"
          @regenerate="handleRegenerate"
          @fork="handleFork"
          @edit-save="handleEditSave"
          @toggle-hidden="handleToggleHidden"
          @delete="handleDelete"
          @swipe="handleSwipe"
          @dismiss-streaming-error="chat.clearStreamingError()"
        />
      </template>
    </div>

    <!-- composer -->
    <div data-test="composer-region" class="app-composer-region shrink-0">
      <Composer ref="composerRef" :disabled="chat.isStreaming" :streaming="chat.isStreaming" @send="handleSend" @stop="handleStop" />
    </div>

    <ChatDetailsDrawer
      :open="detailsOpen"
      :session-id="sessionId"
      :panels="visiblePanels"
      :schema="sessionState.schema"
      :values="sessionState.values"
      @close="closeDetails"
      @panel-action="onPanelAction"
    />
  </div>
</template>
