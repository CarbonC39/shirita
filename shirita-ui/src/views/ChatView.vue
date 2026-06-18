<script setup lang="ts">
import { onMounted, ref, watch, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { useChatStore } from '../stores/chat'
import { useUiStore } from '../stores/ui'
import { estimateTokens, formatTokens } from '../utils/tokens'
import { siblings } from '../utils/tree'
import { getSessionState, getSessionIdentity } from '../api/client'
import type { SessionState, Identity } from '../api/types'
import MessageList from '../components/MessageList.vue'
import Composer from '../components/Composer.vue'
import VariablesPanel from '../components/VariablesPanel.vue'
import { ArrowLeft } from 'lucide-vue-next'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const chat = useChatStore()
const ui = useUiStore()

const sessionId = route.params.id as string

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
    identity.value = await getSessionIdentity(sessionId)
  } catch {
    /* keep fallback */
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
  return a ? `/assets/${a}` : ''
})
const bg = computed(() => {
  const v = sessionState.value.values['$background']
  return typeof v === 'string' && v ? `/assets/${v}` : ''
})
const bgStyle = computed(() => (bg.value ? { backgroundImage: `url(${bg.value})` } : {}))

onMounted(() => {
  chat.loadMessages(sessionId)
  loadState()
  loadIdentity()
})

watch(
  () => route.params.id,
  (newId) => {
    if (newId && newId !== sessionId) {
      chat.loadMessages(newId as string)
    }
  },
)

async function handleSend(text: string) {
  await chat.send(sessionId, text)
  await loadState()
}

function handleCopy(text: string) {
  navigator.clipboard.writeText(text).catch(() => {})
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
  if (newId) router.push(`/chat/${newId}`)
}
</script>

<template>
  <div
    class="flex flex-col h-full max-w-[600px] mx-auto bg-cover bg-center"
    :style="bgStyle"
  >
    <div class="flex items-center gap-2 px-5 pt-4 pb-2 min-w-0">
      <router-link to="/" class="text-muted hover:text-ink shrink-0" :aria-label="$t('chat.back')"><ArrowLeft :size="18" /></router-link>
      <img v-if="avatar" :src="avatar" class="w-6 h-6 rounded-full object-cover shrink-0" alt="" />
      <span class="font-semibold text-ink truncate">{{ headerName }}</span>
      <span v-if="chat.messages.length" class="ml-auto text-[11.5px] text-muted tabular-nums shrink-0">{{ t('common.tokensEstimate', { tokens: formatTokens(convoTokens) }, convoTokens) }}</span>
    </div>

    <p v-if="chat.error" class="text-coral text-sm px-5 py-4">{{ chat.error }}</p>
    <p v-else-if="chat.loading && chat.messages.length === 0" class="text-muted text-sm px-5 pt-12 text-center">{{ $t('common.loading') }}</p>

    <MessageList
      v-else
      :messages="chat.displayed"
      :all-messages="chat.messages"
      :style="ui.messageStyle"
      :is-streaming="chat.isStreaming"
      :streaming-text="chat.streamingText"
      :streaming-error="chat.streamingError"
      :identity="effectiveIdentity"
      @copy="handleCopy"
      @regenerate="handleRegenerate"
      @fork="handleFork"
      @edit-save="handleEditSave"
      @toggle-hidden="handleToggleHidden"
      @swipe="handleSwipe"
    />

    <VariablesPanel :schema="sessionState.schema" :values="sessionState.values" />
    <Composer :disabled="chat.isStreaming" @send="handleSend" />
  </div>
</template>
