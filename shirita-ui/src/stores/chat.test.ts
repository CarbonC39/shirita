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
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's1', active_leaf_id: 'm2' } as any)

    const store = useChatStore()
    await store.loadMessages('s1')

    expect(store.messages).toEqual(items)
    expect(store.loading).toBe(false)
  })

  it('sendMessage streams deltas into streamingText and reloads on done', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([msg()])
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's1', active_leaf_id: null } as any)
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

  it('displays only the active branch and seeds the leaf from the session', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([
      msg({ id: 'a', parent_id: null, created_at: '1' }),
      msg({ id: 'b', parent_id: 'a', role: 'assistant', created_at: '2' }),
      msg({ id: 'b2', parent_id: 'a', role: 'assistant', created_at: '3' }),
    ])
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's', active_leaf_id: 'b2' } as any)
    const store = useChatStore()
    await store.loadMessages('s')
    expect(store.displayed.map((x: Message) => x.id)).toEqual(['a', 'b2'])
  })

  it('switchLeaf updates the leaf from the endpoint response', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([
      msg({ id: 'a', parent_id: null, created_at: '1' }),
      msg({ id: 'b', parent_id: 'a', role: 'assistant', created_at: '2' }),
      msg({ id: 'b2', parent_id: 'a', role: 'assistant', created_at: '3' }),
    ])
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's', active_leaf_id: 'b2' } as any)
    vi.spyOn(client, 'setActiveLeaf').mockResolvedValue({ id: 's', active_leaf_id: 'b' } as any)
    const store = useChatStore()
    await store.loadMessages('s')
    await store.switchLeaf('b')
    expect(store.activeLeafId).toBe('b')
    expect(store.displayed.map((x: Message) => x.id)).toEqual(['a', 'b'])
  })
})
