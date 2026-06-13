# M3 Frontend Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Shirita Vue 3 web app (`shirita-ui/`) with the design system (proposed palette), the AppShell (top-centred nav + breadcrumb + divider), an HTTP API client, routing, and the Home chat-list screen reading the existing `GET /api/sessions`.

**Architecture:** A standalone Vite + Vue 3 + TypeScript SPA in `shirita-ui/`, talking to the existing Axum backend over HTTP. In dev, Vite proxies `/api` and `/assets` to `127.0.0.1:8787`; the bearer token comes from a Vite env var. Components are custom-styled with Tailwind v4 design tokens. Production embedding into the binary is deferred to M9.

**Tech Stack:** Vue 3, Vite 6, TypeScript, Tailwind CSS v4 (`@tailwindcss/vite`, CSS-first `@theme`), Pinia, vue-router 4, lucide-vue-next, Vitest + @vue/test-utils (jsdom).

---

## Plan series (M3 → 5 plans)

This is **Plan 1 of 5**. Spec: `docs/superpowers/specs/2026-06-13-m3-frontend-design.md`.

| Plan | Slice | Spec sections |
|------|-------|---------------|
| **1 (this)** | Scaffold + design tokens + AppShell + router + api client + Home chat list | §2, §3, §4.1, §4.2, §8 (partial) |
| 2 | Chat detail: message list (bubble/flat), composer, SSE streaming, message actions | §4.5, §10 |
| 3 | Backend templates/`prompt_nodes`/assembly upgrade + 2-step new-chat + PromptTree | §4.3, §4.4, §5, §6, §7 |
| 4 | Book editor + definitions CRUD/search + overrides | §4.6, §6.2, §7 |
| 5 | Settings: provider list, generation, custom CSS, regex, test-connection | §4.7, §6.1, §7, §9 |

**Prerequisite:** Node.js ≥ 18 and npm available (`node --version`). If absent, install before starting — it is an environment prerequisite, not a task step.

---

## File Structure (created in this plan)

```
shirita-ui/
├── package.json                 deps + scripts
├── tsconfig.json                app TS config
├── env.d.ts                     vite/import.meta.env typings
├── vite.config.ts               vue + tailwind plugins, dev proxy, vitest config
├── index.html                   SPA entry
├── .env.example                 documents VITE_API_BASE / VITE_API_TOKEN
├── src/
│   ├── main.ts                  app bootstrap (pinia + router + styles)
│   ├── App.vue                  root: <AppShell><router-view/></AppShell>
│   ├── styles.css               tailwind import + @theme palette tokens
│   ├── router/index.ts          routes (home + stubs)
│   ├── api/
│   │   ├── types.ts             Session / Message / Role types
│   │   ├── client.ts            bearer HTTP client + listSessions()
│   │   └── client.test.ts
│   ├── stores/
│   │   ├── ui.ts                messageStyle / theme (localStorage)
│   │   ├── ui.test.ts
│   │   └── sessions.ts          sessions list state + load()
│   ├── components/
│   │   ├── AppShell.vue         nav + breadcrumb + divider
│   │   ├── AppShell.test.ts
│   │   └── ChatCard.vue         one chat-list card
│   └── views/
│       ├── HomeView.vue         chat list + new-chat bubble
│       ├── HomeView.test.ts
│       ├── ChatView.vue         stub (Plan 2)
│       ├── NewChatView.vue      stub (Plan 3)
│       ├── BookView.vue         stub (Plan 4)
│       └── SettingsView.vue     stub (Plan 5)
```

Root `.gitignore` gets `shirita-ui/node_modules/`, `shirita-ui/dist/`, `shirita-ui/.env.local`.

---

## Task 1: Scaffold the Vite project

**Files:**
- Create: `shirita-ui/package.json`
- Create: `shirita-ui/tsconfig.json`
- Create: `shirita-ui/env.d.ts`
- Create: `shirita-ui/vite.config.ts`
- Create: `shirita-ui/index.html`
- Create: `shirita-ui/.env.example`
- Modify: `.gitignore`

- [ ] **Step 1: Create `shirita-ui/package.json`**

```json
{
  "name": "shirita-ui",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc -b && vite build",
    "preview": "vite preview",
    "test": "vitest run"
  },
  "dependencies": {
    "vue": "^3.5.13",
    "vue-router": "^4.5.0",
    "pinia": "^3.0.1",
    "lucide-vue-next": "^0.479.0"
  },
  "devDependencies": {
    "@tailwindcss/vite": "^4.0.9",
    "@vitejs/plugin-vue": "^5.2.1",
    "@vue/test-utils": "^2.4.6",
    "jsdom": "^26.0.0",
    "tailwindcss": "^4.0.9",
    "typescript": "^5.7.3",
    "vite": "^6.2.0",
    "vitest": "^3.0.7",
    "vue-tsc": "^2.2.4"
  }
}
```

- [ ] **Step 2: Create `shirita-ui/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ESNext",
    "useDefineForClassFields": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "jsx": "preserve",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "esModuleInterop": true,
    "lib": ["ESNext", "DOM", "DOM.Iterable"],
    "skipLibCheck": true,
    "noEmit": true,
    "types": ["vitest/globals"]
  },
  "include": ["src/**/*.ts", "src/**/*.vue", "env.d.ts"]
}
```

- [ ] **Step 3: Create `shirita-ui/env.d.ts`**

```ts
/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_API_BASE: string
  readonly VITE_API_TOKEN: string
}
interface ImportMeta {
  readonly env: ImportMetaEnv
}
```

- [ ] **Step 4: Create `shirita-ui/vite.config.ts`**

```ts
/// <reference types="vitest/config" />
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:8787',
      '/assets': 'http://127.0.0.1:8787',
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    env: { VITE_API_BASE: '', VITE_API_TOKEN: 'test-token' },
  },
})
```

- [ ] **Step 5: Create `shirita-ui/index.html`**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Shirita</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

- [ ] **Step 6: Create `shirita-ui/.env.example`**

```
# Copy to .env.local and fill in. Must match the backend TOKEN_SECRET.
VITE_API_BASE=
VITE_API_TOKEN=changeme
```

- [ ] **Step 7: Append to root `.gitignore`**

Add these lines to the existing `/home/cc/workspace/shirita/.gitignore`:

```
shirita-ui/node_modules/
shirita-ui/dist/
shirita-ui/.env.local
```

- [ ] **Step 8: Install dependencies**

Run: `cd shirita-ui && npm install`
Expected: completes without error; `shirita-ui/node_modules/` exists.

- [ ] **Step 9: Commit**

```bash
git add shirita-ui/package.json shirita-ui/package-lock.json shirita-ui/tsconfig.json shirita-ui/env.d.ts shirita-ui/vite.config.ts shirita-ui/index.html shirita-ui/.env.example .gitignore
git commit -m "chore(m3): scaffold shirita-ui Vite project"
```

---

## Task 2: Design tokens + base styles

**Files:**
- Create: `shirita-ui/src/styles.css`

- [ ] **Step 1: Create `shirita-ui/src/styles.css`**

```css
@import "tailwindcss";

@theme {
  --color-surface: #f8f7f6;
  --color-primary: #459797;
  --color-primary-strong: #3a8181;
  --color-coral: #f2a7a4;
  --color-sky: #8ed2eb;
  --color-mauve: #9f8391;
  --color-ink: #1b1b1b;
  --color-muted: #9aa0a6;
  --color-line: #e7e6e4;
}

html,
body,
#app {
  height: 100%;
}

body {
  margin: 0;
  background: var(--color-surface);
  color: var(--color-ink);
  font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto,
    "Helvetica Neue", Arial, "PingFang SC", "Microsoft YaHei", sans-serif;
}

a {
  color: inherit;
  text-decoration: none;
}
```

Note: Tailwind v4 generates utilities from the `--color-*` token names — e.g. `bg-surface`, `text-primary`, `border-line`, `text-mauve`, `bg-sky` are all available from the `@theme` block above.

- [ ] **Step 2: Commit**

```bash
git add shirita-ui/src/styles.css
git commit -m "feat(m3): palette design tokens + base styles"
```

---

## Task 3: API types and HTTP client (TDD)

**Files:**
- Create: `shirita-ui/src/api/types.ts`
- Create: `shirita-ui/src/api/client.ts`
- Test: `shirita-ui/src/api/client.test.ts`

- [ ] **Step 1: Create `shirita-ui/src/api/types.ts`**

```ts
export type Role = 'system' | 'user' | 'assistant'

export interface Session {
  id: string
  name: string
  avatar: string | null
  override_config: Record<string, unknown>
  current_state: Record<string, unknown>
  mounted_definitions: string[]
}

export interface Message {
  id: string
  session_id: string
  parent_id: string | null
  role: Role
  raw_content: string
  display_content: string | null
  is_hidden: boolean
  snapshot_state: Record<string, unknown>
  created_at: string
}
```

- [ ] **Step 2: Write the failing test `shirita-ui/src/api/client.test.ts`**

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { listSessions } from './client'
import type { Session } from './types'

describe('api client', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('listSessions GETs /api/sessions with a bearer token and parses JSON', async () => {
    const sessions: Session[] = [
      {
        id: 's1',
        name: 'Neo',
        avatar: null,
        override_config: {},
        current_state: {},
        mounted_definitions: [],
      },
    ]
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => sessions,
    })
    vi.stubGlobal('fetch', fetchMock)

    const result = await listSessions()

    expect(result).toEqual(sessions)
    expect(fetchMock).toHaveBeenCalledWith('/api/sessions', {
      headers: { Authorization: 'Bearer test-token' },
    })
  })

  it('throws on a non-ok response', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: false, status: 401 })
    vi.stubGlobal('fetch', fetchMock)

    await expect(listSessions()).rejects.toThrow('401')
  })
})
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts`
Expected: FAIL — `Failed to resolve import './client'` (file does not exist yet).

- [ ] **Step 4: Create `shirita-ui/src/api/client.ts`**

```ts
import type { Session } from './types'

const BASE = import.meta.env.VITE_API_BASE ?? ''
const TOKEN = import.meta.env.VITE_API_TOKEN ?? ''

function authHeaders(extra: Record<string, string> = {}): Record<string, string> {
  return { Authorization: `Bearer ${TOKEN}`, ...extra }
}

export async function apiGet<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}/api${path}`, { headers: authHeaders() })
  if (!res.ok) {
    throw new Error(`GET ${path} failed: ${res.status}`)
  }
  return (await res.json()) as T
}

export function listSessions(): Promise<Session[]> {
  return apiGet<Session[]>('/sessions')
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts`
Expected: PASS (2 passed).

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/api/types.ts shirita-ui/src/api/client.ts shirita-ui/src/api/client.test.ts
git commit -m "feat(m3): typed api client with bearer auth + listSessions"
```

---

## Task 4: UI store (message style + theme, persisted) (TDD)

**Files:**
- Create: `shirita-ui/src/stores/ui.ts`
- Test: `shirita-ui/src/stores/ui.test.ts`

- [ ] **Step 1: Write the failing test `shirita-ui/src/stores/ui.test.ts`**

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useUiStore } from './ui'

describe('ui store', () => {
  beforeEach(() => {
    localStorage.clear()
    setActivePinia(createPinia())
  })

  it('defaults to bubble style and system theme', () => {
    const ui = useUiStore()
    expect(ui.messageStyle).toBe('bubble')
    expect(ui.theme).toBe('system')
  })

  it('persists message style to localStorage', () => {
    const ui = useUiStore()
    ui.setMessageStyle('flat')
    expect(ui.messageStyle).toBe('flat')
    expect(localStorage.getItem('ui.messageStyle')).toBe('flat')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/stores/ui.test.ts`
Expected: FAIL — cannot resolve `./ui`.

- [ ] **Step 3: Create `shirita-ui/src/stores/ui.ts`**

```ts
import { defineStore } from 'pinia'

export type MessageStyle = 'bubble' | 'flat'
export type Theme = 'light' | 'dark' | 'system'

export const useUiStore = defineStore('ui', {
  state: () => ({
    messageStyle:
      (localStorage.getItem('ui.messageStyle') as MessageStyle) || 'bubble',
    theme: (localStorage.getItem('ui.theme') as Theme) || 'system',
  }),
  actions: {
    setMessageStyle(style: MessageStyle) {
      this.messageStyle = style
      localStorage.setItem('ui.messageStyle', style)
    },
    setTheme(theme: Theme) {
      this.theme = theme
      localStorage.setItem('ui.theme', theme)
    },
  },
})
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/stores/ui.test.ts`
Expected: PASS (2 passed).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/stores/ui.ts shirita-ui/src/stores/ui.test.ts
git commit -m "feat(m3): ui store for message style + theme"
```

---

## Task 5: Router, App root, AppShell, and view stubs (TDD for AppShell)

**Files:**
- Create: `shirita-ui/src/views/ChatView.vue`
- Create: `shirita-ui/src/views/NewChatView.vue`
- Create: `shirita-ui/src/views/BookView.vue`
- Create: `shirita-ui/src/views/SettingsView.vue`
- Create: `shirita-ui/src/router/index.ts`
- Create: `shirita-ui/src/components/AppShell.vue`
- Test: `shirita-ui/src/components/AppShell.test.ts`
- Create: `shirita-ui/src/App.vue`
- Create: `shirita-ui/src/main.ts`

- [ ] **Step 1: Create the four view stubs**

`shirita-ui/src/views/ChatView.vue`:

```vue
<template>
  <div class="max-w-[600px] mx-auto px-5 pt-8 text-muted">Chat detail — Plan 2.</div>
</template>
```

`shirita-ui/src/views/NewChatView.vue`:

```vue
<template>
  <div class="max-w-[480px] mx-auto px-5 pt-8 text-muted">New chat — Plan 3.</div>
</template>
```

`shirita-ui/src/views/BookView.vue`:

```vue
<template>
  <div class="max-w-[480px] mx-auto px-5 pt-8 text-muted">Book — Plan 4.</div>
</template>
```

`shirita-ui/src/views/SettingsView.vue`:

```vue
<template>
  <div class="max-w-[460px] mx-auto px-5 pt-8 text-muted">Settings — Plan 5.</div>
</template>
```

- [ ] **Step 2: Create `shirita-ui/src/router/index.ts`**

```ts
import { createRouter, createWebHistory } from 'vue-router'
import HomeView from '../views/HomeView.vue'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'home', component: HomeView },
    { path: '/chat/:id', name: 'chat', component: () => import('../views/ChatView.vue') },
    { path: '/new', name: 'new', component: () => import('../views/NewChatView.vue') },
    { path: '/book', name: 'book', component: () => import('../views/BookView.vue') },
    { path: '/settings', name: 'settings', component: () => import('../views/SettingsView.vue') },
  ],
})
```

Note: `HomeView.vue` is created in Task 6. The router imports it eagerly; the dev/test build will resolve once Task 6 lands. To keep the AppShell test in this task self-contained, that test builds its own router (Step 5) and does not import this file.

- [ ] **Step 3: Create `shirita-ui/src/components/AppShell.vue`**

```vue
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
  </div>
</template>
```

- [ ] **Step 4: Write the failing test `shirita-ui/src/components/AppShell.test.ts`**

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import AppShell from './AppShell.vue'

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/book', component: { template: '<div />' } },
      { path: '/settings', component: { template: '<div />' } },
    ],
  })
}

describe('AppShell', () => {
  it('renders three nav links and a slot', async () => {
    const router = makeRouter()
    router.push('/')
    await router.isReady()
    const wrapper = mount(AppShell, {
      global: { plugins: [router] },
      slots: { default: '<p>content</p>' },
    })
    expect(wrapper.findAll('nav a')).toHaveLength(3)
    expect(wrapper.text()).toContain('content')
  })

  it('marks the book section active on /book', async () => {
    const router = makeRouter()
    router.push('/book')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: [router] } })
    const bookLink = wrapper.findAll('nav a')[1]
    expect(bookLink.classes()).toContain('text-ink')
  })
})
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/AppShell.test.ts`
Expected: PASS (2 passed).

- [ ] **Step 6: Create `shirita-ui/src/App.vue`**

```vue
<script setup lang="ts">
import AppShell from './components/AppShell.vue'
</script>

<template>
  <AppShell>
    <router-view />
  </AppShell>
</template>
```

- [ ] **Step 7: Create `shirita-ui/src/main.ts`**

```ts
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { router } from './router'
import './styles.css'

createApp(App).use(createPinia()).use(router).mount('#app')
```

- [ ] **Step 8: Commit**

```bash
git add shirita-ui/src/router shirita-ui/src/components/AppShell.vue shirita-ui/src/components/AppShell.test.ts shirita-ui/src/App.vue shirita-ui/src/main.ts shirita-ui/src/views
git commit -m "feat(m3): router, App root, AppShell nav, view stubs"
```

---

## Task 6: Sessions store, ChatCard, and Home chat list (TDD for Home)

**Files:**
- Create: `shirita-ui/src/stores/sessions.ts`
- Create: `shirita-ui/src/components/ChatCard.vue`
- Create: `shirita-ui/src/views/HomeView.vue`
- Test: `shirita-ui/src/views/HomeView.test.ts`

- [ ] **Step 1: Create `shirita-ui/src/stores/sessions.ts`**

```ts
import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Session } from '../api/types'
import { listSessions } from '../api/client'

export const useSessionsStore = defineStore('sessions', () => {
  const items = ref<Session[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function load() {
    loading.value = true
    error.value = null
    try {
      items.value = await listSessions()
    } catch (e) {
      error.value = (e as Error).message
    } finally {
      loading.value = false
    }
  }

  return { items, loading, error, load }
})
```

- [ ] **Step 2: Create `shirita-ui/src/components/ChatCard.vue`**

```vue
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
```

- [ ] **Step 3: Create `shirita-ui/src/views/HomeView.vue`**

```vue
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
      class="absolute right-5 bottom-2 block"
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
```

- [ ] **Step 4: Write the failing test `shirita-ui/src/views/HomeView.test.ts`**

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import { setActivePinia, createPinia } from 'pinia'
import * as client from '../api/client'
import HomeView from './HomeView.vue'

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: HomeView },
      { path: '/chat/:id', component: { template: '<div />' } },
      { path: '/new', component: { template: '<div />' } },
    ],
  })
}

describe('HomeView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.restoreAllMocks()
  })

  it('renders a card per session from the api', async () => {
    vi.spyOn(client, 'listSessions').mockResolvedValue([
      { id: 's1', name: 'Neo', avatar: null, override_config: {}, current_state: {}, mounted_definitions: [] },
      { id: 's2', name: 'Trinity', avatar: null, override_config: {}, current_state: {}, mounted_definitions: [] },
    ])
    const router = makeRouter()
    router.push('/')
    await router.isReady()

    const wrapper = mount(HomeView, { global: { plugins: [router] } })
    await flushPromises()

    expect(wrapper.text()).toContain('Neo')
    expect(wrapper.text()).toContain('Trinity')
    expect(wrapper.findAll('a[href^="/chat/"]')).toHaveLength(2)
  })

  it('shows an empty state when there are no sessions', async () => {
    vi.spyOn(client, 'listSessions').mockResolvedValue([])
    const router = makeRouter()
    router.push('/')
    await router.isReady()

    const wrapper = mount(HomeView, { global: { plugins: [router] } })
    await flushPromises()

    expect(wrapper.text()).toContain('No conversations yet.')
  })
})
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/views/HomeView.test.ts`
Expected: PASS (2 passed).

- [ ] **Step 6: Commit**

```bash
git add shirita-ui/src/stores/sessions.ts shirita-ui/src/components/ChatCard.vue shirita-ui/src/views/HomeView.vue shirita-ui/src/views/HomeView.test.ts
git commit -m "feat(m3): sessions store + chat list home screen"
```

---

## Task 7: Full verification (type-check, tests, build) + run docs

**Files:**
- Create: `shirita-ui/README.md`

- [ ] **Step 1: Run the full test suite**

Run: `cd shirita-ui && npm run test`
Expected: all test files pass (client, ui, AppShell, HomeView).

- [ ] **Step 2: Type-check + production build**

Run: `cd shirita-ui && npm run build`
Expected: `vue-tsc` reports no type errors; Vite writes `shirita-ui/dist/` with no errors.

- [ ] **Step 3: Create `shirita-ui/README.md`**

```markdown
# shirita-ui

Vue 3 + Vite frontend for Shirita.

## Dev

1. Start the backend (from repo root): `cargo run -p shirita-web` (listens on 127.0.0.1:8787).
2. `cp shirita-ui/.env.example shirita-ui/.env.local` and set `VITE_API_TOKEN` to the backend's `TOKEN_SECRET`.
3. `cd shirita-ui && npm install && npm run dev` — open the printed URL. `/api` and `/assets` are proxied to the backend.

## Test / build

- `npm run test` — Vitest unit/component tests.
- `npm run build` — type-check + production bundle to `dist/`.

Production embedding of `dist/` into the binary is handled later (M9).
```

- [ ] **Step 4: Manual smoke check (optional but recommended)**

With the backend running and a session created (e.g. via `curl -X POST .../api/sessions -d '{"name":"Neo"}'`), run `npm run dev` and confirm the Home screen lists the session card and the teal new-chat bubble appears bottom-right of the column.

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/README.md
git commit -m "docs(m3): shirita-ui dev/test/build readme"
```

---

## Self-review notes

- **Spec coverage (this slice):** §2 design language + palette → Task 2 tokens + AppShell. §3 tech stack → Task 1. §4.1 shell → Task 5 AppShell. §4.2 chat list → Task 6 (name + avatar; last-message/time/unread intentionally omitted — backend doesn't expose them yet, per spec §4.2 / user note). §8 frontend structure (router/stores/components/api) → Tasks 3–6.
- **Deferred to later plans (not gaps):** chat detail + streaming (Plan 2), new-chat/template tree (Plan 3), book (Plan 4), settings (Plan 5). Routes to those are live stubs so navigation works now.
- **Type consistency:** `Session`/`Message` shapes mirror the Rust serde output verified in `shirita-core/src/models/{session,message}.rs`. `listSessions()` is the single name used by client, store, and tests.
- **No new backend** in this plan — it consumes the existing `GET /api/sessions` only.
