<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Copy, RefreshCw, GitFork, Pencil, EyeOff, Eye, ChevronLeft, ChevronRight, Check, X } from 'lucide-vue-next'
import type { Message, Identity } from '../api/types'
import MessageContent from './MessageContent.vue'
import { useMediaStore } from '../stores/media'
import { formatTokens } from '../utils/tokens'

const props = withDefaults(defineProps<{
  message: Message
  style: 'bubble' | 'flat'
  isStreaming?: boolean
  siblingIndex?: number   // 0-based position among siblings
  siblingCount?: number
  identity?: Identity
  /** Running token estimate for the whole conversation; shown only on the last message. */
  tokens?: number
}>(), { siblingCount: 1, siblingIndex: 0 })

const emit = defineEmits<{
  copy: [text: string]
  regenerate: []
  fork: []
  'edit-save': [text: string]
  'toggle-hidden': []
  swipe: [delta: -1 | 1]
}>()

const { t } = useI18n()
const isAssistant = computed(() => props.message.role === 'assistant')
const isUser = computed(() => props.message.role === 'user')
const side = computed(() => (isAssistant.value ? props.identity?.assistant : props.identity?.user))
const displayName = computed(() => side.value?.name || (isAssistant.value ? t('chat.assistant') : t('chat.you')))
const avatarUrl = computed(() => (side.value?.avatar ? `/assets/${side.value.avatar}` : ''))
const label = displayName
const hasSwipes = computed(() => isAssistant.value && (props.siblingCount ?? 1) > 1)
const displayText = computed(() => props.message.display_content ?? props.message.raw_content)

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
function cancelEdit() { editing.value = false }
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
    <div :class="['max-w-[78%]', isUser ? 'order-first' : '']">
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
            <button data-test="edit-save" class="text-primary hover:text-primary-strong" :title="$t('common.save')" @click="saveEdit"><Check :size="16" /></button>
            <button class="text-muted hover:text-ink" :title="$t('common.cancel')" @click="cancelEdit"><X :size="16" /></button>
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
        :class="['flex items-center gap-1.5 mt-1.5 ml-1 text-muted', isUser ? 'justify-end' : '']"
      >
        <span v-if="hasSwipes" data-test="swipe-indicator" class="flex items-center gap-1 text-[12px]">
          <button data-test="swipe-prev" class="hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) <= 0" @click="emit('swipe', -1)"><ChevronLeft :size="14" :stroke-width="2.2" /></button>
          <span>{{ (siblingIndex ?? 0) + 1 }}/{{ siblingCount }}</span>
          <button data-test="swipe-next" class="hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) >= (siblingCount ?? 1) - 1" @click="emit('swipe', 1)"><ChevronRight :size="14" :stroke-width="2.2" /></button>
        </span>
        <span v-if="hasSwipes" class="w-px h-3.5 bg-line" />
        <button v-if="isAssistant" data-test="regenerate-btn" class="hover:text-ink" :title="$t('chat.regenerate')" @click="emit('regenerate')">
          <RefreshCw :size="15" :stroke-width="1.8" />
        </button>
        <button v-if="isAssistant" class="hover:text-ink" :title="$t('chat.fork')" @click="emit('fork')">
          <GitFork :size="15" :stroke-width="1.8" />
        </button>
        <button data-test="copy-btn" class="hover:text-ink" :title="$t('chat.copy')" @click="emit('copy', message.raw_content)">
          <Copy :size="15" :stroke-width="1.8" />
        </button>
        <button data-test="edit-btn" class="hover:text-ink" :title="$t('chat.edit')" @click="startEdit">
          <Pencil :size="15" :stroke-width="1.8" />
        </button>
        <button data-test="hide-btn" class="hover:text-ink" :title="message.is_hidden ? $t('chat.unhide') : $t('chat.hide')" @click="emit('toggle-hidden')">
          <component :is="message.is_hidden ? Eye : EyeOff" :size="15" :stroke-width="1.8" />
        </button>
        <span v-if="tokens !== undefined" data-test="convo-tokens" class="ml-auto text-[11.5px] tabular-nums">{{ $t('common.tokensEstimate', { tokens: formatTokens(tokens) }, tokens) }}</span>
      </div>
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
          <button data-test="edit-save" class="text-primary hover:text-primary-strong" :title="$t('common.save')" @click="saveEdit"><Check :size="16" /></button>
          <button class="text-muted hover:text-ink" :title="$t('common.cancel')" @click="cancelEdit"><X :size="16" /></button>
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
    <div v-if="!editing" data-test="message-actions" class="flex items-center gap-1.5 mt-2 pl-[34px] text-muted">
      <span v-if="hasSwipes" data-test="swipe-indicator" class="flex items-center gap-1 text-[12px]">
        <button data-test="swipe-prev" class="hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) <= 0" @click="emit('swipe', -1)"><ChevronLeft :size="14" :stroke-width="2.2" /></button>
        <span>{{ (siblingIndex ?? 0) + 1 }}/{{ siblingCount }}</span>
        <button data-test="swipe-next" class="hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) >= (siblingCount ?? 1) - 1" @click="emit('swipe', 1)"><ChevronRight :size="14" :stroke-width="2.2" /></button>
      </span>
      <span v-if="hasSwipes" class="w-px h-3.5 bg-line" />
      <button v-if="isAssistant" data-test="regenerate-btn" class="hover:text-ink" :title="$t('chat.regenerate')" @click="emit('regenerate')">
        <RefreshCw :size="15" :stroke-width="1.8" />
      </button>
      <button v-if="isAssistant" class="hover:text-ink" :title="$t('chat.fork')" @click="emit('fork')">
        <GitFork :size="15" :stroke-width="1.8" />
      </button>
      <button data-test="copy-btn" class="hover:text-ink" :title="$t('chat.copy')" @click="emit('copy', message.raw_content)">
        <Copy :size="15" :stroke-width="1.8" />
      </button>
      <button data-test="edit-btn" class="hover:text-ink" :title="$t('chat.edit')" @click="startEdit">
        <Pencil :size="15" :stroke-width="1.8" />
      </button>
      <button data-test="hide-btn" class="hover:text-ink" :title="message.is_hidden ? $t('chat.unhide') : $t('chat.hide')" @click="emit('toggle-hidden')">
        <component :is="message.is_hidden ? Eye : EyeOff" :size="15" :stroke-width="1.8" />
      </button>
      <span v-if="tokens !== undefined" data-test="convo-tokens" class="ml-auto text-[11.5px] tabular-nums">{{ $t('common.tokensEstimate', { tokens: formatTokens(tokens) }, tokens) }}</span>
    </div>
  </div>
</template>
