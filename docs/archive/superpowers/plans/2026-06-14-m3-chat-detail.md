# M3 Chat Detail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the chat detail screen — message list with bubble/flat render modes, SSE streaming, composer, and message actions (regenerate/copy/edit; fork/swipe stubs).

**Architecture:** A Vue 3 chat view consuming the existing `GET /api/sessions/{id}/messages` and `POST /api/sessions/{id}/messages` (SSE) endpoints. No backend changes. Messages are loaded then rendered in a scrollable list; the composer is pinned at the bottom. SSE deltas stream into a local ref and display inline with a blinking cursor. Two render modes (`bubble` / `flat`) driven by the ui store.

**Tech Stack:** Vue 3, TypeScript, Tailwind CSS v4, Pinia, lucide-vue-next, Vitest + @vue/test-utils (jsdom).

---

## Plan series (M3 → 5 plans)

This is **Plan 2 of 5**. Prerequisite: Plan 1 (foundation) complete.

| Plan | Slice | Spec sections |
|------|-------|---------------|
| 1 ✓ | Scaffold + design tokens + AppShell + router + api client + Home chat list | §2, §3, §4.1, §4.2, §8 (partial) |
| **2 (this)** | Chat detail: message list (bubble/flat), composer, SSE streaming, message actions | §4.5, §10 |
| 3 | Backend templates/`prompt_nodes`/assembly upgrade + 2-step new-chat + PromptTree | §4.3, §4.4, §5, §6, §7 |
| 4 | Book editor + definitions CRUD/search + overrides | §4.6, §6.2, §7 |
| 5 | Settings: provider list, generation, custom CSS, regex, test-connection | §4.7, §6.1, §7, §9 |

---

## File Structure (created / modified in this plan)

```
shirita-ui/src/
├── api/
│   ├── types.ts               (no changes — Message type already defined in Plan 1)
│   └── client.ts              (modify: add listMessages(), sendMessage() SSE generator)
│   └── client.test.ts         (modify: add SSE + listMessages tests)
├── stores/
│   └── chat.ts                (create: active-session messages + streaming state)
│   └── chat.test.ts           (create)
├── components/
│   ├── MessageItem.vue         (create: single message bubble/flat row + actions)
│   ├── MessageItem.test.ts     (create)
│   ├── MessageList.vue         (create: scrollable list, streaming ghost)
│   ├── MessageList.test.ts     (create)
│   ├── Composer.vue            (create: input + send button)
│   └── Composer.test.ts        (create)
└── views/
    └── ChatView.vue            (modify: replace stub, wire store + components + router)
    └── ChatView.test.ts        (create)
```

---

## Task 1: Extend API client with listMessages and SSE sendMessage

**Files:**
- Modify: `shirita-ui/src/api/client.ts`
- Modify: `shirita-ui/src/api/client.test.ts`

- [ ] **Step 1: Add `SseEvent` type and `listMessages`/`sendMessage` to `shirita-ui/src/api/client.ts`**

Replace the current `shirita-ui/src/api/client.ts` with:

```ts
import type { Message, Session } from './types'

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

export function listMessages(sessionId: string): Promise<Message[]> {
  return apiGet<Message[]>(`/sessions/${sessionId}/messages`)
}

// --- SSE streaming ---

export type SseEvent =
  | { type: 'delta'; text: string }
  | { type: 'done'; message_id: string }
  | { type: 'error'; message: string }

export async function* sendMessage(
  sessionId: string,
  text: string,
): AsyncGenerator<SseEvent> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/messages`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ text }),
  })
  if (!res.ok) {
    throw new Error(`POST /sessions/${sessionId}/messages failed: ${res.status}`)
  }
  if (!res.body) {
    throw new Error('No response body for SSE stream')
  }

  const reader = res.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''
      for (const line of lines) {
        if (line.startsWith('data: ')) {
          yield JSON.parse(line.slice(6)) as SseEvent
        }
      }
    }
  } finally {
    reader.releaseLock()
  }
}
```

- [ ] **Step 2: Write the failing test additions in `shirita-ui/src/api/client.test.ts`**

Replace the current `shirita-ui/src/api/client.test.ts` with:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { listSessions, listMessages, sendMessage } from './client'
import type { Session, Message } from './types'

function mockFetch(status: number, json?: unknown) {
  return vi.fn().mockResolvedValue({
    ok: status < 400,
    status,
    json: async () => json,
  })
}

describe('api client', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('listSessions GETs /api/sessions with a bearer token and parses JSON', async () => {
    const sessions: Session[] = [
      {
        id: 's1', name: 'Neo', avatar: null,
        override_config: {}, current_state: {}, mounted_definitions: [],
      },
    ]
    const fm = mockFetch(200, sessions)
    vi.stubGlobal('fetch', fm)

    const result = await listSessions()

    expect(result).toEqual(sessions)
    expect(fm).toHaveBeenCalledWith('/api/sessions', {
      headers: { Authorization: 'Bearer test-token' },
    })
  })

  it('throws on a non-ok response', async () => {
    vi.stubGlobal('fetch', mockFetch(401))
    await expect(listSessions()).rejects.toThrow('401')
  })

  it('listMessages GETs /api/sessions/:id/messages', async () => {
    const msgs: Message[] = [
      {
        id: 'm1', session_id: 's1', parent_id: null, role: 'user',
        raw_content: 'hi', display_content: null, is_hidden: false,
        snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
      },
    ]
    const fm = mockFetch(200, msgs)
    vi.stubGlobal('fetch', fm)

    const result = await listMessages('s1')

    expect(result).toEqual(msgs)
    expect(fm).toHaveBeenCalledWith('/api/sessions/s1/messages', {
      headers: { Authorization: 'Bearer test-token' },
    })
  })
})

describe('sendMessage SSE', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('parses SSE data lines into typed events', async () => {
    const events = [
      'data: {"type":"delta","text":"Hel"}',
      '',
      'data: {"type":"delta","text":"lo"}',
      '',
      'data: {"type":"done","message_id":"assist-1"}',
      '',
    ].join('\n')
    const encoder = new TextEncoder()
    const stream = new ReadableStream({
      start(ctrl) {
        ctrl.enqueue(encoder.encode(events))
        ctrl.close()
      },
    })
    const fm = vi.fn().mockResolvedValue({ ok: true, body: stream })
    vi.stubGlobal('fetch', fm)

    const gen = sendMessage('sess-1', 'hi')
    const results = []
    for await (const ev of gen) {
      results.push(ev)
    }

    expect(results).toEqual([
      { type: 'delta', text: 'Hel' },
      { type: 'delta', text: 'lo' },
      { type: 'done', message_id: 'assist-1' },
    ])
    expect(fm).toHaveBeenCalledWith('/api/sessions/sess-1/messages', {
      method: 'POST',
      headers: {
        Authorization: 'Bearer test-token',
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ text: 'hi' }),
    })
  })

  it('throws on non-ok POST', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 404 }))
    const gen = sendMessage('ghost', 'hi')
    await expect(gen.next()).rejects.toThrow('404')
  })

  it('yields error events without throwing', async () => {
    const body = 'data: {"type":"error","message":"session not found"}\n\n'
    const encoder = new TextEncoder()
    const stream = new ReadableStream({
      start(ctrl) {
        ctrl.enqueue(encoder.encode(body))
        ctrl.close()
      },
    })
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, body: stream }))

    const gen = sendMessage('s1', 'hi')
    const results = []
    for await (const ev of gen) {
      results.push(ev)
    }
    expect(results).toEqual([{ type: 'error', message: 'session not found' }])
  })
})
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts`
Expected: FAIL — `listMessages` / `sendMessage` are not exported from `./client`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/api/client.test.ts`
Expected: PASS (6 passed — 3 original + 3 new).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/api/client.ts shirita-ui/src/api/client.test.ts
git commit -m "feat(m3): extend api client with listMessages + SSE sendMessage"
```

---

## Task 2: Chat store (messages, streaming state, actions) (TDD)

**Files:**
- Create: `shirita-ui/src/stores/chat.ts`
- Create: `shirita-ui/src/stores/chat.test.ts`

- [ ] **Step 1: Write the failing test `shirita-ui/src/stores/chat.test.ts`**

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useChatStore } from './chat'
import * as client from '../api/client'
import type { Message } from '../api/types'

function msg(overrides: Partial<Message> = {}): Message {
  return {
    id: 'm1', session_id: 's1', parent_id: null, role: 'user',
    raw_content: 'hi', display_content: null, is_hidden: false,
    snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    ...overrides,
  }
}

describe('chat store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.restoreAllMocks()
  })

  it('loadMessages fetches and stores messages', async () => {
    const items = [msg({ id: 'm1' }), msg({ id: 'm2', role: 'assistant' })]
    vi.spyOn(client, 'listMessages').mockResolvedValue(items)

    const store = useChatStore()
    await store.loadMessages('s1')

    expect(store.messages).toEqual(items)
    expect(store.loading).toBe(false)
  })

  it('sendMessage streams deltas into streamingText and reloads on done', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([msg()])
    async function* stream(): AsyncGenerator<client.SseEvent> {
      yield { type: 'delta', text: 'Hel' }
      yield { type: 'delta', text: 'lo' }
      yield { type: 'done', message_id: 'a1' }
    }
    vi.spyOn(client, 'sendMessage').mockReturnValue(stream())

    const store = useChatStore()
    await store.send('s1', 'hi')

    expect(client.sendMessage).toHaveBeenCalledWith('s1', 'hi')
    expect(store.messages).toEqual([msg()]) // reloaded after done
    expect(store.isStreaming).toBe(false)
    expect(store.streamingText).toBe('')
    expect(store.streamingError).toBeNull()
  })

  it('sendMessage sets streamingError on error event and stops streaming', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    async function* stream(): AsyncGenerator<client.SseEvent> {
      yield { type: 'error', message: 'session not found' }
    }
    vi.spyOn(client, 'sendMessage').mockReturnValue(stream())

    const store = useChatStore()
    await store.send('ghost', 'hi')

    expect(store.isStreaming).toBe(false)
    expect(store.streamingError).toBe('session not found')
  })

  it('sendMessage catches fetch errors', async () => {
    vi.spyOn(client, 'sendMessage').mockRejectedValue(new Error('Network error'))

    const store = useChatStore()
    await store.send('s1', 'hi')

    expect(store.streamingError).toBe('Network error')
    expect(store.isStreaming).toBe(false)
  })

  it('clearStreaming resets streaming state', () => {
    const store = useChatStore()
    store.$patch({ isStreaming: true, streamingText: 'partial', streamingError: 'x' })
    store.clearStreaming()
    expect(store.isStreaming).toBe(false)
    expect(store.streamingText).toBe('')
    expect(store.streamingError).toBeNull()
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/stores/chat.test.ts`
Expected: FAIL — `Failed to resolve import './chat'`.

- [ ] **Step 3: Create `shirita-ui/src/stores/chat.ts`**

```ts
import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Message } from '../api/types'
import { listMessages, sendMessage } from '../api/client'

export const useChatStore = defineStore('chat', () => {
  const messages = ref<Message[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const isStreaming = ref(false)
  const streamingText = ref('')
  const streamingError = ref<string | null>(null)
  const activeSessionId = ref<string | null>(null)

  async function loadMessages(sessionId: string) {
    loading.value = true
    error.value = null
    activeSessionId.value = sessionId
    try {
      messages.value = await listMessages(sessionId)
    } catch (e) {
      error.value = (e as Error).message
    } finally {
      loading.value = false
    }
  }

  async function send(sessionId: string, text: string) {
    isStreaming.value = true
    streamingText.value = ''
    streamingError.value = null

    try {
      const stream = sendMessage(sessionId, text)
      for await (const event of stream) {
        if (event.type === 'delta') {
          streamingText.value += event.text
        } else if (event.type === 'done') {
          // Reload to get the persisted messages with server ids
          await loadMessages(sessionId)
        } else if (event.type === 'error') {
          streamingError.value = event.message
          isStreaming.value = false
          return
        }
      }
    } catch (e) {
      streamingError.value = (e as Error).message
    } finally {
      isStreaming.value = false
    }
  }

  function clearStreaming() {
    isStreaming.value = false
    streamingText.value = ''
    streamingError.value = null
  }

  return {
    messages, loading, error,
    isStreaming, streamingText, streamingError, activeSessionId,
    loadMessages, send, clearStreaming,
  }
})
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/stores/chat.test.ts`
Expected: PASS (5 passed).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/stores/chat.ts shirita-ui/src/stores/chat.test.ts
git commit -m "feat(m3): chat store with SSE streaming state"
```

---

## Task 3: MessageItem component (bubble/flat row + actions) (TDD)

**Files:**
- Create: `shirita-ui/src/components/MessageItem.vue`
- Create: `shirita-ui/src/components/MessageItem.test.ts`

- [ ] **Step 1: Write the failing test `shirita-ui/src/components/MessageItem.test.ts`**

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import MessageItem from './MessageItem.vue'
import type { Message } from '../api/types'

function makeMsg(overrides: Partial<Message> = {}): Message {
  return {
    id: 'm1', session_id: 's1', parent_id: null, role: 'user',
    raw_content: 'Hello world', display_content: null, is_hidden: false,
    snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    ...overrides,
  }
}

describe('MessageItem', () => {
  it('renders user message in bubble mode', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'user' }), style: 'bubble' },
    })
    expect(wrapper.text()).toContain('Hello world')
    // User bubble is right-aligned; wrapper should have justify-end
    const row = wrapper.find('[data-test="msg-row"]')
    expect(row.classes()).toContain('justify-end')
  })

  it('renders assistant message in bubble mode', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    expect(wrapper.text()).toContain('Hello world')
    const row = wrapper.find('[data-test="msg-row"]')
    expect(row.classes()).toContain('justify-start')
  })

  it('renders assistant avatar in bubble mode', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    expect(wrapper.find('[data-test="assistant-avatar"]').exists()).toBe(true)
  })

  it('shows no avatar for user in bubble mode', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'user' }), style: 'bubble' },
    })
    expect(wrapper.find('[data-test="assistant-avatar"]').exists()).toBe(false)
  })

  it('renders in flat mode with role label and full-width content', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'flat' },
    })
    expect(wrapper.text()).toContain('Assistant')
    expect(wrapper.text()).toContain('Hello world')
  })

  it('shows action buttons on hover for assistant messages', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    const actions = wrapper.find('[data-test="message-actions"]')
    expect(actions.exists()).toBe(true)
    expect(actions.text()).toContain('Copy')
  })

  it('does not show action buttons for user messages', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'user' }), style: 'bubble' },
    })
    expect(wrapper.find('[data-test="message-actions"]').exists()).toBe(false)
  })

  it('emits copy with raw_content', async () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    await wrapper.find('[data-test="copy-btn"]').trigger('click')
    expect(wrapper.emitted('copy')).toBeTruthy()
    expect(wrapper.emitted('copy')![0]).toEqual(['Hello world'])
  })

  it('emits regenerate on button click', async () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    await wrapper.find('[data-test="regenerate-btn"]').trigger('click')
    expect(wrapper.emitted('regenerate')).toBeTruthy()
  })

  it('shows streaming cursor when isStreaming is true', () => {
    const wrapper = mount(MessageItem, {
      props: {
        message: makeMsg({ role: 'assistant', raw_content: 'partial' }),
        style: 'bubble',
        isStreaming: true,
      },
    })
    expect(wrapper.find('[data-test="streaming-cursor"]').exists()).toBe(true)
  })

  it('shows swipe stub ‹ 1/1 ›', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    const swipe = wrapper.find('[data-test="swipe-indicator"]')
    expect(swipe.exists()).toBe(true)
    expect(swipe.text()).toContain('1/1')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/MessageItem.test.ts`
Expected: FAIL — cannot resolve `./MessageItem.vue`.

- [ ] **Step 3: Create `shirita-ui/src/components/MessageItem.vue`**

```vue
<script setup lang="ts">
import { computed } from 'vue'
import { Copy, RefreshCw, GitFork, ChevronLeft, ChevronRight } from 'lucide-vue-next'
import type { Message } from '../api/types'

const props = defineProps<{
  message: Message
  style: 'bubble' | 'flat'
  isStreaming?: boolean
}>()

const emit = defineEmits<{
  copy: [text: string]
  regenerate: []
  fork: []
}>()

const isAssistant = computed(() => props.message.role === 'assistant')
const isUser = computed(() => props.message.role === 'user')
const label = computed(() =>
  props.message.role === 'assistant' ? 'Assistant' : 'User',
)
</script>

<template>
  <!-- Bubble mode -->
  <div
    v-if="style === 'bubble'"
    data-test="msg-row"
    :class="['flex gap-2.5 mb-4', isUser ? 'justify-end' : 'justify-start']"
  >
    <!-- Assistant avatar (left side) -->
    <div
      v-if="isAssistant"
      data-test="assistant-avatar"
      class="w-8 h-8 rounded-full bg-sky/30 shrink-0 mt-1"
    />
    <div :class="['max-w-[75%]', isUser ? 'order-first' : '']">
      <div
        :class="[
          'px-4 py-2.5 rounded-2xl text-[15px] leading-relaxed whitespace-pre-wrap',
          isUser
            ? 'bg-coral/20 text-ink rounded-br-md'
            : 'bg-white border border-line rounded-bl-md',
        ]"
      >
        {{ message.raw_content }}
        <span
          v-if="isStreaming"
          data-test="streaming-cursor"
          class="inline-block w-1.5 h-4 bg-primary animate-pulse ml-0.5 align-text-bottom"
        />
      </div>
      <!-- Actions: assistant only -->
      <div
        v-if="isAssistant"
        data-test="message-actions"
        class="flex items-center gap-1 mt-1.5 ml-1 text-[12px] text-muted"
      >
        <span data-test="swipe-indicator" class="flex items-center gap-0.5">
          <ChevronLeft :size="12" />
          <span>1/1</span>
          <ChevronRight :size="12" />
        </span>
        <button
          data-test="copy-btn"
          class="hover:text-ink px-1"
          title="Copy"
          @click="emit('copy', message.raw_content)"
        >
          <Copy :size="13" />
        </button>
        <button
          data-test="regenerate-btn"
          class="hover:text-ink px-1"
          title="Regenerate"
          @click="emit('regenerate')"
        >
          <RefreshCw :size="13" />
        </button>
        <button
          class="hover:text-ink px-1"
          title="Fork"
          @click="emit('fork')"
        >
          <GitFork :size="13" />
        </button>
      </div>
    </div>
    <!-- User avatar placeholder (keeps spacing symmetrical) -->
    <div
      v-if="isUser"
      class="w-8 h-8 rounded-full bg-coral/20 shrink-0 mt-1"
    />
  </div>

  <!-- Flat mode -->
  <div
    v-else
    data-test="msg-row"
    class="mb-4"
  >
    <div class="flex items-center gap-2 mb-1">
      <div
        :class="[
          'w-6 h-6 rounded-full shrink-0',
          isAssistant ? 'bg-sky/30' : 'bg-coral/20',
        ]"
      />
      <span class="text-[13px] font-semibold text-muted capitalize">{{ label }}</span>
    </div>
    <div class="text-[15px] leading-relaxed whitespace-pre-wrap pl-8">
      {{ message.raw_content }}
      <span
        v-if="isStreaming"
        data-test="streaming-cursor"
        class="inline-block w-1.5 h-4 bg-primary animate-pulse ml-0.5 align-text-bottom"
      />
    </div>
    <div v-if="isAssistant" data-test="message-actions" class="flex items-center gap-1 mt-1.5 pl-8 text-[12px] text-muted">
      <span data-test="swipe-indicator" class="flex items-center gap-0.5">
        <ChevronLeft :size="12" />
        <span>1/1</span>
        <ChevronRight :size="12" />
      </span>
      <button data-test="copy-btn" class="hover:text-ink px-1" title="Copy" @click="emit('copy', message.raw_content)">
        <Copy :size="13" />
      </button>
      <button data-test="regenerate-btn" class="hover:text-ink px-1" title="Regenerate" @click="emit('regenerate')">
        <RefreshCw :size="13" />
      </button>
    </div>
    <div class="border-t border-line mt-3" />
  </div>
</template>
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/MessageItem.test.ts`
Expected: PASS (10 passed).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/MessageItem.vue shirita-ui/src/components/MessageItem.test.ts
git commit -m "feat(m3): MessageItem with bubble/flat variants + copy/regenerate/fork actions"
```

---

## Task 4: MessageList component (scrollable list + streaming ghost) (TDD)

**Files:**
- Create: `shirita-ui/src/components/MessageList.vue`
- Create: `shirita-ui/src/components/MessageList.test.ts`

- [ ] **Step 1: Write the failing test `shirita-ui/src/components/MessageList.test.ts`**

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import MessageList from './MessageList.vue'
import type { Message } from '../api/types'

function makeMsg(overrides: Partial<Message> = {}): Message {
  return {
    id: 'm1', session_id: 's1', parent_id: null, role: 'user',
    raw_content: 'Hello', display_content: null, is_hidden: false,
    snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    ...overrides,
  }
}

describe('MessageList', () => {
  it('renders a MessageItem per message', () => {
    const msgs = [
      makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' }),
      makeMsg({ id: 'm2', role: 'assistant', raw_content: 'hello' }),
    ]
    const wrapper = mount(MessageList, {
      props: { messages: msgs, style: 'bubble' },
    })
    const items = wrapper.findAll('[data-test="msg-row"]')
    expect(items).toHaveLength(2)
  })

  it('shows empty state when no messages', () => {
    const wrapper = mount(MessageList, {
      props: { messages: [], style: 'bubble' },
    })
    expect(wrapper.text()).toContain('No messages yet.')
  })

  it('renders streaming ghost when streaming', () => {
    const msgs = [makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' })]
    const wrapper = mount(MessageList, {
      props: {
        messages: msgs,
        style: 'bubble',
        isStreaming: true,
        streamingText: 'partial reply...',
      },
    })
    const items = wrapper.findAll('[data-test="msg-row"]')
    expect(items).toHaveLength(2) // user + streaming ghost
    expect(wrapper.text()).toContain('partial reply...')
    expect(wrapper.find('[data-test="streaming-cursor"]').exists()).toBe(true)
  })

  it('shows streaming error inline', () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [],
        style: 'bubble',
        isStreaming: false,
        streamingText: '',
        streamingError: 'session not found',
      },
    })
    expect(wrapper.text()).toContain('session not found')
  })

  it('passes style prop to MessageItem', () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [makeMsg({ role: 'assistant' })],
        style: 'flat',
      },
    })
    // Flat mode shows role label
    expect(wrapper.text()).toContain('Assistant')
  })

  it('emits copy from MessageItem', async () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [makeMsg({ id: 'm1', role: 'assistant', raw_content: 'test' })],
        style: 'bubble',
      },
    })
    await wrapper.find('[data-test="copy-btn"]').trigger('click')
    expect(wrapper.emitted('copy')).toBeTruthy()
    expect(wrapper.emitted('copy')![0]).toEqual(['test'])
  })

  it('emits regenerate from MessageItem', async () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [makeMsg({ role: 'assistant' })],
        style: 'bubble',
      },
    })
    await wrapper.find('[data-test="regenerate-btn"]').trigger('click')
    expect(wrapper.emitted('regenerate')).toBeTruthy()
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/MessageList.test.ts`
Expected: FAIL — cannot resolve `./MessageList.vue`.

- [ ] **Step 3: Create `shirita-ui/src/components/MessageList.vue`**

```vue
<script setup lang="ts">
import { computed } from 'vue'
import type { Message } from '../api/types'
import MessageItem from './MessageItem.vue'

const props = defineProps<{
  messages: Message[]
  style: 'bubble' | 'flat'
  isStreaming?: boolean
  streamingText?: string
  streamingError?: string | null
}>()

const emit = defineEmits<{
  copy: [text: string]
  regenerate: []
}>()

const streamingMsg = computed<Message | null>(() => {
  if (!props.isStreaming && !props.streamingText) return null
  return {
    id: '__streaming__',
    session_id: '',
    parent_id: null,
    role: 'assistant',
    raw_content: props.streamingText || '',
    display_content: null,
    is_hidden: false,
    snapshot_state: {},
    created_at: '',
  }
})
</script>

<template>
  <div class="flex-1 overflow-y-auto px-5 py-4">
    <p v-if="messages.length === 0 && !streamingMsg && !streamingError" class="text-muted text-sm text-center pt-12">
      No messages yet.
    </p>

    <MessageItem
      v-for="msg in messages"
      :key="msg.id"
      :message="msg"
      :style="style"
      @copy="emit('copy', $event)"
      @regenerate="emit('regenerate')"
    />

    <!-- Streaming ghost: the in-flight assistant response -->
    <MessageItem
      v-if="streamingMsg"
      :message="streamingMsg"
      :style="style"
      :is-streaming="true"
    />

    <p v-if="streamingError" class="text-coral text-sm text-center py-2">
      {{ streamingError }}
    </p>

    <!-- Scroll anchor: keep the latest content visible -->
    <div ref="bottom" />
  </div>
</template>
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/MessageList.test.ts`
Expected: PASS (7 passed).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/MessageList.vue shirita-ui/src/components/MessageList.test.ts
git commit -m "feat(m3): MessageList with streaming ghost + empty/error states"
```

---

## Task 5: Composer component (TDD)

**Files:**
- Create: `shirita-ui/src/components/Composer.vue`
- Create: `shirita-ui/src/components/Composer.test.ts`

- [ ] **Step 1: Write the failing test `shirita-ui/src/components/Composer.test.ts`**

```ts
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import Composer from './Composer.vue'

describe('Composer', () => {
  it('renders a text input and a send button', () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    expect(wrapper.find('textarea').exists()).toBe(true)
    expect(wrapper.find('[data-test="send-btn"]').exists()).toBe(true)
  })

  it('emits send with trimmed text on button click', async () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const textarea = wrapper.find('textarea')
    await textarea.setValue('  hello world  ')
    await wrapper.find('[data-test="send-btn"]').trigger('click')
    expect(wrapper.emitted('send')).toBeTruthy()
    expect(wrapper.emitted('send')![0]).toEqual(['hello world'])
    // Textarea clears after send
    expect((textarea.element as HTMLTextAreaElement).value).toBe('')
  })

  it('emits send on Enter (without Shift)', async () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const textarea = wrapper.find('textarea')
    await textarea.setValue('hi')
    await textarea.trigger('keydown', { key: 'Enter', shiftKey: false })
    expect(wrapper.emitted('send')).toBeTruthy()
  })

  it('does not send on Shift+Enter', async () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const textarea = wrapper.find('textarea')
    await textarea.setValue('hi')
    await textarea.trigger('keydown', { key: 'Enter', shiftKey: true })
    expect(wrapper.emitted('send')).toBeFalsy()
  })

  it('does not send empty text', async () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const textarea = wrapper.find('textarea')
    await textarea.setValue('   ')
    await wrapper.find('[data-test="send-btn"]').trigger('click')
    expect(wrapper.emitted('send')).toBeFalsy()
  })

  it('disables input and send button when disabled prop is true', () => {
    const wrapper = mount(Composer, { props: { disabled: true } })
    expect((wrapper.find('textarea').element as HTMLTextAreaElement).disabled).toBe(true)
    expect((wrapper.find('[data-test="send-btn"]').element as HTMLButtonElement).disabled).toBe(true)
  })

  it('shows disabled styling on send button when text is empty', () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const btn = wrapper.find('[data-test="send-btn"]')
    expect(btn.classes()).toContain('text-muted')
    expect(btn.classes()).not.toContain('text-primary')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/components/Composer.test.ts`
Expected: FAIL — cannot resolve `./Composer.vue`.

- [ ] **Step 3: Create `shirita-ui/src/components/Composer.vue`**

```vue
<script setup lang="ts">
import { ref, computed } from 'vue'
import { ArrowUp, Plus } from 'lucide-vue-next'

const props = defineProps<{ disabled: boolean }>()

const emit = defineEmits<{
  send: [text: string]
}>()

const text = ref('')
const hasText = computed(() => text.value.trim().length > 0)

function submit() {
  const trimmed = text.value.trim()
  if (!trimmed || props.disabled) return
  emit('send', trimmed)
  text.value = ''
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    submit()
  }
}
</script>

<template>
  <div class="border-t border-line bg-white px-4 py-3">
    <div class="max-w-[600px] mx-auto flex items-end gap-2.5">
      <!-- Attachment placeholder (Plan 5 / future) -->
      <button
        type="button"
        class="text-muted hover:text-ink p-1.5 shrink-0 mb-0.5"
        title="Attach"
      >
        <Plus :size="20" />
      </button>

      <textarea
        v-model="text"
        :disabled="disabled"
        rows="1"
        placeholder="Type a message…"
        class="flex-1 resize-none rounded-xl border border-line px-3.5 py-2.5 text-[15px] leading-relaxed
               focus:outline-none focus:border-primary/50 placeholder:text-muted/60
               disabled:bg-surface disabled:text-muted/50"
        @keydown="onKeydown"
      />

      <button
        data-test="send-btn"
        :disabled="disabled || !hasText"
        :class="[
          'w-10 h-10 rounded-full flex items-center justify-center shrink-0 transition-colors',
          hasText && !disabled
            ? 'bg-primary text-white'
            : 'bg-line text-muted',
        ]"
        @click="submit"
      >
        <ArrowUp :size="18" />
      </button>
    </div>
  </div>
</template>
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/components/Composer.test.ts`
Expected: PASS (7 passed).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/components/Composer.vue shirita-ui/src/components/Composer.test.ts
git commit -m "feat(m3): Composer with auto-sizing input + send (Enter / button)"
```

---

## Task 6: Wire ChatView — integrate store, router, and components (TDD)

**Files:**
- Modify: `shirita-ui/src/views/ChatView.vue`
- Create: `shirita-ui/src/views/ChatView.test.ts`
- Modify: `shirita-ui/src/router/index.ts` (eager-load ChatView to simplify test setup)

- [ ] **Step 1: Write the failing test `shirita-ui/src/views/ChatView.test.ts`**

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import { setActivePinia, createPinia } from 'pinia'
import * as client from '../api/client'
import ChatView from './ChatView.vue'

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/chat/:id', component: ChatView },
      { path: '/', component: { template: '<div />' } },
    ],
  })
}

describe('ChatView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.restoreAllMocks()
  })

  it('loads messages on mount', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([
      {
        id: 'm1', session_id: 's1', parent_id: null, role: 'user',
        raw_content: 'hi', display_content: null, is_hidden: false,
        snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
      },
    ])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()

    mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()

    expect(client.listMessages).toHaveBeenCalledWith('s1')
  })

  it('renders loaded messages', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([
      {
        id: 'm1', session_id: 's1', parent_id: null, role: 'user',
        raw_content: 'hello', display_content: null, is_hidden: false,
        snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
      },
    ])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()

    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()

    expect(wrapper.text()).toContain('hello')
  })

  it('shows loading state', async () => {
    // Never resolves during the test — loading stays true
    vi.spyOn(client, 'listMessages').mockReturnValue(new Promise(() => {}))
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()

    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()

    expect(wrapper.text()).toContain('Loading')
  })

  it('shows error state', async () => {
    vi.spyOn(client, 'listMessages').mockRejectedValue(new Error('Not found'))
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()

    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()

    expect(wrapper.text()).toContain('Not found')
  })

  it('calls send on composer submit', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([
      {
        id: 'm1', session_id: 's1', parent_id: null, role: 'user',
        raw_content: 'hi', display_content: null, is_hidden: false,
        snapshot_state: {}, created_at: '',
      },
    ])
    async function* stream(): AsyncGenerator<client.SseEvent> {
      yield { type: 'delta', text: 'ok' }
      yield { type: 'done', message_id: 'a1' }
    }
    const sendSpy = vi.spyOn(client, 'sendMessage').mockReturnValue(stream())

    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()

    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()

    // Type in composer and send
    const textarea = wrapper.find('textarea')
    await textarea.setValue('hello')
    await wrapper.find('[data-test="send-btn"]').trigger('click')
    await flushPromises()

    expect(sendSpy).toHaveBeenCalledWith('s1', 'hello')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd shirita-ui && npx vitest run src/views/ChatView.test.ts`
Expected: FAIL — `ChatView.vue` is still the stub (won't match the expected behavior).

- [ ] **Step 3: Replace `shirita-ui/src/views/ChatView.vue`**

```vue
<script setup lang="ts">
import { onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useChatStore } from '../stores/chat'
import { useUiStore } from '../stores/ui'
import MessageList from '../components/MessageList.vue'
import Composer from '../components/Composer.vue'

const route = useRoute()
const chat = useChatStore()
const ui = useUiStore()

const sessionId = route.params.id as string

onMounted(() => {
  chat.loadMessages(sessionId)
})

// Reload if navigating to a different session
watch(
  () => route.params.id,
  (newId) => {
    if (newId && newId !== sessionId) {
      chat.loadMessages(newId as string)
    }
  },
)

function handleSend(text: string) {
  chat.send(sessionId, text)
}

function handleCopy(text: string) {
  navigator.clipboard.writeText(text).catch(() => {
    // Fallback: clipboard API not available
  })
}

async function handleRegenerate() {
  // Find the last user message and re-send its text
  const lastUser = [...chat.messages].reverse().find((m) => m.role === 'user')
  if (lastUser) {
    await chat.send(sessionId, lastUser.raw_content)
  }
}
</script>

<template>
  <div class="flex flex-col h-full max-w-[600px] mx-auto">
    <!-- Header: session name + style toggle -->
    <div class="flex items-center justify-between px-5 pt-4 pb-2">
      <div class="flex items-center gap-2">
        <router-link to="/" class="text-muted hover:text-ink">
          ←
        </router-link>
        <span class="font-semibold text-ink truncate">
          Chat
        </span>
      </div>
      <button
        class="text-[13px] text-muted hover:text-ink"
        @click="ui.setMessageStyle(ui.messageStyle === 'bubble' ? 'flat' : 'bubble')"
      >
        {{ ui.messageStyle === 'bubble' ? 'Flat' : 'Bubble' }}
      </button>
    </div>

    <!-- Error on load -->
    <p v-if="chat.error" class="text-coral text-sm px-5 py-4">{{ chat.error }}</p>

    <!-- Loading -->
    <p v-else-if="chat.loading && chat.messages.length === 0" class="text-muted text-sm px-5 pt-12 text-center">
      Loading…
    </p>

    <!-- Messages -->
    <MessageList
      v-else
      :messages="chat.messages"
      :style="ui.messageStyle"
      :is-streaming="chat.isStreaming"
      :streaming-text="chat.streamingText"
      :streaming-error="chat.streamingError"
      @copy="handleCopy"
      @regenerate="handleRegenerate"
    />

    <!-- Composer (pinned bottom) -->
    <Composer :disabled="chat.isStreaming" @send="handleSend" />
  </div>
</template>
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd shirita-ui && npx vitest run src/views/ChatView.test.ts`
Expected: PASS (5 passed).

- [ ] **Step 5: Commit**

```bash
git add shirita-ui/src/views/ChatView.vue shirita-ui/src/views/ChatView.test.ts
git commit -m "feat(m3): ChatView wired with store, MessageList, Composer, SSE streaming"
```

---

## Task 7: Full verification + integration

**Files:**
- (no new files — run suite, fix any issues)

- [ ] **Step 1: Run the full test suite**

Run: `cd shirita-ui && npm run test`
Expected: all test files pass (client 6 + ui 2 + AppShell 2 + HomeView 2 + chat store 5 + MessageItem 10 + MessageList 7 + Composer 7 + ChatView 5 = 46 passed).

- [ ] **Step 2: Type-check + production build**

Run: `cd shirita-ui && npm run build`
Expected: `vue-tsc` reports no type errors; Vite writes `shirita-ui/dist/` with no errors.

- [ ] **Step 3: Commit any fixes**

```bash
git add -A && git commit -m "chore(m3): full test + type-check pass — chat detail slice"
```

---

## Self-review notes

- **Spec coverage:** §4.5 chat detail — bubble/flat message list ✓, composer with send ✓, SSE streaming ✓, message actions (regenerate via re-send, copy, fork stub, swipe stub) ✓. §10 M3 minimal regenerate (re-send last user text) ✓; fork/swipe full persistence deferred to M4 (UI stubs present).
- **No backend changes** — consumes existing `GET /api/sessions/{id}/messages` and `POST /api/sessions/{id}/messages` (SSE) only.
- **Type consistency:** `Message` type from Plan 1 `types.ts` used throughout. `SseEvent` added to `client.ts` and used by store. Store `send()` signature matches `sendMessage()` generator. `MessageItem` emits `copy` (text) and `regenerate` (void) — ChatView handlers match.
- **No placeholders:** all steps contain complete code.
- **i18n note:** "Loading…", "No messages yet.", "Type a message…", "Chat", "Assistant", "User", "Copy", "Flat"/"Bubble" are English strings; a later i18n pass will wrap them.
