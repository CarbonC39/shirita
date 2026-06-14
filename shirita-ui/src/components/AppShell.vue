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
  <div class="h-full flex flex-col">
    <header>
      <div class="flex items-center justify-between px-6 pt-4 pb-1.5">
        <div class="min-w-[120px]">
          <div class="w-7 h-7 rounded-lg bg-ink text-white grid place-items-center font-bold text-sm">
            S
          </div>
        </div>
        <nav class="flex items-center gap-8">
          <router-link to="/" :class="['transition-colors', section === 'chat' ? 'text-ink [&_svg]:stroke-[2.5]' : 'text-mauve/30 hover:text-mauve/50']">
            <MessageCircle :size="22" :stroke-width="section === 'chat' ? 2.5 : 1.5" />
          </router-link>
          <router-link to="/book" :class="['transition-colors', section === 'book' ? 'text-ink [&_svg]:stroke-[2.5]' : 'text-mauve/30 hover:text-mauve/50']">
            <BookOpen :size="22" :stroke-width="section === 'book' ? 2.5 : 1.5" />
          </router-link>
          <router-link to="/settings" :class="['transition-colors', section === 'settings' ? 'text-ink [&_svg]:stroke-[2.5]' : 'text-mauve/30 hover:text-mauve/50']">
            <Settings :size="22" :stroke-width="section === 'settings' ? 2.5 : 1.5" />
          </router-link>
        </nav>
        <div class="min-w-[120px]" />
      </div>
      <div class="flex justify-center"><div class="h-px w-[170px] bg-line" /></div>
    </header>
    <main class="flex-1 min-h-0 overflow-hidden">
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
