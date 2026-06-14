<script setup lang="ts">
defineProps<{ modelValue: string; options: { value: string; label: string }[] }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
</script>

<template>
  <div class="inline-flex bg-line/60 rounded-[9px] p-[3px]" data-test="segmented">
    <button
      v-for="opt in options"
      :key="opt.value"
      type="button"
      :class="['text-[13px] rounded-[7px] px-3.5 py-1 transition-colors',
               modelValue === opt.value ? 'bg-card text-ink shadow-sm dark:ring-1 dark:ring-inset dark:ring-white/10' : 'text-muted hover:text-ink']"
      @click="emit('update:modelValue', opt.value)"
    >{{ opt.label }}</button>
  </div>
</template>
