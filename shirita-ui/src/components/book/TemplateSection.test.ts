import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import TemplateSection from './TemplateSection.vue'
import EntityToolbar from '../EntityToolbar.vue'
import PromptTree from '../PromptTree.vue'

function mountSection(overrides: Record<string, any> = {}) {
  return mount(TemplateSection, {
    props: {
      items: [{ id: 't1', name: 'Main' }],
      selectedId: 't1',
      selectedLabel: 'Main',
      placeholder: 'Pick template…',
      createLabel: 'New template',
      isDefault: true,
      nodes: [],
      definitions: [],
      types: [],
      ...overrides,
    },
    global: { stubs: { PromptTree: true } },
  })
}

describe('TemplateSection', () => {
  it('renders the template heading', () => {
    expect(mountSection().find('[data-test="section-template"]').exists()).toBe(true)
  })

  it('forwards the core props to the EntityToolbar, including the default-star flags', () => {
    const tb = mountSection().findComponent(EntityToolbar)
    expect(tb.props('items')).toEqual([{ id: 't1', name: 'Main' }])
    expect(tb.props('selectedId')).toBe('t1')
    expect(tb.props('selectedLabel')).toBe('Main')
    expect(tb.props('createLabel')).toBe('New template')
    expect(tb.props('showDefault')).toBe(true)
    expect(tb.props('isDefault')).toBe(true)
  })

  it('bubbles toolbar select / create / delete / toggle-default events', async () => {
    const w = mountSection()
    const tb = w.findComponent(EntityToolbar)
    tb.vm.$emit('select', 't2')
    tb.vm.$emit('create', 'Combat')
    tb.vm.$emit('delete')
    tb.vm.$emit('toggle-default')
    await w.vm.$nextTick()
    expect(w.emitted('select')).toEqual([['t2']])
    expect(w.emitted('create')).toEqual([['Combat']])
    expect(w.emitted('delete')).toHaveLength(1)
    expect(w.emitted('toggle-default')).toHaveLength(1)
  })

  it('mounts the prompt tree only when a template is selected', () => {
    expect(mountSection().findComponent(PromptTree).exists()).toBe(true)
    expect(mountSection({ selectedId: null }).findComponent(PromptTree).exists()).toBe(false)
  })

  it('relays PromptTree reorder and delete events', async () => {
    const w = mountSection()
    const tree = w.findComponent(PromptTree)
    await tree.vm.$emit('reorder', ['n2', 'n1'])
    await tree.vm.$emit('delete-node', 'n1')
    await tree.vm.$emit('toggle-enabled', 'n3')
    expect(w.emitted('reorder')).toEqual([[['n2', 'n1']]])
    expect(w.emitted('deleteNode')).toEqual([['n1']])
    expect(w.emitted('toggleEnabled')).toEqual([['n3']])
  })

  it('relays the two-argument create/add events unchanged', async () => {
    const w = mountSection()
    const tree = w.findComponent(PromptTree)
    await tree.vm.$emit('add-ref-to-container', 'folder-1', 'd9')
    await tree.vm.$emit('create-new-in-container', null, 'lore')
    await tree.vm.$emit('update-content', 'd1', 'hi')
    expect(w.emitted('addRefToContainer')).toEqual([['folder-1', 'd9']])
    expect(w.emitted('createNewInContainer')).toEqual([[null, 'lore']])
    expect(w.emitted('updateContent')).toEqual([['d1', 'hi']])
  })
})
