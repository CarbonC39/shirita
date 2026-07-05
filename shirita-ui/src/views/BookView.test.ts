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

const libraryMock = vi.hoisted(() => ({
  templates: [] as any[], definitions: [] as any[], containerTypes: [] as any[],
  packs: [{ id: 'imported-pack', name: 'Imported', identity: {}, meta: {} }],
  loadTemplates: vi.fn(), loadDefinitions: vi.fn(), loadTypes: vi.fn(),
  loadPacks: vi.fn(), loadAll: vi.fn(), addType: vi.fn(), removeType: vi.fn(),
}))
vi.mock('../stores/library', () => ({ useLibraryStore: () => libraryMock }))

import BookView from './BookView.vue'
import * as api from '../api/client'
import VariablesEditor from '../components/VariablesEditor.vue'

describe('BookView scopes', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    libraryMock.templates = []
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

  it('drills into a definition via the navigator when customized', async () => {
    ;(api.getSession as any).mockResolvedValue({
      id: 'c1', template_id: 't1',
      override_config: { local_definitions: { d1: { content: 'local' } } },
    })
    ;(api.listNodes as any).mockResolvedValue([
      { id: 'n1', owner_kind: 'session', owner_id: 'c1', parent_id: null, sort_order: 0,
        kind: 'ref', tag: null, definition_id: 'd1', enabled: true, created_at: '', meta: { _source: 'template' } },
    ])
    libraryMock.definitions = [{ id: 'd1', type: 'prompt', name: 'D1', content: 'global', meta: {} }]
    const ui = useUiStore(); ui.setActiveChatId('c1')
    const w = mount(BookView)
    await flushPromises()
    await w.find('[data-test="customize-locally"]').trigger('click')
    await flushPromises()
    await w.find('[data-test="node-row-n1"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-test="nav-level-definition"]').exists()).toBe(true)
  })

  it('shows newly-created (untagged) session nodes in the template tree', async () => {
    // A session node with no _source tag simulates one just created via the
    // tree's create affordances (createNode body carries no meta). It must
    // still render alongside materialized template nodes.
    ;(api.getSession as any).mockResolvedValue({ id: 'c1', template_id: 't1', override_config: {} })
    ;(api.listNodes as any).mockResolvedValue([
      { id: 'tmpl', owner_kind: 'session', owner_id: 'c1', parent_id: null, sort_order: 0,
        kind: 'ref', tag: null, definition_id: 'd1', enabled: true, created_at: '', meta: { _source: 'template' } },
      { id: 'fresh', owner_kind: 'session', owner_id: 'c1', parent_id: null, sort_order: 1,
        kind: 'ref', tag: null, definition_id: 'd2', enabled: true, created_at: '', meta: {} },
    ])
    libraryMock.definitions = [
      { id: 'd1', type: 'prompt', name: 'Tmpl', content: '', meta: {} },
      { id: 'd2', type: 'prompt', name: 'Fresh', content: '', meta: {} },
    ]
    const ui = useUiStore(); ui.setActiveChatId('c1')
    const w = mount(BookView)
    await flushPromises()
    await w.find('[data-test="customize-locally"]').trigger('click')
    await flushPromises()
    // NodeRow's per-id data-test is `node-row-${node.id}` (NodeRow.vue:132).
    expect(w.find('[data-test="node-row-tmpl"]').exists()).toBe(true)
    expect(w.find('[data-test="node-row-fresh"]').exists()).toBe(true)
  })

  it('deep-copies the definition so editing the local buffer cannot mutate the global definition', async () => {
    // template_id must be set so customize-locally actually flips customizedLocally
    // (materializeAll is otherwise a no-op) — without it the drill cannot run.
    ;(api.getSession as any).mockResolvedValue({ id: 'c1', template_id: 't1', override_config: {} })
    ;(api.listNodes as any).mockResolvedValue([
      { id: 'n1', owner_kind: 'session', owner_id: 'c1', parent_id: null, sort_order: 0,
        kind: 'ref', tag: null, definition_id: 'd1', enabled: true, created_at: '', meta: { _source: 'template' } },
    ])
    const globalDef = { id: 'd1', type: 'prompt', name: 'D1', content: 'global', meta: { trigger: { keys: ['a'] } } }
    libraryMock.definitions = [globalDef]
    const ui = useUiStore(); ui.setActiveChatId('c1')
    const w = mount(BookView)
    await flushPromises()
    await w.find('[data-test="customize-locally"]').trigger('click')
    await flushPromises()
    await w.find('[data-test="node-row-n1"]').trigger('click')   // drill -> editLocal('d1') -> deepClone
    await flushPromises()
    const editor = w.findComponent({ name: 'DefinitionEditor' })
    const localMeta = (editor.props('definition') as any).meta
    expect(localMeta).not.toBe(globalDef.meta)                 // top-level reference severed
    expect(localMeta.trigger).not.toBe(globalDef.meta.trigger) // nested reference severed
    expect(globalDef.meta.trigger).toEqual({ keys: ['a'] })    // global unchanged
    expect(globalDef.content).toBe('global')
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

  it('shows a hint when the import summary includes a converted panel', async () => {
    ;(api.importFile as any).mockResolvedValue({
      created: [{ kind: 'pack', id: 'p1', name: 'Neo' }, { kind: 'panel', id: 'p1', name: 'Neo' }],
      skipped: [],
      overwritten: [],
    })
    const w = mount(BookView)
    await flushPromises()
    const input = w.find('input[type="file"]').element as HTMLInputElement
    Object.defineProperty(input, 'files', { value: [new File(['x'], 'card.png')], configurable: true })
    await w.find('input[type="file"]').trigger('change')
    await flushPromises()
    expect(w.text()).toContain('Detected a status bar')
  })

  it('does not render a template-meta variables editor (global view)', async () => {
    libraryMock.templates = [{ id: 't1', name: 'T', meta: { variables: [{ name: 'hp', type: 'number', initial: 1 }] } }]
    const ui = useUiStore(); ui.setActiveChatId(null)
    const w = mount(BookView)
    await flushPromises()
    // a template is auto-selected, so the global template editor is shown…
    expect(w.find('[data-test="book-global"]').exists()).toBe(true)
    // …but it no longer hosts a (template-meta) VariablesEditor
    expect(w.findAllComponents(VariablesEditor).length).toBe(0)
  })

  it('still renders the per-chat local variables editor', async () => {
    const ui = useUiStore(); ui.setActiveChatId('c1')
    const w = mount(BookView)
    await flushPromises()
    expect(w.find('[data-test="local-variables"]').exists()).toBe(true)
  })
})

describe('BookView remembers selection', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    libraryMock.templates = [{ id: 't1', name: 'One', meta: {} }, { id: 't2', name: 'Two', meta: {} }]
    ;(api.getSession as any).mockResolvedValue({ id: 'c1', template_id: null, override_config: {} })
    ;(api.listNodes as any).mockClear()
  })

  it('restores the last-edited template from localStorage', async () => {
    localStorage.setItem('book.templateId', 't2')
    const ui = useUiStore(); ui.setActiveChatId(null)
    mount(BookView)
    await flushPromises()
    expect(api.listNodes).toHaveBeenCalledWith('template', 't2')
  })

  it('falls back to the first template when the saved id is gone', async () => {
    localStorage.setItem('book.templateId', 'deleted')
    const ui = useUiStore(); ui.setActiveChatId(null)
    mount(BookView)
    await flushPromises()
    expect(api.listNodes).toHaveBeenCalledWith('template', 't1')
  })
})

describe('BookView default template', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    libraryMock.templates = [{ id: 't1', name: 'One', meta: {} }]
    ;(api.getSession as any).mockResolvedValue({ id: 'c1', template_id: null, override_config: {} })
    ;(api.updateTemplate as any).mockClear()
  })

  it('flags the selected template as default on star click', async () => {
    const ui = useUiStore(); ui.setActiveChatId(null)
    const w = mount(BookView)
    await flushPromises()
    await w.get('[data-test="template-default"]').trigger('click')
    await flushPromises()
    expect(api.updateTemplate).toHaveBeenCalledWith('t1', 'One', { default: true })
  })
})
