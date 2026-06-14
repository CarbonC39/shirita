import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import NodePicker from './NodePicker.vue'
import type { Definition, DefType } from '../api/types'

const defs: Definition[] = [
  { id: 'd1', type: 'char', name: 'Neo', content: '', meta: {} },
  { id: 'd2', type: 'world', name: 'Zion', content: '', meta: {} },
]
const types: DefType[] = [
  { id: 'char', label: 'Character', sort: 0, builtin: true, created_at: '' },
  { id: 'world', label: 'World', sort: 1, builtin: true, created_at: '' },
]

describe('NodePicker', () => {
  it('filters definitions to the picker type', () => {
    const w = mount(NodePicker, { props: { definitions: defs, filterType: 'char', types } })
    expect(w.text()).toContain('Neo')
    expect(w.text()).not.toContain('Zion')
  })

  it('emits select with the definition id', async () => {
    const w = mount(NodePicker, { props: { definitions: defs, filterType: 'char', types } })
    await w.findAll('button').find((b) => b.text().includes('Neo'))!.trigger('click')
    expect(w.emitted('select')![0]).toEqual(['d1'])
  })
})
