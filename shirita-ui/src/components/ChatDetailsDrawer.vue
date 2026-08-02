<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { X } from 'lucide-vue-next'
import type { SessionPanel, PanelAction, VarDecl } from '../api/types'
import PanelView from './PanelView.vue'

const props = defineProps<{
  open: boolean
  panels: SessionPanel[]
  schema: VarDecl[]
  values: Record<string, unknown>
}>()

const emit = defineEmits<{
  close: []
  'panel-action': [panel: SessionPanel, action: PanelAction]
}>()

const dialogRef = ref<HTMLElement | null>(null)

const system = computed(() => props.schema.filter((d) => d.scope === 'system'))
const custom = computed(() => props.schema.filter((d) => d.scope !== 'system'))

function fmt(v: unknown): string {
  if (typeof v === 'boolean') return v ? '✓' : '✗'
  if (Array.isArray(v)) return v.length ? v.join(', ') : '—'
  if (v === undefined || v === null || v === '') return '—'
  return String(v)
}

function focusables(): HTMLElement[] {
  const el = dialogRef.value
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
  // Complete minimal focus trap: Tab/Shift+Tab cycle within the dialog.
  const list = focusables()
  if (list.length === 0) return
  const first = list[0]
  const last = list[list.length - 1]
  const from = document.activeElement as HTMLElement | null
  if (e.shiftKey) {
    if (from === first || !dialogRef.value?.contains(from)) {
      e.preventDefault()
      last.focus()
    }
  } else if (from === last || !dialogRef.value?.contains(from)) {
    e.preventDefault()
    first.focus()
  }
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      nextTick(() => {
        const list = focusables()
        ;(list[0] ?? dialogRef.value)?.focus()
      })
    }
  },
  { immediate: true },
)
</script>

<template>
  <!-- Fixed overlay, layered above the app shell; not part of the transcript. -->
  <Teleport to="body">
    <div v-if="open" data-test="chat-details" class="app-chat-details fixed inset-0 z-50">
      <div class="absolute inset-0 bg-black/40" data-test="details-backdrop" @click="emit('close')" />
      <div
        ref="dialogRef"
        role="dialog"
        aria-modal="true"
        :aria-label="$t('chat.details')"
        data-test="details-dialog"
        tabindex="-1"
        class="absolute right-0 top-0 bottom-0 w-full max-w-md bg-card border-l border-line flex flex-col shadow-xl"
        @keydown="onKeydown"
      >
        <div class="flex items-center justify-between px-4 py-3 border-b border-line shrink-0">
          <h2 class="text-[13px] font-semibold text-ink">{{ $t('chat.details') }}</h2>
          <button
            data-test="details-close"
            class="text-muted hover:text-ink"
            :aria-label="$t('common.close')"
            :title="$t('common.close')"
            @click="emit('close')"
          >
            <X :size="16" />
          </button>
        </div>
        <div class="flex-1 overflow-y-auto px-4 py-3 space-y-4" data-test="details-scroll">
          <!-- visible session panels in resolution order -->
          <section v-if="panels.length" data-test="details-panels" class="space-y-2">
            <details v-for="p in panels" :key="p.id" open class="rounded-xl border border-line bg-card/50 overflow-hidden">
              <summary class="cursor-pointer select-none px-3 py-2 text-[12px] font-semibold text-muted">{{ p.name }}</summary>
              <div class="px-2 pb-2">
                <PanelView :html="p.html" :css="p.css" :values="values" @action="(a) => emit('panel-action', p, a)" />
              </div>
            </details>
          </section>

          <!-- variables, grouped into system and custom values -->
          <section v-if="schema.length" data-test="details-variables" class="text-[13px] space-y-2">
            <div v-if="system.length" data-test="var-system">
              <span class="text-[11px] uppercase tracking-[0.06em] text-muted">{{ $t('variables.system') }}</span>
              <div class="flex flex-wrap gap-x-4 gap-y-1 mt-1">
                <span v-for="d in system" :key="d.name" data-test="var-row" class="tabular-nums">
                  <span class="text-muted">{{ d.name }}</span> {{ fmt(values[d.name]) }}
                </span>
              </div>
            </div>
            <div v-if="custom.length" data-test="var-custom">
              <span class="text-[11px] uppercase tracking-[0.06em] text-muted">{{ $t('variables.custom') }}</span>
              <div class="flex flex-wrap gap-x-4 gap-y-1 mt-1">
                <span v-for="d in custom" :key="d.name" data-test="var-row" class="tabular-nums">
                  <span class="text-muted">{{ d.name }}</span> {{ fmt(values[d.name]) }}
                </span>
              </div>
            </div>
          </section>
        </div>
      </div>
    </div>
  </Teleport>
</template>
