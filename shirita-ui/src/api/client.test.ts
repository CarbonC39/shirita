import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { listSessions, listMessages, sendMessage, listTypes, reorderNodes, importFile } from './client'
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

describe('types + reorder client', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('listTypes GETs /api/types', async () => {
    const data = [{ id: 'char', label: 'Character', sort: 0, builtin: true, created_at: '' }]
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => data })
    vi.stubGlobal('fetch', fetchMock)
    const out = await listTypes()
    expect(out).toEqual(data)
    expect(fetchMock.mock.calls[0][0]).toContain('/api/types')
  })

  it('reorderNodes PUTs ordered ids', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({}) })
    vi.stubGlobal('fetch', fetchMock)
    await reorderNodes('template', 'tpl1', ['a', 'b'])
    const [url, opts] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/templates/tpl1/nodes/reorder?owner_kind=template')
    expect(opts.method).toBe('PUT')
    expect(JSON.parse(opts.body)).toEqual({ ordered_ids: ['a', 'b'] })
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

describe('importFile', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('posts multipart FormData with file and on_conflict query', async () => {
    vi.stubGlobal('fetch', mockFetch(200, { created: [], skipped: [], overwritten: [] }))
    const file = new File([new Uint8Array([1, 2, 3])], 'card.png', { type: 'image/png' })
    await importFile(file, 'overwrite')
    const fetchMock = fetch as unknown as ReturnType<typeof vi.fn>
    const [url, init] = fetchMock.mock.calls[0]
    expect(String(url)).toContain('/api/import')
    expect(String(url)).toContain('on_conflict=overwrite')
    expect(init.method).toBe('POST')
    expect(init.body).toBeInstanceOf(FormData)
    expect((init.body as FormData).get('file')).toBeInstanceOf(File)
  })
})

describe('runtime config injection', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('prefers injected window.__SHIRITA_RUNTIME__ for BASE and TOKEN', async () => {
    vi.resetModules()
    vi.stubGlobal('__SHIRITA_RUNTIME__', { base: 'http://127.0.0.1:9999', token: 'inj-tok' })
    const fm = vi.fn().mockResolvedValue({ ok: true, status: 200, json: async () => [] })
    vi.stubGlobal('fetch', fm)
    const { listSessions } = await import('./client')
    await listSessions()
    expect(fm).toHaveBeenCalledWith('http://127.0.0.1:9999/api/sessions', {
      headers: { Authorization: 'Bearer inj-tok' },
    })
  })
})
