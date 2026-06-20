import { describe, it, expect, beforeEach, vi } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'

vi.mock('../api/client', () => ({
  listDefinitions: vi.fn().mockResolvedValue([]),
  listTemplates: vi.fn().mockResolvedValue([]),
  listTypes: vi.fn().mockResolvedValue([]),
  listPacks: vi.fn().mockResolvedValue([
    { id: 'p1', name: 'Alice', identity: { display_name: null, avatar: null }, meta: {}, created_at: '', updated_at: '' },
  ]),
  createType: vi.fn(),
  deleteType: vi.fn(),
}))

import { useLibraryStore } from './library'

describe('library store packs', () => {
  beforeEach(() => { setActivePinia(createPinia()) })

  it('loadPacks fills packs from the API', async () => {
    const lib = useLibraryStore()
    expect(lib.packs).toEqual([])
    await lib.loadPacks()
    expect(lib.packs.map((p) => p.id)).toEqual(['p1'])
  })

  it('loadAll also loads packs', async () => {
    const lib = useLibraryStore()
    await lib.loadAll()
    expect(lib.packs.length).toBe(1)
  })
})
