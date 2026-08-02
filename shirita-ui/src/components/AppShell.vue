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

// The chat route is a workspace: the transcript owns scrolling, so the host
// must not scroll. Every other route scrolls in the host.
const isChat = computed(() => route.name === 'chat')
const layoutMode = computed(() => (isChat.value ? 'workspace' : 'page'))

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
// Mobile breadcrumbs live in the page's own scrolling content; the chat route
// gets a compact in-bar header instead of a second permanent row.
const showMobileCrumbs = computed(() => crumbs.value.length > 0 && !isChat.value)
</script>

<template>
  <div data-app="shell" class="app-shell">
    <!-- app-wide background image + scrim (full viewport, fixed) -->
    <div class="fixed inset-0 -z-10 bg-cover bg-center" :style="bgStyle" />
    <div class="fixed inset-0 -z-10 bg-surface/30" />

    <!-- centered app panel: top bar + route host together over the background -->
    <div class="mx-auto flex flex-col h-dvh bg-surface/85" :style="{ maxWidth: ui.contentWidth + 'px' }">
      <header class="app-topbar">
        <div class="flex items-center justify-between gap-2 px-3 sm:px-6 pt-[calc(env(safe-area-inset-top)+0.5rem)] pb-2 min-w-0">
          <router-link
            to="/"
            data-test="brand"
            class="w-7 h-7 rounded-lg overflow-hidden grid place-items-center shrink-0"
          >
            <img :src="logoUrl" alt="Shirita" class="w-7 h-7 object-cover" />
          </router-link>
          <!-- breadcrumbs: inline in the top bar on desktop (truncating, never
               displacing nav); hidden on mobile (mobile crumbs live in content) -->
          <span class="max-sm:hidden flex items-center gap-1.5 truncate min-w-0">
            <template v-for="(c, i) in crumbs" :key="i">
              <ChevronRight :size="13" class="text-muted/50 shrink-0" />
              <router-link v-if="c.to" :to="c.to" class="text-[13px] text-muted hover:text-ink truncate">{{ $t(c.label) }}</router-link>
              <span v-else class="text-[13px] text-ink truncate">{{ $t(c.label) }}</span>
            </template>
          </span>
          <nav class="flex items-center gap-3 sm:gap-6 shrink-0" aria-label="Primary">
            <router-link :to="chatTo" :aria-label="$t('shell.chats')" :aria-current="section === 'chat' ? 'page' : undefined" :class="['inline-flex items-center p-1.5 -m-1.5', section === 'chat' ? 'text-ink' : 'text-muted hover:text-ink']">
              <MessageCircle :size="22" :stroke-width="1.8" />
            </router-link>
            <router-link to="/book" :aria-label="$t('shell.book')" :aria-current="section === 'book' ? 'page' : undefined" :class="['inline-flex items-center p-1.5 -m-1.5', section === 'book' ? 'text-ink' : 'text-muted hover:text-ink']">
              <BookOpen :size="22" :stroke-width="1.8" />
            </router-link>
            <router-link to="/settings" :aria-label="$t('shell.settings')" :aria-current="section === 'settings' ? 'page' : undefined" :class="['inline-flex items-center p-1.5 -m-1.5', section === 'settings' ? 'text-ink' : 'text-muted hover:text-ink']">
              <Settings :size="22" :stroke-width="1.8" />
            </router-link>
          </nav>
        </div>
      </header>
      <!-- Route host: ordinary pages scroll; the chat workspace does not. -->
      <main
        data-test="route-host"
        :data-layout="layoutMode"
        :class="['flex-1 min-h-0 px-0 sm:px-2', layoutMode === 'page' ? 'overflow-y-auto' : 'overflow-hidden']"
      >
        <!-- mobile breadcrumbs: inside the page's scrolling content, below the bar -->
        <div v-if="showMobileCrumbs" data-test="mobile-crumbs" class="sm:hidden flex items-center gap-1.5 pt-3 pb-1 px-3">
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
