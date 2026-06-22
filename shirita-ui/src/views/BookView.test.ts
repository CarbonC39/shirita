import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import { useUiStore } from '../stores/ui'

vi.mock('../api/client', () => ({
  listNodes: vi.fn().mockResolvedValue([]),
  createNode: vi.fn().mockResolvedValue({}),
  updateNode: vi.fn().mockResolvedValue({}),
  deleteNode: vi.fn().mockResolvedValue(undefined),
  reorderNodes: vi.fn().mockResolvedValue(undefined),
  updateDefinition: vi.fn().mockResolvedValue({}),
  createDefinition: vi.fn().mockResolvedValue({}),
  deleteDefinition: vi.fn().mockResolvedValue(undefined),
  createTemplate: vi.fn().mockResolvedValue({}),
  updateTemplate: vi.fn().mockResolvedValue({}),
  duplicateTemplate: vi.fn().mockResolvedValue({}),
  deleteTemplate: vi.fn().mockResolvedValue(undefined),
  getSession: vi.fn().mockResolvedValue({ id: 'c1', template_id: null, override_config: {} }),
  setLocalDefinition: vi.fn().mockResolvedValue(undefined),
  clearLocalDefinition: vi.fn().mockResolvedValue(undefined),
  promoteLocalDefinition: vi.fn().mockResolvedValue(undefined),
  materializeNodes: vi.fn().mockResolvedValue(undefined),
  setLocalVariables: vi.fn().mockResolvedValue(undefined),
  listPacks: vi.fn().mockResolvedValue([]),
  createPack: vi.fn().mockResolvedValue({ id: 'np' }),
  updatePack: vi.fn().mockResolvedValue({}),
  deletePack: vi.fn().mockResolvedValue(undefined),
  duplicatePack: vi.fn().mockResolvedValue({ id: 'dp' }),
  downloadPackExport: vi.fn().mockResolvedValue(undefined),
  importFile: vi.fn().mockResolvedValue({ created: [], skipped: [], overwritten: [] }),
  getOrphanDefinitionsForPack: vi.fn().mockResolvedValue([]),
}))

vi.mock('../stores/library', () => ({
  useLibraryStore: () => ({
    templates: [], definitions: [], containerTypes: [],
    packs: [{ id: 'imported-pack', name: 'Imported', identity: {}, meta: {} }],
    loadTemplates: vi.fn(), loadDefinitions: vi.fn(), loadTypes: vi.fn(),
    loadPacks: vi.fn(), loadAll: vi.fn(),
    addType: vi.fn(), removeType: vi.fn(),
  }),
}))

import BookView from './BookView.vue'
import * as api from '../api/client'

describe('BookView scopes', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    ;(api.getSession as any).mockResolvedValue({ id: 'c1', template_id: null, override_config: {} })
  })

  it('shows only the global section when there is no active chat', async () => {
    const ui = useUiStore(); ui.setActiveChatId(null)
    const w = mount(BookView)
    await flushPromises()
    expect(w.find('[data-test="book-local"]').exists()).toBe(false)
    expect(w.find('[data-test="book-global"]').exists()).toBe(true)
  })

  it('shows the local section above global when a chat is active', async () => {
    const ui = useUiStore(); ui.setActiveChatId('c1')
    const w = mount(BookView)
    await flushPromises()
    expect(w.find('[data-test="book-local"]').exists()).toBe(true)
    expect(w.find('[data-test="book-global"]').exists()).toBe(true)
  })

  it('shows the changed-in-this-chat chip strip when a local override exists', async () => {
    ;(api.getSession as any).mockResolvedValue({
      id: 'c1', template_id: null,
      override_config: { local_definitions: { d1: { content: 'local' } } },
    })
    const ui = useUiStore(); ui.setActiveChatId('c1')
    const w = mount(BookView)
    await flushPromises()
    expect(w.find('[data-test="local-chips"]').exists()).toBe(true)
  })

  it('shows the Pack section (picker + heading) in the global view', async () => {
    const ui = useUiStore(); ui.setActiveChatId(null)
    const w = mount(BookView)
    await flushPromises()
    expect(w.find('[data-test="book-pack"]').exists()).toBe(true)
    expect(w.find('[data-test="section-pack"]').exists()).toBe(true)
    expect(w.find('[data-test="pack-picker"]').exists()).toBe(true)
  })

  it('renders a pack Import button that triggers the shared file input', async () => {
    const ui = useUiStore(); ui.setActiveChatId(null)
    const w = mount(BookView)
    await flushPromises()
    // Import lives in the Pack section even with no pack selected (it creates one).
    const btn = w.find('[data-test="pack-import"]')
    expect(btn.exists()).toBe(true)
    // Clicking it opens the shared hidden file input.
    const input = w.find('input[type="file"]').element as HTMLInputElement
    const clickSpy = vi.spyOn(input, 'click').mockImplementation(() => {})
    await btn.trigger('click')
    expect(clickSpy).toHaveBeenCalled()
  })

  it('selects the newly imported pack immediately instead of leaving the editor unchanged', async () => {
    ;(api.importFile as any).mockResolvedValue({
      created: [{ kind: 'pack', id: 'imported-pack', name: 'Imported' }],
      skipped: [],
      overwritten: [],
    })
    const ui = useUiStore(); ui.setActiveChatId(null)
    const w = mount(BookView)
    await flushPromises()
    const input = w.find('input[type="file"]').element as HTMLInputElement
    Object.defineProperty(input, 'files', { value: [new File(['x'], 'card.png')], configurable: true })
    await w.find('input[type="file"]').trigger('change')
    await flushPromises()
    expect(w.find('[data-test="pack-editor"]').exists()).toBe(true)
  })
})
