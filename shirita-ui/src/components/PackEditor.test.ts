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
import * as api from '../api/client'

const pack = { id: 'p1', name: 'Alice', identity: { display_name: 'Alice', avatar: null }, meta: {}, created_at: '', updated_at: '' }
const stubs = { AssetPicker: true, PromptTree: true, VariablesEditor: true, PanelView: true }

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

  it('renders the panel editor seeded from meta.panel with a live preview', async () => {
    const withPanel = { ...pack, meta: { panel: { html: '<span data-bind="hp">x</span>', css: '', caps: {} } } }
    const w = mount(PackEditor, { props: { pack: withPanel }, global: { stubs } })
    await flushPromises()
    expect(w.find('[data-test="pack-panel"]').exists()).toBe(true)
    expect((w.find('[data-test="panel-html"]').element as HTMLTextAreaElement).value).toBe('<span data-bind="hp">x</span>')
    expect(w.find('[data-test="pack-panel"]').text()).toContain('Preview') // PanelView preview section present
  })

  it('editing the panel HTML saves meta.panel', async () => {
    const w = mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    const ta = w.find('[data-test="panel-html"]')
    await ta.setValue('<b>{{hp}}</b>')
    await ta.trigger('change')
    await flushPromises()
    expect(api.updatePack).toHaveBeenCalledWith('p1', expect.objectContaining({
      meta: expect.objectContaining({ panel: { html: '<b>{{hp}}</b>', css: '', caps: {} } }),
    }))
  })

  it('toggling a capability saves it', async () => {
    const w = mount(PackEditor, { props: { pack }, global: { stubs } })
    await flushPromises()
    await w.find('[data-test="cap-write"]').setValue(true)
    await flushPromises()
    expect(api.updatePack).toHaveBeenCalledWith('p1', expect.objectContaining({
      meta: expect.objectContaining({ panel: expect.objectContaining({ caps: { write: true } }) }),
    }))
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
