<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{ label: string; modelValue: number; min: number; max: number; step?: number }>()
const emit = defineEmits<{ 'update:modelValue': [value: number] }>()

const pct = computed(() => {
  const p = ((props.modelValue - props.min) / (props.max - props.min)) * 100
  return Math.max(0, Math.min(100, p))
})

const trackStyle = computed(() => ({
  background: `linear-gradient(to right, var(--color-primary) 0%, var(--color-primary) ${pct.value}%, var(--color-line) ${pct.value}%, var(--color-line) 100%)`,
}))
</script>

<template>
  <div class="mb-4">
    <div class="flex items-center justify-between mb-2">
      <label class="text-[13.5px] text-ink">{{ label }}</label>
      <span class="text-[13px] text-muted tabular-nums">{{ modelValue }}</span>
    </div>
    <input
      type="range"
      :value="modelValue"
      :min="min"
      :max="max"
      :step="step || 0.01"
      :style="trackStyle"
      class="w-full h-1 rounded-full appearance-none cursor-pointer
             [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-[15px] [&::-webkit-slider-thumb]:h-[15px] [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary [&::-webkit-slider-thumb]:border-0 [&::-webkit-slider-thumb]:cursor-pointer
             [&::-moz-range-thumb]:w-[15px] [&::-moz-range-thumb]:h-[15px] [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:bg-primary [&::-moz-range-thumb]:border-0"
      @input="emit('update:modelValue', parseFloat(($event.target as HTMLInputElement).value))"
    />
  </div>
</template>
