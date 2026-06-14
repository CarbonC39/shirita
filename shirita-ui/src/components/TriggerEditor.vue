<script setup lang="ts">
import { ref } from 'vue'
import { X } from 'lucide-vue-next'
import SegmentedControl from './SegmentedControl.vue'
import SliderControl from './SliderControl.vue'
import type { Trigger } from '../api/types'

const props = defineProps<{ modelValue: Trigger }>()
const emit = defineEmits<{ 'update:modelValue': [value: Trigger] }>()

const draft = ref('')

function patch(p: Partial<Trigger>) {
  emit('update:modelValue', { ...props.modelValue, ...p })
}
function addKey() {
  const k = draft.value.trim()
  if (!k || props.modelValue.keys.includes(k)) { draft.value = ''; return }
  patch({ keys: [...props.modelValue.keys, k] })
  draft.value = ''
}
function removeKey(k: string) {
  patch({ keys: props.modelValue.keys.filter((x) => x !== k) })
}
</script>

<template>
  <div class="space-y-2.5" data-test="trigger-editor">
    <div class="flex items-center gap-2">
      <span class="text-[12px] text-muted">Trigger</span>
      <SegmentedControl
        :model-value="modelValue.mode"
        :options="[
          { value: 'constant', label: 'Constant' },
          { value: 'keyword', label: 'Keyword' },
          { value: 'random', label: 'Random' },
        ]"
        @update:model-value="patch({ mode: $event as Trigger['mode'] })"
      />
    </div>

    <div v-if="modelValue.mode === 'keyword'" data-test="trigger-keys">
      <div class="flex flex-wrap items-center gap-1.5 border border-line rounded-[9px] bg-white px-2.5 py-2">
        <span
          v-for="k in modelValue.keys"
          :key="k"
          class="flex items-center gap-1 bg-mauve/15 text-ink text-[12px] rounded-full pl-2.5 pr-1.5 py-0.5"
        >
          {{ k }}
          <button class="text-muted hover:text-coral" @click="removeKey(k)"><X :size="12" /></button>
        </span>
        <input
          v-model="draft"
          type="text"
          placeholder="Add keyword…"
          class="flex-1 min-w-[80px] text-[13px] bg-transparent outline-none placeholder:text-muted/60"
          @keydown.enter.prevent="addKey"
        />
      </div>
    </div>

    <div v-else-if="modelValue.mode === 'random'">
      <SliderControl
        :model-value="modelValue.probability"
        label="Probability %"
        :min="0"
        :max="100"
        :step="1"
        @update:model-value="patch({ probability: $event })"
      />
    </div>
  </div>
</template>
