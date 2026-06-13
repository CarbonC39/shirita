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
    expect(store.messages).toEqual([msg()])
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
    async function* stream(): AsyncGenerator<client.SseEvent> {
      throw new Error('Network error')
    }
    vi.spyOn(client, 'sendMessage').mockReturnValue(stream())

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
