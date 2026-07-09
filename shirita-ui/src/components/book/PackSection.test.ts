import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import PackSection from './PackSection.vue'
import EntityToolbar from '../EntityToolbar.vue'
import PackEditor from '../PackEditor.vue'
import type { Pack } from '../../api/types'

const pack: Pack = {
  id: 'p1', name: 'Alice',
  identity: { display_name: 'Alice', avatar: null },
  meta: {}, created_at: '', updated_at: '',
}

function mountSection(overrides: Record<string, any> = {}) {
  return mount(PackSection, {
    props: {
      items: [{ id: 'p1', name: 'Alice' }],
      selectedId: 'p1',
      selectedLabel: 'Alice',
      placeholder: 'Pick pack…',
      createLabel: 'New pack',
      selectedPack: pack,
      ...overrides,
    },
    global: { stubs: { PackEditor: true } },
  })
}

describe('PackSection', () => {
  it('renders the pack heading', () => {
    expect(mountSection().find('[data-test="section-pack"]').exists()).toBe(true)
  })

  it('forwards the core props to the EntityToolbar', () => {
    const tb = mountSection().findComponent(EntityToolbar)
    expect(tb.props('items')).toEqual([{ id: 'p1', name: 'Alice' }])
    expect(tb.props('selectedId')).toBe('p1')
    expect(tb.props('selectedLabel')).toBe('Alice')
    expect(tb.props('createLabel')).toBe('New pack')
  })

  it('bubbles toolbar select / create / delete events', async () => {
    const w = mountSection()
    const tb = w.findComponent(EntityToolbar)
    tb.vm.$emit('select', 'p2')
    tb.vm.$emit('create', 'Bob')
    tb.vm.$emit('delete')
    await w.vm.$nextTick()
    expect(w.emitted('select')).toEqual([['p2']])
    expect(w.emitted('create')).toEqual([['Bob']])
    expect(w.emitted('delete')).toHaveLength(1)
  })

  it('mounts the pack editor only when a pack is selected', () => {
    expect(mountSection().findComponent(PackEditor).exists()).toBe(true)
    expect(mountSection({ selectedPack: null }).findComponent(PackEditor).exists()).toBe(false)
  })

  it('relays PackEditor change events as packChanged', async () => {
    const w = mountSection()
    await w.findComponent(PackEditor).vm.$emit('changed')
    expect(w.emitted('packChanged')).toHaveLength(1)
  })
})
