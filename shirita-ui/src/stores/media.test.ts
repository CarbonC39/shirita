import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useMediaStore } from './media'
import * as client from '../api/client'

describe('media store by kind', () => {
  beforeEach(() => { setActivePinia(createPinia()); vi.restoreAllMocks() })

  it('caches assets per kind', async () => {
    vi.spyOn(client, 'listAssets').mockImplementation(async (kind?: string) =>
      kind === 'avatar'
        ? [{ id: 'a', name: 'f', path: 'a.png', url: '/assets/a.png', kind: 'avatar' }]
        : [{ id: 'b', name: 's', path: 'b.png', url: '/assets/b.png', kind: 'background' }],
    )
    const m = useMediaStore()
    await m.load('avatar')
    await m.load('background')
    expect(m.byKind('avatar').map((a) => a.id)).toEqual(['a'])
    expect(m.byKind('background').map((a) => a.id)).toEqual(['b'])
  })

  it('invalidate forces the next load to refetch (picks up assets created server-side)', async () => {
    const list = vi.spyOn(client, 'listAssets').mockResolvedValue([])
    const m = useMediaStore()
    await m.load('avatar')
    expect(list).toHaveBeenCalledTimes(1)

    await m.load('avatar') // cached — no second fetch
    expect(list).toHaveBeenCalledTimes(1)

    m.invalidate('avatar')
    list.mockResolvedValue([{ id: 'x', name: 'n', path: 'x.png', url: '/assets/x.png', kind: 'avatar' }])
    await m.load('avatar')
    expect(list).toHaveBeenCalledTimes(2)
    expect(m.byKind('avatar').map((a) => a.id)).toEqual(['x'])
  })
})
