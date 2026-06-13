<script setup lang="ts">
defineProps<{ label: string; modelValue: number; min: number; max: number; step?: number }>()
const emit = defineEmits<{ 'update:modelValue': [value: number] }>()
</script>

<template>
  <div class="mb-4">
    <div class="flex items-center justify-between mb-1">
      <label class="text-[13px] text-ink">{{ label }}</label>
      <span class="text-[13px] text-muted tabular-nums">{{ modelValue }}</span>
    </div>
    <input type="range" :value="modelValue" :min="min" :max="max" :step="step || 0.01"
      class="w-full h-1.5 rounded-full bg-line appearance-none cursor-pointer accent-primary [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:h-4 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary [&::-webkit-slider-thumb]:appearance-none"
      @input="emit('update:modelValue', parseFloat(($event.target as HTMLInputElement).value))" />
  </div>
</template>
