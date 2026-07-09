import { describe, it, expect, beforeEach, vi } from 'vitest'

vi.mock('../api/client', () => ({
  createPack: vi.fn().mockResolvedValue({ id: 'p-new', name: '', identity: { display_name: null, avatar: null }, meta: {}, created_at: '', updated_at: '' }),
  updatePack: vi.fn().mockResolvedValue({}),
  deletePack: vi.fn().mockResolvedValue(undefined),
  duplicatePack: vi.fn().mockResolvedValue({ id: 'p-dup', name: '', identity: { display_name: null, avatar: null }, meta: {}, created_at: '', updated_at: '' }),
  getOrphanDefinitionsForPack: vi.fn().mockResolvedValue([]),
  downloadPackExport: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('../stores/library', async () => {
  const { reactive } = await import('vue')
  // Reactive so `selectedPack`'s computed tracks `packs` like the real pinia
  // store would (an empty-array first read must still invalidate later).
  const store = reactive({ packs: [] as any[], loadPacks: vi.fn(async () => {}) })
  return { useLibraryStore: () => store }
})

import { usePackOps } from './usePackOps'
import { invokeComposable } from './_testkit'
import * as api from '../api/client'
import { useLibraryStore } from '../stores/library'

type Ops = ReturnType<typeof usePackOps>

const pack = (id: string, name: string, meta: Record<string, unknown> = {}) => ({
  id, name, identity: { display_name: name, avatar: null }, meta, created_at: '', updated_at: '',
})

function make() {
  return invokeComposable<Ops>(() => usePackOps())
}

describe('usePackOps', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    window.confirm = vi.fn(() => true) as any
    localStorage.clear()
    useLibraryStore().packs = []
    ;(api.createPack as any).mockResolvedValue(pack('p-new', 'New pack'))
    ;(api.duplicatePack as any).mockResolvedValue(pack('p-dup', 'Alice copy'))
    ;(api.getOrphanDefinitionsForPack as any).mockResolvedValue([])
  })

  it('selectPack stores the id locally and resolves selectedPack', () => {
    useLibraryStore().packs = [pack('p1', 'Alice')]
    const ops = make()
    ops.selectPack('p1')
    expect(ops.selectedPackId.value).toBe('p1')
    expect(localStorage.getItem('book.packId')).toBe('p1')
    expect(ops.selectedPack.value?.name).toBe('Alice')
  })

  it('selectPack with an empty id clears the selection', () => {
    const ops = make()
    ops.selectPack('p1')
    ops.selectPack('')
    expect(ops.selectedPackId.value).toBe(null)
    expect(ops.selectedPack.value).toBe(null)
  })

  it('createPackNamed creates a pack, reloads, and selects it', async () => {
    ;(api.createPack as any).mockResolvedValue(pack('p2', 'Bob'))
    const lib = useLibraryStore()
    const ops = make()
    await ops.createPackNamed('  Bob  ')
    expect(api.createPack).toHaveBeenCalledWith({ name: 'Bob' })
    expect(lib.loadPacks).toHaveBeenCalled()
    expect(ops.selectedPackId.value).toBe('p2')
  })

  it('createPackNamed falls back to "New pack" for a blank name', async () => {
    const ops = make()
    await ops.createPackNamed('   ')
    expect(api.createPack).toHaveBeenCalledWith({ name: 'New pack' })
  })

  it('startRenamePack seeds the draft from the selected pack', () => {
    useLibraryStore().packs = [pack('p1', 'Alice')]
    const ops = make()
    ops.selectPack('p1')
    ops.startRenamePack()
    expect(ops.packNameDraft.value).toBe('Alice')
    expect(ops.renamingPack.value).toBe(true)
  })

  it('renamePack persists a changed name and clears renaming flag', async () => {
    useLibraryStore().packs = [pack('p1', 'Alice')]
    const lib = useLibraryStore()
    const ops = make()
    ops.selectPack('p1')
    ops.startRenamePack()
    ops.packNameDraft.value = 'Alice 2'
    await ops.renamePack()
    expect(api.updatePack).toHaveBeenCalledWith('p1', { name: 'Alice 2', identity: pack('p1', 'Alice').identity, meta: {} })
    expect(lib.loadPacks).toHaveBeenCalled()
    expect(ops.renamingPack.value).toBe(false)
  })

  it('renamePack is a no-op when the name is unchanged', async () => {
    useLibraryStore().packs = [pack('p1', 'Alice')]
    const ops = make()
    ops.selectPack('p1')
    ops.startRenamePack()
    await ops.renamePack()
    expect(api.updatePack).not.toHaveBeenCalled()
  })

  it('dupPack duplicates and selects the copy', async () => {
    const ops = make()
    ops.selectPack('p1')
    await ops.dupPack()
    expect(api.duplicatePack).toHaveBeenCalledWith('p1')
    expect(ops.selectedPackId.value).toBe('p-dup')
  })

  it('delPack with no orphans deletes without touching orphans', async () => {
    const lib = useLibraryStore()
    const ops = make()
    ops.selectPack('p1')
    await ops.delPack()
    expect(api.deletePack).toHaveBeenCalledWith('p1', false)
    expect(ops.selectedPackId.value).toBe(null)
    expect(lib.loadPacks).toHaveBeenCalled()
  })

  it('delPack cascades to orphans only on the second confirm', async () => {
    ;(api.getOrphanDefinitionsForPack as any).mockResolvedValue([{ kind: 'definition', id: 'd1', name: 'N' }])
    const ops = make()
    ops.selectPack('p1')
    await ops.delPack()
    expect(api.deletePack).toHaveBeenCalledWith('p1', true)
  })

  it('delPack cancels cleanly on the first confirm', async () => {
    window.confirm = vi.fn(() => false) as any
    const ops = make()
    ops.selectPack('p1')
    await ops.delPack()
    expect(api.deletePack).not.toHaveBeenCalled()
    expect(ops.selectedPackId.value).toBe('p1')
  })

  it('exportSelectedPack downloads the selected pack, skipping when none', async () => {
    const ops = make()
    await ops.exportSelectedPack()
    expect(api.downloadPackExport).not.toHaveBeenCalled()
    useLibraryStore().packs = [pack('p1', 'Alice')]
    ops.selectPack('p1')
    await ops.exportSelectedPack()
    expect(api.downloadPackExport).toHaveBeenCalledWith('p1', 'Alice')
  })
})
