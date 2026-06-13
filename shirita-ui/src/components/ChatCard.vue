<script setup lang="ts">
import type { Session } from '../api/types'

defineProps<{ session: Session }>()
</script>

<template>
  <router-link
    :to="`/chat/${session.id}`"
    class="flex items-center gap-3.5 bg-white border border-line rounded-2xl px-4 py-3.5 mb-3"
  >
    <div class="w-11 h-11 rounded-full bg-sky/30 shrink-0 overflow-hidden">
      <img
        v-if="session.avatar"
        :src="`/assets/${session.avatar}`"
        class="w-full h-full object-cover"
        alt=""
      />
    </div>
    <div class="flex-1 min-w-0">
      <div class="font-semibold text-ink truncate">{{ session.name }}</div>
      <div class="text-[13px] text-muted truncate">Tap to open</div>
    </div>
  </router-link>
</template>
