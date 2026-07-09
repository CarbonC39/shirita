import { describe, it, expect, beforeEach, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import { ref } from 'vue'

vi.mock('../api/client', () => ({
  listNodes: vi.fn(),
  createNode: vi.fn().mockResolvedValue({ id: 'n-new' }),
  updateNode: vi.fn().mockResolvedValue({}),
  deleteNode: vi.fn().mockResolvedValue(undefined),
  reorderNodes: vi.fn().mockResolvedValue(undefined),
  createDefinition: vi.fn().mockResolvedValue({ id: 'd-new', type: 'prompt', name: '', content: '', meta: {} }),
  updateDefinition: vi.fn().mockResolvedValue({}),
}))
vi.mock('../stores/library', () => {
  const store = {
    definitions: [] as any[],
    loadDefinitions: vi.fn(async () => {}),
    addType: vi.fn(async () => ({ id: 'type-1', label: 'T', sort: 0, builtin: false, created_at: '' })),
  }
  return { useLibraryStore: () => store }
})
vi.mock('../utils/tree', () => ({ selectOneSiblingsToDisable: vi.fn(() => []) }))

import { useTreeEditor } from './useTreeEditor'
import { invokeComposable } from './_testkit'
import * as api from '../api/client'
import { useLibraryStore } from '../stores/library'
import { selectOneSiblingsToDisable } from '../utils/tree'
import type { Trigger } from '../api/types'

type Ed = ReturnType<typeof useTreeEditor>

function makeEditor(overrides: Record<string, any> = {}) {
  const ownerId = ref<string | null>('owner-1')
  const nodes = ref<any[]>([])
  const reload = vi.fn(async () => {})
  const ensureMaterialized = vi.fn(async () => {})
  const setLocalPatch = vi.fn(async () => {})
  const ed = invokeComposable<Ed>(() =>
    useTreeEditor({
      scope: overrides.scope ?? 'template',
      ownerId,
      nodes,
      reload,
      ensureMaterialized,
      setLocalPatch,
      ...overrides,
    }),
  )
  return { ed, ownerId, nodes, reload, ensureMaterialized, setLocalPatch }
}

describe('useTreeEditor', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    window.confirm = vi.fn(() => true) as any
    localStorage.clear()
    const lib = useLibraryStore()
    lib.definitions = []
    ;(lib.addType as any).mockResolvedValue({ id: 'type-1', label: 'T', sort: 0, builtin: false, created_at: '' })
    ;(api.createNode as any).mockResolvedValue({ id: 'n-new' })
    ;(api.createDefinition as any).mockResolvedValue({ id: 'd-new', type: 'prompt', name: '', content: '', meta: {} })
  })

  describe('create / add', () => {
    it('addPrompt creates a top-level ref node and reloads', async () => {
      const { ed, reload } = makeEditor()
      await ed.addPrompt('def-9')
      expect(api.createNode).toHaveBeenCalledWith('template', 'owner-1', { parent_id: null, kind: 'ref', definition_id: 'def-9' })
      expect(reload).toHaveBeenCalledTimes(1)
    })

    it('addContainer creates a folder tagged with the type id', async () => {
      const { ed } = makeEditor()
      await ed.addContainer('character')
      expect(api.createNode).toHaveBeenCalledWith('template', 'owner-1', { parent_id: null, kind: 'folder', tag: 'character' })
    })

    it('addRefToContainer nests a ref under the given parent', async () => {
      const { ed } = makeEditor()
      await ed.addRefToContainer('folder-1', 'def-9')
      expect(api.createNode).toHaveBeenCalledWith('template', 'owner-1', { parent_id: 'folder-1', kind: 'ref', definition_id: 'def-9' })
    })

    it('createNewPrompt makes a prompt def, reloads library, then links it', async () => {
      const lib = useLibraryStore()
      ;(api.createDefinition as any).mockResolvedValue({ id: 'd-prompt', type: 'prompt', name: 'Greet', content: '', meta: {} })
      const { ed } = makeEditor()
      await ed.createNewPrompt('  Greet  ')
      expect(api.createDefinition).toHaveBeenCalledWith(expect.objectContaining({ type: 'prompt', name: 'Greet', content: '', meta: {} }))
      expect(lib.loadDefinitions).toHaveBeenCalled()
      expect(api.createNode).toHaveBeenCalledWith('template', 'owner-1', { parent_id: null, kind: 'ref', definition_id: 'd-prompt' })
    })

    it('createNewPrompt defaults a blank name to "New prompt"', async () => {
      const { ed } = makeEditor()
      await ed.createNewPrompt('   ')
      expect(api.createDefinition).toHaveBeenCalledWith(expect.objectContaining({ name: 'New prompt' }))
    })

    it('createNewInContainer scaffolds a regex_rule with rule meta', async () => {
      ;(api.createDefinition as any).mockResolvedValue({ id: 'd-rx', type: 'regex_rule', name: 'New rule', content: '', meta: {} })
      const { ed } = makeEditor()
      await ed.createNewInContainer('folder-1', 'regex_rule')
      expect(api.createDefinition).toHaveBeenCalledWith(expect.objectContaining({
        type: 'regex_rule',
        name: 'New rule',
        meta: { pattern: '', replacement: '', disabled: false, scope: 'display', targets: ['ai_output'] },
      }))
      expect(api.createNode).toHaveBeenCalledWith('template', 'owner-1', { parent_id: 'folder-1', kind: 'ref', definition_id: 'd-rx' })
    })

    it('createNewInContainer names a non-regex type "New <type>"', async () => {
      const { ed } = makeEditor()
      await ed.createNewInContainer(null, 'lore')
      expect(api.createDefinition).toHaveBeenCalledWith(expect.objectContaining({ type: 'lore', name: 'New lore', meta: {} }))
    })

    it('createType slugifies the name, adds the type, then containers it', async () => {
      const { ed } = makeEditor()
      await ed.createType('My Type')
      expect((useLibraryStore().addType as any)).toHaveBeenCalledWith('my-type', 'My Type')
      expect(api.createNode).toHaveBeenCalledWith('template', 'owner-1', { parent_id: null, kind: 'folder', tag: 'type-1' })
    })

    it('createType ignores a blank name', async () => {
      const { ed } = makeEditor()
      await ed.createType('   ')
      expect(useLibraryStore().addType).not.toHaveBeenCalled()
      expect(api.createNode).not.toHaveBeenCalled()
    })
  })

  describe('update', () => {
    it('toggleEnabled flips the node and reloads', async () => {
      const { ed, nodes, reload } = makeEditor()
      nodes.value = [{ id: 'n1', enabled: false } as any]
      await ed.toggleEnabled('n1')
      expect(api.updateNode).toHaveBeenCalledWith('n1', { enabled: true })
      expect(reload).toHaveBeenCalled()
    })

    it('toggleEnabled disabling a node leaves siblings alone', async () => {
      const { ed, nodes } = makeEditor()
      nodes.value = [
        { id: 'n1', enabled: true } as any,
        { id: 'n2', enabled: true } as any,
      ]
      await ed.toggleEnabled('n1')
      expect(api.updateNode).toHaveBeenCalledWith('n1', { enabled: false })
      expect((api.updateNode as any).mock.calls.filter((c: any[]) => c[0] === 'n2')).toHaveLength(0)
    })

    it('toggleEnabled, when enabling, disables every select-one sibling', async () => {
      ;(selectOneSiblingsToDisable as any).mockReturnValue(['n2', 'n3'])
      const { ed, nodes } = makeEditor()
      nodes.value = [{ id: 'n1', enabled: false } as any]
      await ed.toggleEnabled('n1')
      expect(api.updateNode).toHaveBeenCalledWith('n1', { enabled: true })
      expect(api.updateNode).toHaveBeenCalledWith('n2', { enabled: false })
      expect(api.updateNode).toHaveBeenCalledWith('n3', { enabled: false })
      expect(selectOneSiblingsToDisable).toHaveBeenCalledWith(nodes.value, 'n1')
    })

    it('updateNodeMeta writes meta and reloads', async () => {
      const { ed } = makeEditor()
      await ed.updateNodeMeta('n1', { note: 'x' })
      expect(api.updateNode).toHaveBeenCalledWith('n1', { meta: { note: 'x' } })
    })

    it('updateContent writes via setLocalPatch when provided (session scope)', async () => {
      const { ed, setLocalPatch } = makeEditor({ scope: 'session' })
      await ed.updateContent('d1', 'hi')
      expect(setLocalPatch).toHaveBeenCalledWith('d1', { content: 'hi' })
      expect(api.updateDefinition).not.toHaveBeenCalled()
    })

    it('updateContent writes to the global def when no local patcher', async () => {
      const lib = useLibraryStore()
      const { ed } = makeEditor({ setLocalPatch: undefined })
      await ed.updateContent('d1', 'hi')
      expect(api.updateDefinition).toHaveBeenCalledWith('d1', { content: 'hi' })
      expect(lib.loadDefinitions).toHaveBeenCalled()
    })

    it('updateTrigger routes to setLocalPatch when provided', async () => {
      const trig: Trigger = { mode: 'keyword', keys: ['go'], probability: 100 }
      const { ed, setLocalPatch } = makeEditor({ scope: 'session' })
      await ed.updateTrigger('d1', trig)
      expect(setLocalPatch).toHaveBeenCalledWith('d1', { trigger: trig })
    })

    it('updateTrigger merges trigger into def meta otherwise', async () => {
      const lib = useLibraryStore()
      lib.definitions = [{ id: 'd1', meta: { foo: 1 } } as any]
      const trig: Trigger = { mode: 'random', keys: [], probability: 50 }
      const { ed } = makeEditor({ setLocalPatch: undefined })
      await ed.updateTrigger('d1', trig)
      expect(api.updateDefinition).toHaveBeenCalledWith('d1', { meta: { foo: 1, trigger: trig } })
    })

    it('updateTrigger is a no-op when the def is missing', async () => {
      const { ed } = makeEditor()
      await ed.updateTrigger('missing', { mode: 'constant', keys: [], probability: 100 })
      expect(api.updateDefinition).not.toHaveBeenCalled()
    })
  })

  describe('delete', () => {
    it('deletes a ref node after confirm and reloads', async () => {
      const { ed, nodes, reload } = makeEditor()
      nodes.value = [{ id: 'n1', kind: 'ref', tag: null } as any]
      await ed.deleteNode('n1')
      expect(api.deleteNode).toHaveBeenCalledTimes(1)
      expect(api.deleteNode).toHaveBeenCalledWith('n1')
      expect(reload).toHaveBeenCalled()
    })

    it('aborts when the user cancels the confirm', async () => {
      window.confirm = vi.fn(() => false) as any
      const { ed, nodes } = makeEditor()
      nodes.value = [{ id: 'n1', kind: 'ref', tag: null } as any]
      await ed.deleteNode('n1')
      expect(api.deleteNode).not.toHaveBeenCalled()
    })

    it('asks twice before deleting a non-empty folder', async () => {
      const { ed, nodes } = makeEditor()
      nodes.value = [
        { id: 'f1', kind: 'folder', tag: 'grp' } as any,
        { id: 'c1', kind: 'ref', parent_id: 'f1' } as any,
      ]
      await ed.deleteNode('f1')
      expect(window.confirm).toHaveBeenCalledTimes(2)
      expect(api.deleteNode).toHaveBeenCalledWith('f1')
    })

    it('skips the extra child-confirm for an empty folder', async () => {
      const { ed, nodes } = makeEditor()
      nodes.value = [{ id: 'f1', kind: 'folder', tag: 'grp' } as any]
      await ed.deleteNode('f1')
      expect(window.confirm).toHaveBeenCalledTimes(1)
    })

    it('ignores an unknown node id', async () => {
      const { ed } = makeEditor()
      await ed.deleteNode('nope')
      expect(api.deleteNode).not.toHaveBeenCalled()
    })
  })

  describe('reorder', () => {
    it('persists the new order for the owner and reloads', async () => {
      const { ed, reload } = makeEditor()
      await ed.reorder(['n2', 'n1', 'n3'])
      expect(api.reorderNodes).toHaveBeenCalledWith('template', 'owner-1', ['n2', 'n1', 'n3'])
      expect(reload).toHaveBeenCalled()
    })

    it('is a no-op without an owner', async () => {
      const { ed, ownerId } = makeEditor()
      ownerId.value = null
      await ed.reorder(['n1'])
      expect(api.reorderNodes).not.toHaveBeenCalled()
    })
  })

  describe('addPanel', () => {
    it('scaffolds an html+css def pair, a panel folder, and two ref bricks', async () => {
      ;(api.createDefinition as any)
        .mockResolvedValueOnce({ id: 'html1', type: 'html', name: 'Panel HTML', content: '', meta: {} })
        .mockResolvedValueOnce({ id: 'css1', type: 'css', name: 'Panel CSS', content: '', meta: {} })
      ;(api.createNode as any).mockResolvedValueOnce({
        id: 'folder1', owner_kind: 'template', owner_id: 'owner-1', parent_id: null, sort_order: 0,
        kind: 'folder', tag: 'panel', definition_id: null, enabled: true, created_at: '', meta: { name: 'Panel', caps: {} },
      })
      const { ed, reload } = makeEditor()
      await ed.addPanel()
      expect(api.createDefinition).toHaveBeenCalledWith(expect.objectContaining({ type: 'html' }))
      expect(api.createDefinition).toHaveBeenCalledWith(expect.objectContaining({ type: 'css' }))
      expect(api.createNode).toHaveBeenCalledWith('template', 'owner-1', { parent_id: null, kind: 'folder', tag: 'panel' })
      expect(api.updateNode).toHaveBeenCalledWith('folder1', { meta: { name: 'Panel', caps: {} } })
      expect(api.createNode).toHaveBeenCalledWith('template', 'owner-1', { parent_id: 'folder1', definition_id: 'html1', kind: 'ref' })
      expect(api.createNode).toHaveBeenCalledWith('template', 'owner-1', { parent_id: 'folder1', definition_id: 'css1', kind: 'ref' })
      expect(reload).toHaveBeenCalled()
    })
  })

  describe('session scope', () => {
    it('materializes before the first mutating call', async () => {
      const { ed, ensureMaterialized } = makeEditor({ scope: 'session' })
      await ed.addPrompt('d1')
      expect(ensureMaterialized).toHaveBeenCalled()
      expect(api.createNode).toHaveBeenCalledWith('session', 'owner-1', expect.anything())
    })
  })
})
