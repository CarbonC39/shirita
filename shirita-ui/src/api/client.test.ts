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
