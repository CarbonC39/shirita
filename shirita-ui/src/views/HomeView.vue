<script setup lang="ts">
import { onMounted } from 'vue'
import { useSessionsStore } from '../stores/sessions'
import ChatCard from '../components/ChatCard.vue'

const store = useSessionsStore()
onMounted(() => store.load())
</script>

<template>
  <div class="relative max-w-[560px] mx-auto px-5 pt-7 pb-8 min-h-[70vh]">
    <p v-if="store.loading" class="text-muted text-sm">Loading…</p>
    <p v-else-if="store.error" class="text-coral text-sm">{{ store.error }}</p>
    <p v-else-if="store.items.length === 0" class="text-muted text-sm">
      No conversations yet.
    </p>
    <ChatCard v-for="s in store.items" :key="s.id" :session="s" />

    <router-link
      to="/new"
      aria-label="New chat"
      class="fixed right-5 bottom-6 block z-20"
    >
      <svg
        width="54"
        height="54"
        viewBox="0 0 24 24"
        style="transform: scaleX(-1); filter: drop-shadow(0 7px 16px rgba(0, 0, 0, 0.18))"
      >
        <path fill="var(--color-primary)" d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" />
        <line x1="8" y1="12" x2="16" y2="12" stroke="#fff" stroke-width="2.2" stroke-linecap="round" />
        <line x1="12" y1="8" x2="12" y2="16" stroke="#fff" stroke-width="2.2" stroke-linecap="round" />
      </svg>
    </router-link>
  </div>
</template>
