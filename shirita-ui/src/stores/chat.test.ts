import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useChatStore } from './chat'
import * as client from '../api/client'
import type { Message } from '../api/types'

function msg(overrides: Partial<Message> = {}): Message {
  return {
    id: 'm1', session_id: 's1', parent_id: null, role: 'user',
    raw_content: 'hi', display_content: null, is_hidden: false, is_anchor: false,
    attachments: [], snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
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

    expect(client.sendMessage).toHaveBeenCalledWith('s1', 'hi', [], expect.any(AbortSignal))
    expect(store.messages).toEqual([msg()])
    expect(store.isStreaming).toBe(false)
    expect(store.streamingText).toBe('')
    expect(store.streamingError).toBeNull()
  })

  it('shows the user message immediately, before the assistant reply streams in', () => {
    let resolveStream!: () => void
    async function* stream(): AsyncGenerator<client.SseEvent> {
      await new Promise<void>((r) => { resolveStream = r })
      yield { type: 'done', message_id: 'a1' }
    }
    vi.spyOn(client, 'sendMessage').mockReturnValue(stream())
    vi.spyOn(client, 'listMessages').mockResolvedValue([])

    const store = useChatStore()
    const promise = store.send('s1', 'hello')

    expect(store.displayed.some((m) => m.role === 'user' && m.raw_content === 'hello')).toBe(true)

    resolveStream()
    return promise
  })

  it('rolls back the optimistic user message if the stream errors before persisting', async () => {
    async function* stream(): AsyncGenerator<client.SseEvent> {
      yield { type: 'error', message: 'boom' }
    }
    vi.spyOn(client, 'sendMessage').mockReturnValue(stream())

    const store = useChatStore()
    await store.send('s1', 'hello')

    expect(store.displayed.some((m) => m.raw_content === 'hello')).toBe(false)
    expect(store.activeLeafId).toBeNull()
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

  it('stop() reloads the transcript so a backend-persisted partial reply is shown', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([msg()])
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's1', active_leaf_id: null } as any)
    const abortSpy = vi.spyOn(client, 'abortSession').mockResolvedValue()
    const store = useChatStore()
    await store.loadMessages('s1') // initial load (call #1) + sets activeSessionId
    expect(client.listMessages).toHaveBeenCalledTimes(1)
    await store.stop()
    expect(abortSpy).toHaveBeenCalledWith('s1')
    // The hard abort discards the `stopped` SSE event, so stop() must reload
    // itself — otherwise the persisted partial reply is invisible until the
    // user navigates away and back.
    expect(client.listMessages).toHaveBeenCalledTimes(2)
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

  it('hiding an intermediate message keeps the full root-to-leaf chain displayed', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([
      msg({ id: 'root', parent_id: null, role: 'user', created_at: '1' }),
      msg({ id: 'mid', parent_id: 'root', role: 'assistant', created_at: '2' }),
      msg({ id: 'leaf', parent_id: 'mid', role: 'user', created_at: '3' }),
    ])
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's', active_leaf_id: 'leaf' } as any)
    // The server flips is_hidden on the middle message.
    vi.spyOn(client, 'editMessage').mockResolvedValue(
      msg({ id: 'mid', parent_id: 'root', role: 'assistant', created_at: '2', is_hidden: true }),
    )
    const store = useChatStore()
    await store.loadMessages('s')
    await store.toggleHidden('mid')
    // Hiding the middle message must not orphan the root from the active path.
    expect(store.displayed.map((x: Message) => x.id)).toEqual(['root', 'mid', 'leaf'])
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
