<script setup lang="ts">
import { ref, computed } from 'vue'
import { ChevronDown, ChevronRight } from 'lucide-vue-next'
import type { VarDecl } from '../api/types'

const props = defineProps<{ schema: VarDecl[]; values: Record<string, unknown> }>()
const open = ref(false)

const system = computed(() => props.schema.filter((d) => d.scope === 'system'))
const custom = computed(() => props.schema.filter((d) => d.scope !== 'system'))

function fmt(v: unknown): string {
  if (typeof v === 'boolean') return v ? '✓' : '✗'
  if (Array.isArray(v)) return v.length ? v.join(', ') : '—'
  if (v === undefined || v === null || v === '') return '—'
  return String(v)
}
</script>

<template>
  <div v-if="schema.length" data-test="variables-panel" class="border-t border-line/70 px-5 py-2 text-[13px]">
    <button data-test="variables-toggle" class="flex items-center gap-1 text-muted hover:text-ink" @click="open = !open">
      <component :is="open ? ChevronDown : ChevronRight" :size="14" />
      <span>Variables</span>
    </button>
    <div v-if="open" class="mt-2 space-y-2">
      <div v-if="system.length" data-test="var-system">
        <span class="text-[11px] uppercase tracking-[0.06em] text-muted">System</span>
        <div class="flex flex-wrap gap-x-4 gap-y-1 mt-1">
          <span v-for="d in system" :key="d.name" data-test="var-row" class="tabular-nums">
            <span class="text-muted">{{ d.name }}</span> {{ fmt(values[d.name]) }}
          </span>
        </div>
      </div>
      <div v-if="custom.length" data-test="var-custom">
        <span class="text-[11px] uppercase tracking-[0.06em] text-muted">Custom</span>
        <div class="flex flex-wrap gap-x-4 gap-y-1 mt-1">
          <span v-for="d in custom" :key="d.name" data-test="var-row" class="tabular-nums">
            <span class="text-muted">{{ d.name }}</span> {{ fmt(values[d.name]) }}
          </span>
        </div>
      </div>
    </div>
  </div>
</template>
