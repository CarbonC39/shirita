import { describe, it, expect, beforeEach, vi } from 'vitest'
import { nextTick } from 'vue'

vi.mock('../api/client', () => ({
  createDefinition: vi.fn().mockResolvedValue({ id: 'd-new', type: 'prompt', name: '', content: '', meta: {} }),
  updateDefinition: vi.fn().mockResolvedValue({}),
  deleteDefinition: vi.fn().mockResolvedValue(undefined),
  exportDefinitionPath: (id: string) => `/api/definitions/${id}/export`,
  downloadExport: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('../stores/library', () => {
  const store = { definitions: [] as any[], loadDefinitions: vi.fn(async () => {}) }
  return { useLibraryStore: () => store }
})

import { useDefinitionOps } from './useDefinitionOps'
import { invokeComposable } from './_testkit'
import * as api from '../api/client'
import { useLibraryStore } from '../stores/library'

type Ops = ReturnType<typeof useDefinitionOps>

function make() {
  return invokeComposable<Ops>(() => useDefinitionOps())
}

describe('useDefinitionOps', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    window.confirm = vi.fn(() => true) as any
    localStorage.clear()
    useLibraryStore().definitions = []
    ;(api.createDefinition as any).mockResolvedValue({ id: 'd-new', type: 'prompt', name: '', content: '', meta: {} })
  })

  it('selectDefinition loads an existing def and persists the id', () => {
    useLibraryStore().definitions = [
      { id: 'd1', type: 'prompt', name: 'Greet', content: 'hi', meta: { a: 1 } },
    ]
    const ops = make()
    ops.selectDefinition('d1')
    expect(ops.editDef.id).toBe('d1')
    expect(ops.editDef.name).toBe('Greet')
    expect(ops.editDef.meta).toEqual({ a: 1 })
    expect(ops.defActive.value).toBe(true)
    expect(localStorage.getItem('book.defId')).toBe('d1')
  })

  it('selectDefinition with an empty id opens a blank editor seeded by name', () => {
    const ops = make()
    ops.selectDefinition('', '  New thing  ')
    expect(ops.editDef.id).toBe('')
    expect(ops.editDef.name).toBe('New thing')
    expect(ops.defActive.value).toBe(true)
  })

  it('saveDefinition creates a new def and captures its id', async () => {
    ;(api.createDefinition as any).mockResolvedValue({ id: 'd-7', type: 'prompt', name: 'X', content: '', meta: {} })
    const ops = make()
    ops.selectDefinition('')
    ops.editDef.type = 'prompt'
    ops.editDef.name = 'X'
    ops.editDef.content = 'body'
    const tickBefore = ops.defSavedTick.value
    await ops.saveDefinition()
    expect(api.createDefinition).toHaveBeenCalledWith({ type: 'prompt', name: 'X', content: 'body', meta: {} })
    expect(ops.editDef.id).toBe('d-7')
    expect(useLibraryStore().loadDefinitions).toHaveBeenCalled()
    expect(ops.defSavedTick.value).toBe(tickBefore + 1)
  })

  it('saveDefinition updates an existing def by id', async () => {
    const ops = make()
    ops.loadDef({ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: { x: 1 } })
    ops.editDef.name = 'N2'
    await ops.saveDefinition()
    expect(api.updateDefinition).toHaveBeenCalledWith('d1', { type: 'prompt', name: 'N2', content: 'c', meta: { x: 1 } })
    expect(api.createDefinition).not.toHaveBeenCalled()
  })

  it('deleteDef removes the def after confirm and resets the editor', async () => {
    const ops = make()
    ops.loadDef({ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: {} })
    await ops.deleteDef()
    expect(api.deleteDefinition).toHaveBeenCalledWith('d1')
    expect(ops.editDef.id).toBe('')
    expect(ops.defActive.value).toBe(false)
  })

  it('deleteDef aborts on cancel without calling the api', async () => {
    window.confirm = vi.fn(() => false) as any
    const ops = make()
    ops.loadDef({ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: {} })
    await ops.deleteDef()
    expect(api.deleteDefinition).not.toHaveBeenCalled()
  })

  it('deleteDef with no id just clears the editor', async () => {
    const ops = make()
    ops.selectDefinition('')
    await ops.deleteDef()
    expect(api.deleteDefinition).not.toHaveBeenCalled()
    expect(ops.defActive.value).toBe(false)
  })

  it('duplicateDef creates "<name> copy" and loads it into the editor', async () => {
    ;(api.createDefinition as any).mockResolvedValue({ id: 'd-cp', type: 'prompt', name: 'N copy', content: 'c', meta: {} })
    const ops = make()
    ops.loadDef({ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: {} })
    await ops.duplicateDef()
    expect(api.createDefinition).toHaveBeenCalledWith(expect.objectContaining({ name: 'N copy', content: 'c' }))
    expect(ops.editDef.id).toBe('d-cp')
  })

  it('exportDefinition downloads a named json and skips blanks', async () => {
    const ops = make()
    await ops.exportDefinition({ id: '', type: 'prompt', name: '', content: '', meta: {} })
    expect(api.downloadExport).not.toHaveBeenCalled()
    await ops.exportDefinition({ id: 'd1', type: 'prompt', name: 'My Def', content: '', meta: {} })
    expect(api.downloadExport).toHaveBeenCalledWith('/api/definitions/d1/export', 'My Def.json')
  })

  it('reactivity: editDef mutations are observable', async () => {
    const ops = make()
    ops.selectDefinition('')
    let seen = ''
    const stop = (ops as any).editDef // reactive proxy
    ops.editDef.name = 'first'
    seen = stop.name
    await nextTick()
    expect(seen).toBe('first')
  })
})
