<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{ label: string; modelValue: number; min: number; max: number; step?: number }>()
const emit = defineEmits<{ 'update:modelValue': [value: number] }>()

const step = computed(() => props.step ?? 0.01)

const pct = computed(() => {
  const p = ((props.modelValue - props.min) / (props.max - props.min)) * 100
  return Math.max(0, Math.min(100, p))
})

const trackStyle = computed(() => ({
  background: `linear-gradient(to right, var(--color-primary) 0%, var(--color-primary) ${pct.value}%, var(--color-line) ${pct.value}%, var(--color-line) 100%)`,
}))

function clamp(v: number) {
  if (Number.isNaN(v)) return props.min
  return Math.max(props.min, Math.min(props.max, v))
}

function onNumberInput(e: Event) {
  const raw = parseFloat((e.target as HTMLInputElement).value)
  emit('update:modelValue', clamp(raw))
}
</script>

<template>
  <div class="mb-4">
    <div class="flex items-center justify-between mb-2">
      <label class="text-[13.5px] text-ink">{{ label }}</label>
      <input
        type="number"
        :value="modelValue"
        :min="min"
        :max="max"
        :step="step"
        class="w-[72px] border border-line rounded-lg px-2 py-1 text-[13px] text-right tabular-nums bg-card outline-none focus:border-primary/50"
        @input="onNumberInput"
      />
    </div>
    <input
      type="range"
      :value="modelValue"
      :min="min"
      :max="max"
      :step="step"
      :style="trackStyle"
      class="w-full h-1 rounded-full appearance-none cursor-pointer
             [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-[15px] [&::-webkit-slider-thumb]:h-[15px] [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary [&::-webkit-slider-thumb]:border-0 [&::-webkit-slider-thumb]:cursor-pointer
             [&::-moz-range-thumb]:w-[15px] [&::-moz-range-thumb]:h-[15px] [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:bg-primary [&::-moz-range-thumb]:border-0"
      @input="emit('update:modelValue', parseFloat(($event.target as HTMLInputElement).value))"
    />
  </div>
</template>
