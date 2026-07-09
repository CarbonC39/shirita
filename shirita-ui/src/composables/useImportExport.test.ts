import { describe, it, expect, beforeEach, vi } from 'vitest'
import { ref } from 'vue'

vi.mock('../api/client', () => ({
  importFile: vi.fn(),
  downloadExport: vi.fn().mockResolvedValue(undefined),
  exportDefinitionPath: (id: string) => `/api/definitions/${id}/export`,
  exportTemplatePath: (id: string) => `/api/templates/${id}/export`,
  downloadPackExport: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('../stores/library', () => {
  const store = { loadAll: vi.fn(async () => {}) }
  return { useLibraryStore: () => store }
})
vi.mock('../stores/media', () => {
  const store = { invalidate: vi.fn() }
  return { useMediaStore: () => store }
})

import { useImportExport } from './useImportExport'
import { invokeComposable } from './_testkit'
import * as api from '../api/client'
import { useLibraryStore } from '../stores/library'
import { useMediaStore } from '../stores/media'
import type { Pack } from '../api/types'

type Ops = ReturnType<typeof useImportExport>

function makeDeps() {
  return {
    selectedTemplateId: ref<string | null>(null),
    templateName: ref(''),
    selectedPackId: ref<string | null>(null),
    selectedPack: ref<Pack | null>(null),
    selectTemplate: vi.fn(async () => {}),
    selectPack: vi.fn(),
  }
}

function make() {
  const deps = makeDeps()
  const ops = invokeComposable<Ops>(() => useImportExport(deps))
  return { ops, deps }
}

const summary = (created: any[] = [], skipped: any[] = [], overwritten: any[] = []) => ({ created, skipped, overwritten })

describe('useImportExport', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('runImport selects a newly created pack and clears pending file', async () => {
    ;(api.importFile as any).mockResolvedValue(summary([{ kind: 'pack', id: 'p9', name: 'P' }]))
    const { ops, deps } = make()
    await ops.runImport(new File(['{}'], 'x.json'), 'skip')
    expect(api.importFile).toHaveBeenCalledWith(expect.any(File), 'skip')
    expect(useLibraryStore().loadAll).toHaveBeenCalled()
    expect(useMediaStore().invalidate).toHaveBeenCalledWith('avatar')
    expect(deps.selectedPackId.value).toBe('p9')
    expect(deps.selectTemplate).not.toHaveBeenCalled()
    expect(ops.pendingImportFile.value).toBe(null)
    expect(ops.importBusy.value).toBe(false)
  })

  it('runImport selects a newly created template when no pack is created', async () => {
    ;(api.importFile as any).mockResolvedValue(summary([{ kind: 'template', id: 't9', name: 'T' }]))
    const { ops, deps } = make()
    await ops.runImport(new File(['{}'], 'x.json'), 'skip')
    expect(deps.selectTemplate).toHaveBeenCalledWith('t9')
    expect(deps.selectedPackId.value).toBe(null)
  })

  it('runImport keeps the file pending when entries were skipped', async () => {
    ;(api.importFile as any).mockResolvedValue(summary([], [{ kind: 'definition', id: 'd1', name: 'N' }]))
    const { ops } = make()
    const file = new File(['{}'], 'x.json')
    await ops.runImport(file, 'skip')
    expect(ops.pendingImportFile.value).toBe(file)
    expect(ops.importSummary.value?.skipped).toHaveLength(1)
  })

  it('runImport resets state and rethrows on failure', async () => {
    ;(api.importFile as any).mockRejectedValue(new Error('boom'))
    const { ops } = make()
    await expect(ops.runImport(new File(['{}'], 'x.json'), 'skip')).rejects.toThrow('boom')
    expect(ops.importSummary.value).toBe(null)
    expect(ops.pendingImportFile.value).toBe(null)
    expect(ops.importBusy.value).toBe(false)
  })

  it('onImportPicked runs an import with "skip" and resets the input', async () => {
    ;(api.importFile as any).mockResolvedValue(summary())
    const { ops } = make()
    const target = { files: [new File(['{}'], 'x.json')], value: 'x' } as any
    await ops.onImportPicked({ target } as unknown as Event)
    expect(api.importFile).toHaveBeenCalledWith(expect.any(File), 'skip')
    expect(target.value).toBe('')
  })

  it('onImportPicked ignores an empty file selection', async () => {
    const { ops } = make()
    await ops.onImportPicked({ target: { files: [] } } as unknown as Event)
    expect(api.importFile).not.toHaveBeenCalled()
  })

  it('resolveImportConflicts re-runs the pending import with the chosen strategy', async () => {
    ;(api.importFile as any)
      .mockResolvedValueOnce(summary([], [{ kind: 'definition', id: 'd1', name: 'N' }]))
      .mockResolvedValueOnce(summary())
    const { ops } = make()
    const file = new File(['{}'], 'x.json')
    await ops.runImport(file, 'skip')
    await ops.resolveImportConflicts('overwrite')
    expect((api.importFile as any).mock.calls[1]).toEqual([file, 'overwrite'])
  })

  it('resolveImportConflicts is a no-op without a pending file', async () => {
    const { ops } = make()
    await ops.resolveImportConflicts('overwrite')
    expect(api.importFile).not.toHaveBeenCalled()
  })

  it('exportDefinition downloads named json and skips blanks', async () => {
    const { ops } = make()
    await ops.exportDefinition({ id: '', type: 'prompt', name: '', content: '', meta: {} })
    expect(api.downloadExport).not.toHaveBeenCalled()
    await ops.exportDefinition({ id: 'd1', type: 'prompt', name: 'My Def', content: '', meta: {} })
    expect(api.downloadExport).toHaveBeenCalledWith('/api/definitions/d1/export', 'My Def.json')
  })

  it('exportSelectedTemplate downloads the current template', async () => {
    const { ops, deps } = make()
    await ops.exportSelectedTemplate()
    expect(api.downloadExport).not.toHaveBeenCalled()
    deps.selectedTemplateId.value = 't1'
    deps.templateName.value = 'Main'
    await ops.exportSelectedTemplate()
    expect(api.downloadExport).toHaveBeenCalledWith('/api/templates/t1/export', 'Main.json')
  })

  it('exportSelectedPack downloads the current pack', async () => {
    const { ops, deps } = make()
    await ops.exportSelectedPack()
    expect(api.downloadPackExport).not.toHaveBeenCalled()
    deps.selectedPack.value = { id: 'p1', name: 'Alice', identity: { display_name: 'Alice', avatar: null }, meta: {}, created_at: '', updated_at: '' }
    await ops.exportSelectedPack()
    expect(api.downloadPackExport).toHaveBeenCalledWith('p1', 'Alice')
  })
})
