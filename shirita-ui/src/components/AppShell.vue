<script setup lang="ts">
import { computed, watch } from 'vue'
import { useRoute } from 'vue-router'
import { MessageCircle, BookOpen, Settings, ChevronRight } from 'lucide-vue-next'
import { useUiStore } from '../stores/ui'
import { assetUrl } from '../api/client'
import logoUrl from '../assets/favicon.svg'

const ui = useUiStore()
const route = useRoute()
const bgStyle = computed(() =>
  ui.background ? { backgroundImage: `url(${assetUrl(ui.background)})` } : { backgroundColor: 'var(--color-surface, #f8f7f6)' },
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
  <div data-app="shell" class="h-full">
    <!-- app-wide background image + scrim (full viewport, fixed) -->
    <div class="fixed inset-0 -z-10 bg-cover bg-center" :style="bgStyle" />
    <div class="fixed inset-0 -z-10 bg-surface/30" />

    <!-- centered app panel: header + content together over the background -->
    <div class="mx-auto h-full flex flex-col bg-surface/85" :style="{ maxWidth: ui.contentWidth + 'px' }">
      <header>
        <div class="grid grid-cols-[1fr_auto_1fr] items-center px-6 pt-4 pb-1.5">
          <div class="flex items-center gap-2 min-w-0">
            <router-link
              to="/"
              data-test="brand"
              class="w-7 h-7 rounded-lg overflow-hidden grid place-items-center shrink-0"
            >
              <img :src="logoUrl" alt="Shirita" class="w-7 h-7 object-cover" />
            </router-link>
            <!-- breadcrumbs: in header on desktop, inside <main> on mobile -->
            <span class="max-sm:hidden flex items-center gap-1.5 truncate">
              <template v-for="(c, i) in crumbs" :key="i">
                <ChevronRight :size="13" class="text-muted/50 shrink-0" />
                <router-link v-if="c.to" :to="c.to" class="text-[13px] text-muted hover:text-ink truncate">{{ $t(c.label) }}</router-link>
                <span v-else class="text-[13px] text-ink truncate">{{ $t(c.label) }}</span>
              </template>
            </span>
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
          <div />
        </div>
        <div class="flex justify-center"><div class="h-px w-[170px] bg-line" /></div>
      </header>
      <main class="flex-1 min-h-0 overflow-y-auto px-8">
        <!-- mobile breadcrumbs: inside main content, below header -->
        <div v-if="crumbs.length" class="sm:hidden flex items-center gap-1.5 pt-3 pb-1">
          <template v-for="(c, i) in crumbs" :key="i">
            <ChevronRight :size="13" class="text-muted/50 shrink-0" />
            <router-link v-if="c.to" :to="c.to" class="text-[13px] text-muted hover:text-ink whitespace-nowrap">{{ $t(c.label) }}</router-link>
            <span v-else class="text-[13px] text-ink whitespace-nowrap">{{ $t(c.label) }}</span>
          </template>
        </div>
        <slot />
      </main>
    </div>
  </div>
</template>
