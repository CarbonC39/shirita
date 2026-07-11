<script setup lang="ts">
import { useToast } from '../composables/useToast'
const { toast, dismiss } = useToast()
</script>

<template>
  <Transition name="expand">
    <button
      v-if="toast"
      type="button"
      class="fixed left-1/2 -translate-x-1/2 bottom-[calc(env(safe-area-inset-bottom)+1rem)] z-[60] max-w-[90vw] px-4 py-2 rounded-full text-[13px] font-medium shadow-lg"
      :class="toast.kind === 'error' ? 'bg-coral text-white' : 'bg-ink text-surface'"
      @click="dismiss"
    >{{ toast.message }}</button>
  </Transition>
</template>
