<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { Message, Identity } from '../api/types'
import { siblings } from '../utils/tree'
import MessageItem from './MessageItem.vue'

const props = defineProps<{
  messages: Message[]        // the active path (displayed)
  allMessages?: Message[]    // full set, for sibling counts (defaults to messages)
  style: 'bubble' | 'flat'
  isStreaming?: boolean
  streamingText?: string
  streamingError?: string | null
  identity?: Identity
  /** Running token estimate for the conversation; shown next to the last message's actions. */
  tokens?: number
}>()

const emit = defineEmits<{
  copy: [text: string]
  regenerate: [id: string]
  fork: [id: string]
  'edit-save': [id: string, text: string]
  'toggle-hidden': [id: string]
  delete: [id: string]
  swipe: [id: string, delta: -1 | 1]
  'dismiss-streaming-error': []
}>()

// Scroll anchoring: the transcript auto-follows the bottom only while the user
// is at or near it (within FOLLOW_THRESHOLD px). Scrolling up above the
// threshold disables following so streaming growth and the streaming→persisted
// replacement never yank the view back down. The decision is local to this
// component — Pinia does not need to know about scroll state.
const FOLLOW_THRESHOLD = 48
const scroller = ref<HTMLElement | null>(null)
const isFollowingBottom = ref(true)

function distanceFromBottom(): number {
  const el = scroller.value
  if (!el) return 0
  return el.scrollHeight - el.clientHeight - el.scrollTop
}

/** Schedule a bottom scroll after Vue has updated the DOM for this render
 *  turn, but only if the user is still following. Coalesces per-turn: the
 *  watchers below all funnel through here, so token deltas in one update
 *  produce one scroll. */
async function scrollToBottomIfFollowing() {
  await nextTick()
  if (!isFollowingBottom.value) return
  scroller.value?.scrollTo({ top: scroller.value.scrollHeight, behavior: 'auto' })
}

function onScroll() {
  isFollowingBottom.value = distanceFromBottom() <= FOLLOW_THRESHOLD
}

onMounted(() => {
  void scrollToBottomIfFollowing()
  scroller.value?.addEventListener('scroll', onScroll)
})
onBeforeUnmount(() => {
  scroller.value?.removeEventListener('scroll', onScroll)
})

// React to exactly the render signals that change the transcript's height or
// identity: message count/identity, the streaming ghost's text, and the
// streaming flag (ghost present ⇔ streaming). Anything else (style, identity
// name) must not trigger a scroll.
watch(
  [() => props.messages, () => props.streamingText, () => props.isStreaming],
  () => void scrollToBottomIfFollowing(),
)

// Anchor messages are synthetic prompt-only turns; never render them.
const visibleMessages = computed(() => props.messages.filter((m) => !m.is_anchor))
const lastVisibleId = computed(() => visibleMessages.value.at(-1)?.id)

function sibInfo(msg: Message) {
  const sibs = siblings(props.allMessages ?? props.messages, msg)
  return { index: sibs.findIndex((s) => s.id === msg.id), count: sibs.length }
}

const streamingMsg = computed<Message | null>(() => {
  if (!props.streamingText) return null
  if (!props.isStreaming) return null
  return {
    id: '__streaming__',
    session_id: '',
    parent_id: null,
    role: 'assistant',
    raw_content: props.streamingText || '',
    display_content: null,
    is_hidden: false,
    is_anchor: false,
    attachments: [],
    snapshot_state: {},
    created_at: '',
  }
})
</script>

<template>
  <div ref="scroller" data-test="message-scroll" class="flex-1 overflow-y-auto px-3 sm:px-5 py-4">
    <p v-if="visibleMessages.length === 0 && !streamingMsg && !streamingError" class="text-muted text-sm text-center pt-12">
      {{ $t('chat.empty') }}
    </p>

    <MessageItem
      v-for="msg in visibleMessages"
      :key="msg.id"
      :message="msg"
      :style="style"
      :identity="identity"
      :sibling-index="sibInfo(msg).index"
      :sibling-count="sibInfo(msg).count"
      :tokens="msg.id === lastVisibleId ? tokens : undefined"
      @copy="emit('copy', $event)"
      @regenerate="emit('regenerate', msg.id)"
      @fork="emit('fork', msg.id)"
      @edit-save="(t) => emit('edit-save', msg.id, t)"
      @toggle-hidden="emit('toggle-hidden', msg.id)"
      @delete="emit('delete', msg.id)"
      @swipe="(d) => emit('swipe', msg.id, d)"
    />

    <MessageItem
      v-if="streamingMsg"
      :message="streamingMsg"
      :style="style"
      :identity="identity"
      :is-streaming="true"
    />

    <div v-if="streamingError" data-test="streaming-error" class="flex items-center justify-center gap-2 py-2">
      <span class="text-coral text-sm">{{ streamingError }}</span>
      <button class="text-muted hover:text-ink text-sm" data-test="dismiss-streaming-error" @click="emit('dismiss-streaming-error')">
        {{ $t('chat.dismiss') }}
      </button>
    </div>
  </div>
</template>
