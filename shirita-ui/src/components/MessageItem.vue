<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronLeft, ChevronRight, Check, X, MoreHorizontal } from 'lucide-vue-next'
import type { Message, Identity } from '../api/types'
import MessageContent from './MessageContent.vue'
import { containsHtmlCard } from '../utils/markdown'
import { useMediaStore } from '../stores/media'
import { formatTokens } from '../utils/tokens'
import { assetUrl } from '../api/client'
import { actionsFor, type MessageActionKey } from '../utils/messageActions'

const props = withDefaults(defineProps<{
  message: Message
  style: 'bubble' | 'flat'
  isStreaming?: boolean
  siblingIndex?: number   // 0-based position among siblings
  siblingCount?: number
  identity?: Identity
  /** Running token estimate for the whole conversation; shown only on the last message. */
  tokens?: number
  /** When true, open this message's inline editor (driven by the action sheet). */
  forceEdit?: boolean
}>(), { siblingCount: 1, siblingIndex: 0, forceEdit: false })

const emit = defineEmits<{
  copy: [text: string]
  regenerate: []
  fork: []
  'edit-save': [text: string]
  'edit-cancel': []
  'toggle-hidden': []
  delete: []
  swipe: [delta: -1 | 1]
  'open-actions': []
}>()

const { t } = useI18n()
const isAssistant = computed(() => props.message.role === 'assistant')
const isUser = computed(() => props.message.role === 'user')
const inlineActions = computed(() => actionsFor(props.message.role))

function inlineLabel(key: MessageActionKey): string {
  if (key === 'toggle-hidden') return t(props.message.is_hidden ? 'chat.unhide' : 'chat.hide')
  return t(inlineActions.value.find((a) => a.key === key)?.labelKey ?? '')
}
function runInline(key: MessageActionKey) {
  switch (key) {
    case 'copy': emit('copy', props.message.raw_content); break
    case 'regenerate': emit('regenerate'); break
    case 'fork': emit('fork'); break
    case 'edit': startEdit(); break
    case 'toggle-hidden': emit('toggle-hidden'); break
    case 'delete': emit('delete'); break
  }
}

// Read a string value from snapshot_state, guarding type.
function stateStr(key: string): string | undefined {
  const v = (props.message?.snapshot_state as Record<string, unknown>)?.[key]
  return typeof v === 'string' ? v : undefined
}

// Per-message identity with fallback chain:
//   snapshot_state → props.identity → hardcoded default
const displayName = computed(() => {
  if (isAssistant.value) {
    return stateStr('$assistant_name') || props.identity?.assistant?.name || t('chat.assistant')
  }
  return stateStr('$user_name') || props.identity?.user?.name || t('chat.you')
})
const avatarUrl = computed(() => {
  const a = isAssistant.value
    ? (stateStr('$assistant_avatar') || props.identity?.assistant?.avatar)
    : (stateStr('$user_avatar') || props.identity?.user?.avatar)
  return a ? assetUrl(a) : ''
})
const label = displayName
const hasSwipes = computed(() => isAssistant.value && (props.siblingCount ?? 1) > 1)
const displayText = computed(() => props.message.display_content ?? props.message.raw_content)
// HTML cards are rich, self-contained content meant to use the full message-row
// width — the 78% cap below is a chat-bubble-text aesthetic that squeezes them.
const isCard = computed(() => containsHtmlCard(displayText.value))

const media = useMediaStore()
onMounted(() => { media.load('avatar'); media.load('background') })
const allAssets = computed(() => [...media.byKind('avatar'), ...media.byKind('background')])
const attachmentUrls = computed(() =>
  props.message.attachments
    .map((id) => allAssets.value.find((a) => a.id === id)?.url)
    .filter((u): u is string => !!u),
)

const editing = ref(false)
const draft = ref('')
function startEdit() { draft.value = props.message.raw_content; editing.value = true }
function saveEdit() { editing.value = false; emit('edit-save', draft.value) }
function cancelEdit() { editing.value = false; emit('edit-cancel') }
// The action sheet can ask a specific message to open its inline editor.
watch(
  () => props.forceEdit,
  (on) => { if (on) startEdit() },
)
</script>

<template>
  <!-- Bubble mode -->
  <div
    v-if="style === 'bubble'"
    data-test="msg-row"
    :data-role="message.role"
    :class="['app-message flex gap-2.5 mb-4', isUser ? 'justify-end' : 'justify-start']"
  >
    <div v-if="isAssistant" data-test="assistant-avatar" class="w-8 h-8 rounded-full bg-sky/40 shrink-0 mt-0.5 overflow-hidden">
      <img v-if="avatarUrl" :src="avatarUrl" class="w-full h-full object-cover rounded-full" alt="" />
    </div>
    <div data-test="msg-bubble-wrapper" :class="[isCard ? 'max-w-full' : 'max-w-[78%]', isUser ? 'order-first' : '']">
      <div
        :class="[
          'px-3.5 py-2.5 text-[15px] leading-relaxed whitespace-pre-wrap',
          isUser
            ? 'bg-coral text-[#1b1b1b] rounded-[16px] rounded-br-[4px]'
            : 'bg-card border border-line text-ink rounded-[16px] rounded-bl-[4px]',
          message.is_hidden ? 'opacity-50' : '',
        ]"
      >
        <template v-if="editing">
          <textarea
            data-test="edit-area"
            v-model="draft"
            rows="3"
            class="w-full bg-card text-ink border border-line rounded-[10px] px-3 py-2 text-[15px] outline-none focus:border-primary/50"
          />
          <div class="flex gap-2 mt-1.5">
            <button data-test="edit-save" class="text-primary hover:text-primary-strong" :aria-label="$t('common.save')" :title="$t('common.save')" @click="saveEdit"><Check :size="16" /></button>
            <button class="text-muted hover:text-ink" :aria-label="$t('common.cancel')" :title="$t('common.cancel')" @click="cancelEdit"><X :size="16" /></button>
          </div>
        </template>
        <template v-else>
          <div v-if="attachmentUrls.length" data-test="message-attachments" class="flex flex-wrap gap-1.5 mb-1.5">
            <img v-for="url in attachmentUrls" :key="url" :src="url" class="w-20 h-20 rounded-lg object-cover border border-line/50" alt="" />
          </div>
          <MessageContent :text="displayText" /><span
            v-if="isStreaming"
            data-test="streaming-cursor"
            class="inline-block w-[7px] h-[15px] bg-primary align-[-3px] ml-0.5 rounded-[1px] animate-pulse"
          />
        </template>
      </div>

      <div
        v-if="!editing"
        data-test="message-actions"
        :class="['max-sm:hidden flex flex-wrap items-center gap-1.5 mt-1.5 ml-1 text-muted', isUser ? 'justify-end' : '']"
      >
        <span v-if="hasSwipes" data-test="swipe-indicator" class="flex items-center gap-1 text-[12px]">
          <button data-test="swipe-prev" class="hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) <= 0" :aria-label="$t('chat.previousVariation')" @click="emit('swipe', -1)"><ChevronLeft :size="14" :stroke-width="2.2" /></button>
          <span>{{ (siblingIndex ?? 0) + 1 }}/{{ siblingCount }}</span>
          <button data-test="swipe-next" class="hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) >= (siblingCount ?? 1) - 1" :aria-label="$t('chat.nextVariation')" @click="emit('swipe', 1)"><ChevronRight :size="14" :stroke-width="2.2" /></button>
        </span>
        <span v-if="hasSwipes" class="w-px h-3.5 bg-line" />
        <button
          v-for="a in inlineActions"
          :key="a.key"
          :data-test="a.testId"
          class="hover:text-ink"
          :class="a.key === 'delete' ? 'hover:text-coral' : ''"
          :aria-label="inlineLabel(a.key)"
          :title="inlineLabel(a.key)"
          @click="runInline(a.key)"
        >
          <component :is="a.icon" :size="15" :stroke-width="1.8" />
        </button>
        <span v-if="tokens !== undefined" data-test="convo-tokens" class="ml-auto text-[11.5px] tabular-nums">{{ $t('common.tokensEstimate', { tokens: formatTokens(tokens) }, tokens) }}</span>
      </div>
      <!-- mobile: explicit More actions trigger opens the viewport-level sheet -->
      <button
        v-if="!editing && !isStreaming"
        data-test="more-actions-btn"
        class="sm:hidden flex items-center gap-1 mt-1.5 ml-1 text-muted hover:text-ink text-[12px]"
        :aria-label="$t('chat.options')"
        :title="$t('chat.options')"
        @click="emit('open-actions')"
      >
        <MoreHorizontal :size="16" :stroke-width="1.8" />
        <span>{{ $t('chat.options') }}</span>
      </button>
    </div>
  </div>

  <!-- Flat mode -->
  <div v-else data-test="msg-row" :data-role="message.role" class="app-message px-1 py-3.5 border-b border-line/70 last:border-b-0">
    <div class="flex items-center gap-2.5 mb-1.5">
      <div :class="['w-6 h-6 rounded-full shrink-0 overflow-hidden', isAssistant ? 'bg-sky/40' : 'bg-mauve/30']">
        <img v-if="avatarUrl" :src="avatarUrl" class="w-full h-full object-cover rounded-full" alt="" />
      </div>
      <span class="text-[13px] font-semibold text-ink">{{ label }}</span>
    </div>
    <div :class="['text-[15px] leading-relaxed whitespace-pre-wrap pl-[34px] text-ink', message.is_hidden ? 'opacity-50' : '']">
      <template v-if="editing">
        <textarea
          data-test="edit-area"
          v-model="draft"
          rows="3"
          class="w-full bg-card text-ink border border-line rounded-[10px] px-3 py-2 text-[15px] outline-none focus:border-primary/50"
        />
        <div class="flex gap-2 mt-1.5">
          <button data-test="edit-save" class="text-primary hover:text-primary-strong" :aria-label="$t('common.save')" :title="$t('common.save')" @click="saveEdit"><Check :size="16" /></button>
          <button class="text-muted hover:text-ink" :aria-label="$t('common.cancel')" :title="$t('common.cancel')" @click="cancelEdit"><X :size="16" /></button>
        </div>
      </template>
      <template v-else>
        <div v-if="attachmentUrls.length" data-test="message-attachments" class="flex flex-wrap gap-1.5 mb-1.5">
          <img v-for="url in attachmentUrls" :key="url" :src="url" class="w-20 h-20 rounded-lg object-cover border border-line/50" alt="" />
        </div>
        <MessageContent :text="displayText" /><span
          v-if="isStreaming"
          data-test="streaming-cursor"
          class="inline-block w-[7px] h-[15px] bg-primary align-[-3px] ml-0.5 rounded-[1px] animate-pulse"
        />
      </template>
    </div>
    <div v-if="!editing" data-test="message-actions" class="max-sm:hidden flex flex-wrap items-center gap-1.5 mt-2 pl-[34px] text-muted">
      <span v-if="hasSwipes" data-test="swipe-indicator" class="flex items-center gap-1 text-[12px]">
        <button data-test="swipe-prev" class="hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) <= 0" :aria-label="$t('chat.previousVariation')" @click="emit('swipe', -1)"><ChevronLeft :size="14" :stroke-width="2.2" /></button>
        <span>{{ (siblingIndex ?? 0) + 1 }}/{{ siblingCount }}</span>
        <button data-test="swipe-next" class="hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) >= (siblingCount ?? 1) - 1" :aria-label="$t('chat.nextVariation')" @click="emit('swipe', 1)"><ChevronRight :size="14" :stroke-width="2.2" /></button>
      </span>
      <span v-if="hasSwipes" class="w-px h-3.5 bg-line" />
      <button
        v-for="a in inlineActions"
        :key="a.key"
        :data-test="a.testId"
        class="hover:text-ink"
        :class="a.key === 'delete' ? 'hover:text-coral' : ''"
        :aria-label="inlineLabel(a.key)"
        :title="inlineLabel(a.key)"
        @click="runInline(a.key)"
      >
        <component :is="a.icon" :size="15" :stroke-width="1.8" />
      </button>
      <span v-if="tokens !== undefined" data-test="convo-tokens" class="ml-auto text-[11.5px] tabular-nums">{{ $t('common.tokensEstimate', { tokens: formatTokens(tokens) }, tokens) }}</span>
    </div>
    <button
      v-if="!editing && !isStreaming"
      data-test="more-actions-btn"
      class="sm:hidden flex items-center gap-1 mt-2 pl-[34px] text-muted hover:text-ink text-[12px]"
      :aria-label="$t('chat.options')"
      :title="$t('chat.options')"
      @click="emit('open-actions')"
    >
      <MoreHorizontal :size="16" :stroke-width="1.8" />
      <span>{{ $t('chat.options') }}</span>
    </button>
  </div>
</template>
