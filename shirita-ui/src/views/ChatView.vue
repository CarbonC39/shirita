<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { useChatStore } from '../stores/chat'
import { useUiStore } from '../stores/ui'
import { estimateTokens } from '../utils/tokens'
import { siblings } from '../utils/tree'
import { getSessionState, getSessionIdentity, getSession, getPack, applyStateUpdates, assetUrl } from '../api/client'
import type { SessionState, Identity, Pack, Panel, PanelAction } from '../api/types'
import MessageList from '../components/MessageList.vue'
import Composer from '../components/Composer.vue'
import VariablesPanel from '../components/VariablesPanel.vue'
import PanelView from '../components/PanelView.vue'
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
// Mounted packs that ship a panel, in mount order.
const panelPacks = ref<Pack[]>([])
function panelOf(p: Pack): Panel {
  return (p.meta as { panel: Panel }).panel
}
async function loadPanels() {
  try {
    const session = await getSession(sessionId)
    const ids = session.mounted_packs ?? []
    const packs = await Promise.all(ids.map((pid) => getPack(pid)))
    panelPacks.value = packs.filter((p) => (p.meta as { panel?: Panel }).panel)
  } catch {
    panelPacks.value = []
  }
}

async function onPanelAction(pack: Pack, action: PanelAction) {
  const caps = panelOf(pack).caps || {}
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
  chat.loadMessages(sessionId)
  loadState()
  loadIdentity()
  loadPanels()
})

watch(
  () => route.params.id,
  (newId) => {
    if (newId && newId !== sessionId) {
      chat.loadMessages(newId as string)
    }
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
onUnmounted(() => { window.removeEventListener('message', onCardMessage) })

async function handleSend(text: string, attachments: string[]) {
  await chat.send(sessionId, text, attachments)
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
    class="app-chat-column flex flex-col h-full"
  >
    <div class="flex items-center gap-2 pt-4 pb-2 min-w-0">
      <router-link to="/" class="text-muted hover:text-ink shrink-0" :aria-label="$t('chat.back')"><ArrowLeft :size="18" /></router-link>
      <img v-if="avatar" :src="avatar" class="w-6 h-6 rounded-full object-cover shrink-0" alt="" />
      <span class="font-semibold text-ink truncate">{{ headerName }}</span>
    </div>

    <div v-if="panelPacks.length" data-test="panel-stack" class="flex flex-col gap-2 py-2">
      <details v-for="p in panelPacks" :key="p.id" open class="rounded-xl border border-line bg-card/50 overflow-hidden">
        <summary class="cursor-pointer select-none px-3 py-2 text-[12px] font-semibold text-muted">{{ p.identity.display_name || p.name }}</summary>
        <div class="px-2 pb-2">
          <PanelView :html="panelOf(p).html" :css="panelOf(p).css" :values="sessionState.values" @action="onPanelAction(p, $event)" />
        </div>
      </details>
    </div>

    <p v-if="chat.error" class="text-coral text-sm py-4">{{ chat.error }}</p>
    <p v-else-if="chat.loading && chat.messages.length === 0" class="text-muted text-sm pt-12 text-center">{{ $t('common.loading') }}</p>

    <MessageList
      v-else
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
      @swipe="handleSwipe"
    />

    <VariablesPanel :schema="sessionState.schema" :values="sessionState.values" />
    <Composer ref="composerRef" :disabled="chat.isStreaming" @send="handleSend" />
  </div>
</template>
