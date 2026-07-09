import { describe, it, expect, beforeEach, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import { ref } from 'vue'

vi.mock('../api/client', () => ({
  getSession: vi.fn(),
  listNodes: vi.fn().mockResolvedValue([]),
  setLocalDefinition: vi.fn().mockResolvedValue(undefined),
  clearLocalDefinition: vi.fn().mockResolvedValue(undefined),
  setLocalVariables: vi.fn().mockResolvedValue(undefined),
  materializeNodes: vi.fn().mockResolvedValue(undefined),
  materializePackNodes: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('../utils/clone', () => ({ deepClone: <T>(o: T): T => JSON.parse(JSON.stringify(o)) }))
vi.mock('../stores/library', () => {
  const store = { definitions: [] as any[] }
  return { useLibraryStore: () => store }
})
vi.mock('../stores/ui', () => {
  const s = { activeChatId: 's1' as string | null }
  return { useUiStore: () => s }
})

import { useLocalOverrides } from './useLocalOverrides'
import { invokeComposable } from './_testkit'
import * as api from '../api/client'
import { useLibraryStore } from '../stores/library'
import { useUiStore } from '../stores/ui'

type Ops = ReturnType<typeof useLocalOverrides>

const trig = { mode: 'keyword', keys: ['go'], probability: 100 }

function session(override_config: Record<string, unknown> = {}) {
  return { id: 's1', name: 'S', avatar: null, template_id: null, override_config, current_state: {}, mounted_definitions: [] }
}

async function boot(selectedPackId = ref<string | null>(null)) {
  const ops = invokeComposable<Ops>(() => useLocalOverrides({ selectedPackId }))
  await flushPromises()
  return ops
}

describe('useLocalOverrides', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
    useLibraryStore().definitions = []
    useUiStore().activeChatId = 's1'
    ;(api.listNodes as any).mockResolvedValue([])
    ;(api.getSession as any).mockResolvedValue(session({ local_definitions: {}, local_variables: [] }))
  })

  it('loadLocal clears the session when no chat is active', async () => {
    useUiStore().activeChatId = null
    const ops = await boot()
    expect(ops.localSession.value).toBe(null)
    expect(api.getSession).not.toHaveBeenCalled()
  })

  it('loadLocal fetches the session for the active chat', async () => {
    const ops = await boot()
    expect(api.getSession).toHaveBeenCalledWith('s1')
    expect(ops.localSession.value?.id).toBe('s1')
  })

  it('editLocal merges the local patch over the base definition', async () => {
    useLibraryStore().definitions = [{ id: 'd1', type: 'prompt', name: 'Base', content: 'base', meta: {} }]
    ;(api.getSession as any).mockResolvedValue(session({ local_definitions: { d1: { content: 'patched', trigger: trig } } }))
    const ops = await boot()
    ops.editLocal('d1')
    expect(ops.localEditDef.id).toBe('d1')
    expect(ops.localEditDef.name).toBe('Base') // patch has no name override
    expect(ops.localEditDef.content).toBe('patched')
    expect((ops.localEditDef.meta as any).trigger).toEqual(trig)
    expect(ops.localDefActive.value).toBe(true)
  })

  it('editLocal ignores an unknown definition id', async () => {
    const ops = await boot()
    ops.editLocal('missing')
    expect(ops.localDefActive.value).toBe(false)
  })

  it('saveLocal writes only the changed fields', async () => {
    useLibraryStore().definitions = [{ id: 'd1', type: 'prompt', name: 'Base', content: 'base', meta: {} }]
    ;(api.getSession as any).mockResolvedValue(session({ local_definitions: { d1: { content: 'patched', trigger: trig } } }))
    const ops = await boot()
    ops.editLocal('d1')
    const tickBefore = ops.localSavedTick.value
    await ops.saveLocal()
    expect(api.setLocalDefinition).toHaveBeenCalledWith('s1', 'd1', expect.objectContaining({ content: 'patched', trigger: trig }))
    // name unchanged → not part of the patch
    const patch = (api.setLocalDefinition as any).mock.calls.at(-1)[2]
    expect(patch).not.toHaveProperty('name')
    expect(ops.localSavedTick.value).toBe(tickBefore + 1)
  })

  it('revertLocal clears the override and closes the editor', async () => {
    useLibraryStore().definitions = [{ id: 'd1', type: 'prompt', name: 'Base', content: 'base', meta: {} }]
    ;(api.getSession as any).mockResolvedValue(session({ local_definitions: { d1: { content: 'patched' } } }))
    const ops = await boot()
    ops.editLocal('d1')
    expect(ops.localDefActive.value).toBe(true)
    await ops.revertLocal('d1')
    expect(api.clearLocalDefinition).toHaveBeenCalledWith('s1', 'd1')
    expect(ops.localDefActive.value).toBe(false)
  })

  it('setLocalPatch merges onto the existing patch', async () => {
    ;(api.getSession as any).mockResolvedValue(session({ local_definitions: { d1: { content: 'patched', trigger: trig } } }))
    const ops = await boot()
    await ops.setLocalPatch('d1', { content: 'new' })
    expect(api.setLocalDefinition).toHaveBeenCalledWith('s1', 'd1', expect.objectContaining({ content: 'new', trigger: trig }))
  })

  it('saveLocalVars persists the variable list', async () => {
    const ops = await boot()
    const vars = [{ name: 'x', type: 'string' as const, initial: '' }]
    await ops.saveLocalVars(vars)
    expect(api.setLocalVariables).toHaveBeenCalledWith('s1', vars)
  })

  it('ensureMaterialized materializes the session tree when empty', async () => {
    ;(api.listNodes as any).mockResolvedValue([])
    const ops = await boot()
    expect(ops.localNodes.value).toEqual([])
    await ops.ensureMaterialized()
    expect(api.materializeNodes).toHaveBeenCalledWith('s1')
    expect(ops.customizedLocally.value).toBe(true)
  })

  it('ensureMaterialized skips materialization when nodes already exist', async () => {
    ;(api.listNodes as any).mockResolvedValue([{ id: 'n1' }])
    const ops = await boot()
    await ops.ensureMaterialized()
    expect(api.materializeNodes).not.toHaveBeenCalled()
    expect(ops.customizedLocally.value).toBe(true)
  })

  it('materializeAll materializes template then pack', async () => {
    ;(api.listNodes as any).mockResolvedValue([])
    const selectedPackId = ref<string | null>('pack-1')
    const ops = await boot(selectedPackId)
    ops.localSession.value = { ...session({}), template_id: 't1' } as any
    await ops.materializeAll()
    expect(api.materializeNodes).toHaveBeenCalledWith('s1')
    expect(api.materializePackNodes).toHaveBeenCalledWith('s1', 'pack-1')
  })

  it('templateNodes keeps template + new nodes, excludes pack-sourced ones', async () => {
    ;(api.listNodes as any).mockResolvedValue([
      { id: 'n1', meta: { _source: 'template' } },
      { id: 'n2', meta: { _source: 'pack' } },
      { id: 'n3', meta: {} },
    ])
    const ops = await boot()
    const ids = ops.templateNodes.value.map((n: any) => n.id)
    expect(ids).toEqual(['n1', 'n3'])
  })
})
