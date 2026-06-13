<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { MessageCircle, BookOpen, Settings } from 'lucide-vue-next'

const route = useRoute()
const section = computed(() => {
  if (route.path.startsWith('/book')) return 'book'
  if (route.path.startsWith('/settings')) return 'settings'
  return 'chat'
})
</script>

<template>
  <div class="min-h-full flex flex-col">
    <header>
      <div class="flex items-center justify-between px-6 pt-4 pb-1.5">
        <div class="min-w-[120px]">
          <div class="w-7 h-7 rounded-lg bg-ink text-white grid place-items-center font-bold text-sm">
            S
          </div>
        </div>
        <nav class="flex items-center gap-8">
          <router-link to="/" :class="section === 'chat' ? 'text-ink' : 'text-mauve/60'">
            <MessageCircle :size="22" />
          </router-link>
          <router-link to="/book" :class="section === 'book' ? 'text-ink' : 'text-mauve/60'">
            <BookOpen :size="22" />
          </router-link>
          <router-link to="/settings" :class="section === 'settings' ? 'text-ink' : 'text-mauve/60'">
            <Settings :size="22" />
          </router-link>
        </nav>
        <div class="min-w-[120px]" />
      </div>
      <div class="flex justify-center"><div class="h-px w-[170px] bg-line" /></div>
    </header>
    <main class="flex-1">
      <slot />
    </main>
    <footer class="text-center py-3 text-xs text-muted">
      <a
        href="https://github.com/CarbonC39/shirita"
        target="_blank"
        rel="noopener noreferrer"
        class="hover:text-primary transition-colors"
      >Shirita</a>
    </footer>
  </div>
</template>
