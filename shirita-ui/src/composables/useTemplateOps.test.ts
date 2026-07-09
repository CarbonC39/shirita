import { describe, it, expect, beforeEach, vi } from 'vitest'

vi.mock('../api/client', () => ({
  listNodes: vi.fn().mockResolvedValue([]),
  createTemplate: vi.fn().mockResolvedValue({ id: 't-new', name: '', meta: {}, created_at: '', updated_at: '' }),
  updateTemplate: vi.fn().mockResolvedValue({}),
  duplicateTemplate: vi.fn().mockResolvedValue({ id: 't-dup', name: '', meta: {}, created_at: '', updated_at: '' }),
  deleteTemplate: vi.fn().mockResolvedValue(undefined),
  getOrphanDefinitions: vi.fn().mockResolvedValue([]),
}))
vi.mock('../utils/tokens', () => ({
  estimateTokens: (c: string) => (c || '').length,
  formatTokens: vi.fn(),
}))
vi.mock('../stores/library', () => {
  const store = {
    definitions: [] as any[],
    templates: [] as any[],
    loadTemplates: vi.fn(async () => {}),
  }
  return { useLibraryStore: () => store }
})

import { useTemplateOps } from './useTemplateOps'
import { invokeComposable } from './_testkit'
import * as api from '../api/client'
import { useLibraryStore } from '../stores/library'

type Ops = ReturnType<typeof useTemplateOps>

const tpl = (id: string, name: string, meta: Record<string, unknown> = {}) => ({ id, name, meta, created_at: '', updated_at: '' })

function make() {
  return invokeComposable<Ops>(() => useTemplateOps())
}

describe('useTemplateOps', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    window.confirm = vi.fn(() => true) as any
    localStorage.clear()
    const lib = useLibraryStore()
    lib.definitions = []
    lib.templates = []
    ;(api.listNodes as any).mockResolvedValue([])
    ;(api.createTemplate as any).mockResolvedValue(tpl('t-new', 'New template'))
    ;(api.duplicateTemplate as any).mockResolvedValue(tpl('t-dup', 'A copy'))
    ;(api.getOrphanDefinitions as any).mockResolvedValue([])
  })

  it('selectTemplate loads nodes + name and persists the id', async () => {
    useLibraryStore().templates = [tpl('t1', 'Main')]
    ;(api.listNodes as any).mockResolvedValue([{ id: 'n1', kind: 'ref', enabled: true, definition_id: 'd1' }])
    const ops = make()
    await ops.selectTemplate('t1')
    expect(ops.selectedTemplateId.value).toBe('t1')
    expect(ops.templateName.value).toBe('Main')
    expect(localStorage.getItem('book.templateId')).toBe('t1')
    expect(api.listNodes).toHaveBeenCalledWith('template', 't1')
    expect(ops.nodes.value).toHaveLength(1)
  })

  it('selectTemplate with an empty id clears state', async () => {
    const ops = make()
    await ops.selectTemplate('t1')
    await ops.selectTemplate('')
    expect(ops.selectedTemplateId.value).toBe(null)
    expect(ops.nodes.value).toEqual([])
    expect(ops.templateName.value).toBe('')
  })

  it('reload re-fetches nodes for the current template', async () => {
    ;(api.listNodes as any).mockResolvedValue([{ id: 'n1' }])
    const ops = make()
    await ops.selectTemplate('t1')
    ;(api.listNodes as any).mockResolvedValue([{ id: 'n1' }, { id: 'n2' }])
    await ops.reload()
    expect(ops.nodes.value).toHaveLength(2)
  })

  it('createTemplateNamed creates, reloads, and selects the new template', async () => {
    ;(api.createTemplate as any).mockResolvedValue(tpl('t9', 'Combat'))
    useLibraryStore().templates = [tpl('t9', 'Combat')]
    const ops = make()
    await ops.createTemplateNamed('  Combat  ')
    expect(api.createTemplate).toHaveBeenCalledWith('Combat')
    expect(useLibraryStore().loadTemplates).toHaveBeenCalled()
    expect(ops.selectedTemplateId.value).toBe('t9')
  })

  it('renameTemplate persists a changed name', async () => {
    useLibraryStore().templates = [tpl('t1', 'Old')]
    const lib = useLibraryStore()
    const ops = make()
    await ops.selectTemplate('t1')
    ops.templateName.value = 'New'
    await ops.renameTemplate()
    expect(api.updateTemplate).toHaveBeenCalledWith('t1', 'New')
    expect(lib.loadTemplates).toHaveBeenCalled()
  })

  it('renameTemplate is a no-op when the name is unchanged', async () => {
    useLibraryStore().templates = [tpl('t1', 'Same')]
    const ops = make()
    await ops.selectTemplate('t1')
    await ops.renameTemplate()
    expect(api.updateTemplate).not.toHaveBeenCalled()
  })

  it('dupTemplate duplicates and selects the copy', async () => {
    const ops = make()
    await ops.selectTemplate('t1')
    await ops.dupTemplate()
    expect(api.duplicateTemplate).toHaveBeenCalledWith('t1')
    expect(ops.selectedTemplateId.value).toBe('t-dup')
  })

  it('delTemplate deletes and clears state', async () => {
    const lib = useLibraryStore()
    const ops = make()
    await ops.selectTemplate('t1')
    await ops.delTemplate()
    expect(api.deleteTemplate).toHaveBeenCalledWith('t1', false)
    expect(ops.selectedTemplateId.value).toBe(null)
    expect(ops.nodes.value).toEqual([])
    expect(lib.loadTemplates).toHaveBeenCalled()
  })

  it('delTemplate cascades orphans on the second confirm', async () => {
    ;(api.getOrphanDefinitions as any).mockResolvedValue([{ kind: 'definition', id: 'd1', name: 'N' }])
    const ops = make()
    await ops.selectTemplate('t1')
    await ops.delTemplate()
    expect(api.deleteTemplate).toHaveBeenCalledWith('t1', true)
  })

  it('delTemplate cancels on the first confirm', async () => {
    window.confirm = vi.fn(() => false) as any
    const ops = make()
    await ops.selectTemplate('t1')
    await ops.delTemplate()
    expect(api.deleteTemplate).not.toHaveBeenCalled()
  })

  it('toggleDefaultTemplate demotes the other default and promotes the current', async () => {
    useLibraryStore().templates = [
      tpl('t1', 'A', { default: true }),
      tpl('t2', 'B', {}),
    ]
    const lib = useLibraryStore()
    const ops = make()
    await ops.selectTemplate('t2')
    await ops.toggleDefaultTemplate()
    expect(api.updateTemplate).toHaveBeenCalledWith('t1', 'A', { default: false })
    expect(api.updateTemplate).toHaveBeenCalledWith('t2', 'B', { default: true })
    expect(lib.loadTemplates).toHaveBeenCalled()
  })

  it('isDefaultTemplate reflects the current template meta', async () => {
    useLibraryStore().templates = [tpl('t1', 'A', { default: true })]
    const ops = make()
    await ops.selectTemplate('t1')
    expect(ops.isDefaultTemplate.value).toBe(true)
  })

  it('templateTokens sums enabled ref bricks only', async () => {
    useLibraryStore().definitions = [
      { id: 'd1', type: 'prompt', name: 'D1', content: 'hello', meta: {} },
      { id: 'd2', type: 'prompt', name: 'D2', content: 'world!', meta: {} },
    ]
    ;(api.listNodes as any).mockResolvedValue([
      { id: 'n1', kind: 'ref', enabled: true, definition_id: 'd1' },
      { id: 'n2', kind: 'ref', enabled: false, definition_id: 'd2' },
      { id: 'n3', kind: 'folder', enabled: true, definition_id: null },
      { id: 'n4', kind: 'ref', enabled: true, definition_id: 'missing' },
    ])
    const ops = make()
    await ops.selectTemplate('t1')
    expect(ops.templateTokens.value).toBe(5) // 'hello' only
  })
})
