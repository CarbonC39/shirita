import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'

vi.mock('../api/client', () => ({
  getPack: vi.fn(),
  updatePack: vi.fn().mockResolvedValue({}),
  listNodes: vi.fn().mockResolvedValue([]),
  createNode: vi.fn().mockResolvedValue({}),
  updateNode: vi.fn().mockResolvedValue({}),
  deleteNode: vi.fn().mockResolvedValue(undefined),
  reorderNodes: vi.fn().mockResolvedValue(undefined),
  createDefinition: vi.fn().mockResolvedValue({}),
  updateDefinition: vi.fn().mockResolvedValue({}),
}))
vi.mock('../stores/library', () => ({
  useLibraryStore: () => ({ definitions: [], containerTypes: [], loadDefinitions: vi.fn(), addType: vi.fn() }),
}))

import PackEditor from './PackEditor.vue'
import PromptTree from './PromptTree.vue'
import * as api from '../api/client'

const pack = { id: 'p1', name: 'Alice', identity: { display_name: 'Alice', avatar: null }, meta: {}, created_at: '', updated_at: '' }
const stubs = { AssetPicker: true, PromptTree: true }

describe('PackEditor', () => {
  beforeEach(() => { setActivePinia(createPinia()); vi.clearAllMocks() })

  it('loads the pack node tree on mount', async () => {
    mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    expect(api.listNodes).toHaveBeenCalledWith('pack', 'p1')
  })

  it('shows the current display name', async () => {
    const w = mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    expect((w.find('[data-test="pack-display-name"]').element as HTMLInputElement).value).toBe('Alice')
  })

  it('no longer renders the legacy pack-panel section', async () => {
    const withPanel = { ...pack, meta: { panel: { html: '<span data-bind="hp">x</span>', css: '', caps: {} } } }
    const w = mount(PackEditor, { props: { pack: withPanel }, global: { stubs } })
    await flushPromises()
    expect(w.find('[data-test="pack-panel"]').exists()).toBe(false)
    expect(w.find('[data-test="panel-html"]').exists()).toBe(false)
  })

  it('no longer renders the pack variables section', () => {
    const w = mount(PackEditor, { props: { pack }, global: { stubs } })
    expect(w.find('[data-test="pack-variables"]').exists()).toBe(false)
  })

  it('Add panel scaffolds a panel folder with html and css bricks', async () => {
    const createDefinitionMock = api.createDefinition as unknown as ReturnType<typeof vi.fn>
    const createNodeMock = api.createNode as unknown as ReturnType<typeof vi.fn>
    createDefinitionMock
      .mockResolvedValueOnce({ id: 'html1', type: 'html', name: 'Panel HTML', content: '', meta: {} })
      .mockResolvedValueOnce({ id: 'css1', type: 'css', name: 'Panel CSS', content: '', meta: {} })
    createNodeMock.mockResolvedValueOnce({
      id: 'folder1', owner_kind: 'pack', owner_id: 'p1', parent_id: null, sort_order: 0,
      kind: 'folder', tag: 'panel', definition_id: null, enabled: true, created_at: '', meta: { name: 'Panel', caps: {} },
    })

    const w = mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    await w.findComponent(PromptTree).vm.$emit('add-panel')
    await flushPromises()

    expect(api.createDefinition).toHaveBeenCalledWith(expect.objectContaining({ type: 'html' }))
    expect(api.createDefinition).toHaveBeenCalledWith(expect.objectContaining({ type: 'css' }))
    expect(api.createNode).toHaveBeenCalledWith('pack', 'p1', expect.objectContaining({ kind: 'folder', tag: 'panel' }))
    expect(api.updateNode).toHaveBeenCalledWith('folder1', expect.objectContaining({ meta: { name: 'Panel', caps: {} } }))
    expect(api.createNode).toHaveBeenCalledWith('pack', 'p1', expect.objectContaining({ parent_id: 'folder1', definition_id: 'html1' }))
    expect(api.createNode).toHaveBeenCalledWith('pack', 'p1', expect.objectContaining({ parent_id: 'folder1', definition_id: 'css1' }))
  })

  it('editing the display name updates identity and emits changed', async () => {
    const w = mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    const input = w.find('[data-test="pack-display-name"]')
    await input.setValue('Alice 2')
    await input.trigger('change')
    await flushPromises()
    expect(api.updatePack).toHaveBeenCalledWith('p1', {
      name: 'Alice',
      identity: { display_name: 'Alice 2', avatar: null },
      meta: {},
    })
    expect(w.emitted('changed')).toBeTruthy()
  })
})
