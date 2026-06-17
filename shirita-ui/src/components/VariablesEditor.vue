<script setup lang="ts">
import { X, Plus } from 'lucide-vue-next'
import type { VarDecl, VarType } from '../api/types'

const props = defineProps<{ modelValue: VarDecl[] }>()
const emit = defineEmits<{ 'update:modelValue': [v: VarDecl[]] }>()

const types: VarType[] = ['number', 'bool', 'string', 'list']

function emitWith(next: VarDecl[]) { emit('update:modelValue', next) }
function addRow() { emitWith([...props.modelValue, { name: '', type: 'number', initial: 0 }]) }
function removeRow(i: number) { emitWith(props.modelValue.filter((_, idx) => idx !== i)) }
function patch(i: number, p: Partial<VarDecl>) {
  emitWith(props.modelValue.map((d, idx) => (idx === i ? { ...d, ...p } : d)))
}
function defaultInitial(t: VarType): unknown {
  return t === 'number' ? 0 : t === 'bool' ? false : t === 'list' ? [] : ''
}
</script>

<template>
  <div class="space-y-2">
    <div v-for="(d, i) in modelValue" :key="i" class="flex items-center gap-2">
      <input
        :value="d.name" :placeholder="$t('variables.namePlaceholder')" class="field flex-1 text-[13px]"
        @input="patch(i, { name: ($event.target as HTMLInputElement).value })"
      />
      <select
        :value="d.type" class="field text-[13px]"
        @change="patch(i, { type: ($event.target as HTMLSelectElement).value as VarType, initial: defaultInitial(($event.target as HTMLSelectElement).value as VarType) })"
      >
        <option v-for="t in types" :key="t" :value="t">{{ t }}</option>
      </select>
      <input
        :value="String(d.initial ?? '')" :placeholder="$t('variables.initialPlaceholder')" class="field w-20 text-[13px]"
        @input="patch(i, { initial: d.type === 'number' ? Number(($event.target as HTMLInputElement).value) || 0 : d.type === 'bool' ? ($event.target as HTMLInputElement).value === 'true' : ($event.target as HTMLInputElement).value })"
      />
      <button data-test="remove-var" class="text-muted hover:text-coral" @click="removeRow(i)"><X :size="14" /></button>
    </div>
    <button data-test="add-var" class="flex items-center gap-1 text-[12px] text-primary hover:text-primary-strong" @click="addRow">
      <Plus :size="13" /> {{ $t('variables.add') }}
    </button>
  </div>
</template>
