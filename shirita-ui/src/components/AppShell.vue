<script setup lang="ts">
import { computed, watch } from 'vue'
import { useRoute } from 'vue-router'
import { MessageCircle, BookOpen, Settings, ChevronRight } from 'lucide-vue-next'
import { useUiStore } from '../stores/ui'

const ui = useUiStore()
const route = useRoute()
const bgStyle = computed(() =>
  ui.background ? { backgroundImage: `url(/assets/${ui.background})` } : { backgroundColor: 'var(--color-surface, #f8f7f6)' },
)
const section = computed(() => {
  if (route.path.startsWith('/book')) return 'book'
  if (route.path.startsWith('/settings')) return 'settings'
  return 'chat'
})

// Remember the conversation you're "in": set on entering a chat, kept while you
// browse Book/Settings, cleared when you return to the list. So the Chat icon
// reopens that conversation from anywhere — until you leave it for home.
const activeChatId = computed(() => ui.activeChatId)
watch(
  () => route.fullPath,
  () => {
    if (route.name === 'chat') ui.setActiveChatId(route.params.id as string)
    else if (route.path === '/') ui.setActiveChatId(null)
  },
  { immediate: true },
)
const chatTo = computed(() => (activeChatId.value ? `/chat/${activeChatId.value}` : '/'))

type Crumb = { label: string; to?: string }
const crumbs = computed(() => (route.meta.crumbs as Crumb[] | undefined) ?? [])
</script>

<template>
  <div data-app="shell" class="h-full flex flex-col">
    <!-- app-wide background (image + scrim always present, even without custom image) -->
    <div class="fixed inset-0 -z-10 bg-cover bg-center" :style="bgStyle" />
    <div class="fixed inset-0 -z-10 bg-surface/30" />
    <header>
      <div class="flex items-center justify-between px-6 pt-4 pb-1.5">
        <div class="flex items-center gap-2 min-w-[120px]">
          <router-link to="/" class="w-7 h-7 rounded-lg bg-primary text-white grid place-items-center font-bold text-sm shrink-0">
            S
          </router-link>
          <template v-for="(c, i) in crumbs" :key="i">
            <ChevronRight :size="13" class="text-muted/50 shrink-0" />
            <router-link v-if="c.to" :to="c.to" class="text-[13px] text-muted hover:text-ink whitespace-nowrap">{{ $t(c.label) }}</router-link>
            <span v-else class="text-[13px] text-ink whitespace-nowrap">{{ $t(c.label) }}</span>
          </template>
        </div>
        <nav class="flex items-center gap-8">
          <router-link :to="chatTo" :class="['transition-colors duration-200', section === 'chat' ? 'text-ink' : 'text-muted hover:text-ink']">
            <MessageCircle :size="22" :stroke-width="1.8" />
          </router-link>
          <router-link to="/book" :class="['transition-colors duration-200', section === 'book' ? 'text-ink' : 'text-muted hover:text-ink']">
            <BookOpen :size="22" :stroke-width="1.8" />
          </router-link>
          <router-link to="/settings" :class="['transition-colors duration-200', section === 'settings' ? 'text-ink' : 'text-muted hover:text-ink']">
            <Settings :size="22" :stroke-width="1.8" />
          </router-link>
        </nav>
        <div class="min-w-[120px]" />
      </div>
      <div class="flex justify-center"><div class="h-px w-[170px] bg-line" /></div>
    </header>
    <main class="flex-1 min-h-0 overflow-y-auto">
      <slot />
    </main>
  </div>
</template>
