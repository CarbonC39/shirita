import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import PromptTree from './PromptTree.vue'
import type { Definition, DefType, PromptNode } from '../api/types'

const types: DefType[] = [
  { id: 'char', label: 'Character', sort: 0, builtin: true, created_at: '' },
  { id: 'world', label: 'World', sort: 1, builtin: true, created_at: '' },
]
const defs: Definition[] = [
  { id: 'p1', type: 'prompt', name: 'Main', content: '', meta: {} },
  { id: 'c1', type: 'char', name: 'Neo', content: '', meta: {} },
]
function n(p: Partial<PromptNode>): PromptNode {
  return { id: 'x', owner_kind: 'template', owner_id: 't', parent_id: null, sort_order: 0,
    kind: 'ref', tag: null, definition_id: null, enabled: true, created_at: '', ...p }
}

describe('PromptTree add flows', () => {
  it('Add container lists only types without an existing container', async () => {
    const nodes = [n({ id: 'f-char', kind: 'folder', tag: 'char', definition_id: null })]
    const w = mount(PromptTree, { props: { nodes, definitions: defs, types } })
    await w.find('[data-test="root-add"]').trigger('click')
    await w.find('[data-test="add-container"]').trigger('click')
    // char already has a container → only world offered
    const labels = w.findAll('[data-test="container-type-option"]').map((b) => b.text())
    expect(labels).toEqual(['World'])
  })

  it('Add container emits addContainer with the chosen type', async () => {
    const w = mount(PromptTree, { props: { nodes: [], definitions: defs, types } })
    await w.find('[data-test="root-add"]').trigger('click')
    await w.find('[data-test="add-container"]').trigger('click')
    await w.findAll('[data-test="container-type-option"]')[0].trigger('click')
    expect(w.emitted('addContainer')![0]).toEqual(['char'])
  })
})

describe('PromptTree drag reorder', () => {
  it('emits reorder with the new root order on drop', async () => {
    const nodes = [
      n({ id: 'a', kind: 'folder', tag: 'char', definition_id: null, sort_order: 0 }),
      n({ id: 'b', kind: 'folder', tag: 'world', definition_id: null, sort_order: 1 }),
    ]
    const w = mount(PromptTree, { props: { nodes, definitions: defs, types } })
    const rows = w.findAll('[data-test="row-wrap"]')
    await rows[0].trigger('dragstart')
    await rows[1].trigger('drop')
    expect(w.emitted('reorder')![0]).toEqual([['b', 'a']])
  })
})
