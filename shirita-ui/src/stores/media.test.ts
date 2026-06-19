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
})
