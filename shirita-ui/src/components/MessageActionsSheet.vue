<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronLeft, ChevronRight } from 'lucide-vue-next'
import type { Message } from '../api/types'
import { actionsFor, type MessageActionKey } from '../utils/messageActions'

const props = defineProps<{
  message: Message
  siblingIndex?: number
  siblingCount?: number
}>()

const emit = defineEmits<{
  action: [key: MessageActionKey]
  swipe: [delta: -1 | 1]
  close: []
}>()

const { t } = useI18n()
const sheetRef = ref<HTMLElement | null>(null)

const isAssistant = computed(() => props.message.role === 'assistant')
const isHidden = computed(() => props.message.is_hidden)
const actions = computed(() => actionsFor(props.message.role))
const hasSwipes = computed(() => isAssistant.value && (props.siblingCount ?? 1) > 1)

function label(key: MessageActionKey): string {
  if (key === 'toggle-hidden') return t(isHidden.value ? 'chat.unhide' : 'chat.hide')
  return t(actions.value.find((a) => a.key === key)?.labelKey ?? '')
}

function run(key: MessageActionKey) {
  emit('action', key)
  emit('close')
}

function swipe(delta: -1 | 1) {
  emit('swipe', delta)
  emit('close')
}

function focusables(): HTMLElement[] {
  const el = sheetRef.value
  if (!el) return []
  const sel = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
  return Array.from(el.querySelectorAll<HTMLElement>(sel))
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    emit('close')
    return
  }
  if (e.key !== 'Tab') return
  // Complete minimal focus trap: Tab/Shift+Tab cycle within the sheet so
  // keyboard users never fall through into the background page.
  const list = focusables()
  if (list.length === 0) return
  const first = list[0]
  const last = list[list.length - 1]
  const from = document.activeElement as HTMLElement | null
  if (e.shiftKey) {
    if (from === first || !sheetRef.value?.contains(from)) {
      e.preventDefault()
      last.focus()
    }
  } else if (from === last || !sheetRef.value?.contains(from)) {
    e.preventDefault()
    first.focus()
  }
}

watch(
  () => props.message.id,
  () => {
    nextTick(() => {
      const list = focusables()
      ;(list[0] ?? sheetRef.value)?.focus()
    })
  },
  { immediate: true },
)
</script>

<template>
  <!-- One viewport-level action surface per transcript, teleported outside the
       message subtree. Backdrop spans the viewport; the sheet respects the
       left/right/bottom safe areas. -->
  <Teleport to="body">
    <div data-test="message-action-sheet" class="app-message-action-sheet fixed inset-0 z-50">
      <div class="absolute inset-0 bg-black/40" data-test="action-backdrop" @click="emit('close')" />
      <div
        ref="sheetRef"
        role="dialog"
        aria-modal="true"
        :aria-label="t('chat.options')"
        data-test="action-dialog"
        tabindex="-1"
        class="absolute inset-x-0 bottom-0 sm:inset-x-auto sm:right-0 sm:top-0 sm:bottom-0 sm:w-72 bg-card border-t sm:border-t-0 sm:border-l border-line flex flex-col shadow-xl"
        style="padding-bottom: env(safe-area-inset-bottom)"
        @keydown="onKeydown"
      >
        <div class="flex items-center justify-between px-4 py-3 border-b border-line shrink-0">
          <h2 class="text-[13px] font-semibold text-ink truncate">{{ $t('chat.options') }}</h2>
          <button data-test="action-close" class="text-muted hover:text-ink" :aria-label="$t('common.close')" :title="$t('common.close')" @click="emit('close')">
            ✕
          </button>
        </div>
        <div class="flex-1 overflow-y-auto px-2 py-2 space-y-1" style="padding-left: max(env(safe-area-inset-left), 0.5rem); padding-right: max(env(safe-area-inset-right), 0.5rem)">
          <!-- variation controls -->
          <div v-if="hasSwipes" data-test="action-swipes" class="flex items-center justify-between px-2 py-2 rounded-lg hover:bg-surface">
            <span class="text-[13px] text-ink">{{ (siblingIndex ?? 0) + 1 }}/{{ siblingCount }}</span>
            <span class="flex items-center gap-1">
              <button data-test="action-swipe-prev" class="p-1 text-muted hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) <= 0" :aria-label="$t('chat.previousVariation')" @click="swipe(-1)"><ChevronLeft :size="16" /></button>
              <button data-test="action-swipe-next" class="p-1 text-muted hover:text-ink disabled:opacity-30" :disabled="(siblingIndex ?? 0) >= (siblingCount ?? 1) - 1" :aria-label="$t('chat.nextVariation')" @click="swipe(1)"><ChevronRight :size="16" /></button>
            </span>
          </div>

          <button
            v-for="a in actions"
            :key="a.key"
            :data-test="a.testId"
            class="w-full flex items-center gap-2.5 px-3 py-2.5 rounded-lg text-left text-[13px] text-ink hover:bg-surface data-[danger]:text-coral"
            :data-danger="a.key === 'delete' ? '' : undefined"
            @click="run(a.key)"
          >
            <component :is="a.icon" :size="16" :stroke-width="1.8" />
            <span>{{ label(a.key) }}</span>
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
