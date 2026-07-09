import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import DefinitionSection from './DefinitionSection.vue'
import EntityToolbar from '../EntityToolbar.vue'
import DefinitionEditor from '../DefinitionEditor.vue'
import type { Definition } from '../../api/types'

const editDef: Definition = { id: 'd1', type: 'prompt', name: 'Greet', content: 'hi', meta: {} }

function mountSection(overrides: Record<string, any> = {}) {
  return mount(DefinitionSection, {
    props: {
      items: [{ id: 'd1', name: 'Greet' }],
      selectedId: 'd1',
      selectedLabel: 'Greet',
      placeholder: 'Pick definition…',
      createLabel: 'New definition',
      editDef,
      editDefActive: true,
      editDefSavedTick: 0,
      types: [],
      ...overrides,
    },
    global: { stubs: { DefinitionEditor: true } },
  })
}

describe('DefinitionSection', () => {
  it('renders the definition heading', () => {
    expect(mountSection().find('[data-test="section-definition"]').exists()).toBe(true)
  })

  it('forwards the core props to the EntityToolbar', () => {
    const tb = mountSection().findComponent(EntityToolbar)
    expect(tb.props('items')).toEqual([{ id: 'd1', name: 'Greet' }])
    expect(tb.props('selectedId')).toBe('d1')
    expect(tb.props('selectedLabel')).toBe('Greet')
    expect(tb.props('createLabel')).toBe('New definition')
  })

  it('bubbles toolbar select / create / rename events', async () => {
    const w = mountSection()
    const tb = w.findComponent(EntityToolbar)
    tb.vm.$emit('select', 'd2')
    tb.vm.$emit('create', 'New')
    tb.vm.$emit('rename', 'Renamed')
    await w.vm.$nextTick()
    expect(w.emitted('select')).toEqual([['d2']])
    expect(w.emitted('create')).toEqual([['New']])
    expect(w.emitted('rename')).toEqual([['Renamed']])
  })

  it('relays editor save and field updates with the def- prefix', async () => {
    const w = mountSection()
    const editor = w.findComponent(DefinitionEditor)
    await editor.vm.$emit('save')
    await editor.vm.$emit('update:name', 'Renamed')
    await editor.vm.$emit('update:type', 'lore')
    await editor.vm.$emit('update:content', 'body')
    await editor.vm.$emit('update:meta', { x: 1 })
    expect(w.emitted('save')).toHaveLength(1)
    expect(w.emitted('update:def-name')).toEqual([['Renamed']])
    expect(w.emitted('update:def-type')).toEqual([['lore']])
    expect(w.emitted('update:def-content')).toEqual([['body']])
    expect(w.emitted('update:def-meta')).toEqual([[{ x: 1 }]])
  })

  it('relays type create / delete events', async () => {
    const w = mountSection()
    const editor = w.findComponent(DefinitionEditor)
    await editor.vm.$emit('create-type', 'Faction')
    await editor.vm.$emit('delete-type', 'faction')
    expect(w.emitted('create-type')).toEqual([['Faction']])
    expect(w.emitted('delete-type')).toEqual([['faction']])
  })
})
