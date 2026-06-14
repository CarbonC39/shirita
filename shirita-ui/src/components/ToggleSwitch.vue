<script setup lang="ts">
defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()
</script>

<template>
  <button
    type="button"
    role="switch"
    :aria-checked="modelValue"
    data-test="toggle"
    :class="['relative w-10 h-[23px] rounded-full shrink-0 transition-colors', modelValue ? 'bg-primary' : 'bg-line']"
    @click="emit('update:modelValue', !modelValue)"
  >
    <span
      :class="['absolute top-[2px] w-[19px] h-[19px] rounded-full bg-white shadow-sm transition-all', modelValue ? 'left-[19px]' : 'left-[2px]']"
    />
  </button>
</template>
