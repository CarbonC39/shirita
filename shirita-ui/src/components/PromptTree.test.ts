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

describe('PromptTree omnibox add flow', () => {
  it('lists container types and prompt defs together, excluding added containers', async () => {
    const nodes = [n({ id: 'f-char', kind: 'folder', tag: 'char', definition_id: null })]
    const w = mount(PromptTree, { props: { nodes, definitions: defs, types } })
    await w.find('[data-test="root-add"]').trigger('click')
    // char container already exists → world (container) + Main (prompt)
    const items = w.findAll('[data-test="omni-item"]').map((b) => b.text())
    expect(items.some((t) => t.includes('World'))).toBe(true)
    expect(items.some((t) => t.includes('Main'))).toBe(true)
    expect(items.some((t) => t.includes('Character'))).toBe(false)
  })

  it('emits addContainer when a container item is chosen', async () => {
    const w = mount(PromptTree, { props: { nodes: [], definitions: defs, types } })
    await w.find('[data-test="root-add"]').trigger('click')
    // containers come first: [Character, World], then prompt [Main]
    await w.findAll('[data-test="omni-item"]')[0].trigger('click')
    expect(w.emitted('addContainer')![0]).toEqual(['char'])
  })

  it('emits addPrompt when a prompt item is chosen', async () => {
    const w = mount(PromptTree, { props: { nodes: [], definitions: defs, types } })
    await w.find('[data-test="root-add"]').trigger('click')
    const main = w.findAll('[data-test="omni-item"]').find((b) => b.text().includes('Main'))!
    await main.trigger('click')
    expect(w.emitted('addPrompt')![0]).toEqual(['p1'])
  })

  it('typing offers create rows that carry the query', async () => {
    const w = mount(PromptTree, { props: { nodes: [], definitions: defs, types } })
    await w.find('[data-test="root-add"]').trigger('click')
    await w.find('[data-test="omni-input"]').setValue('wor')
    await w.find('[data-test="omni-new-prompt"]').trigger('click')
    expect(w.emitted('createNewPrompt')![0]).toEqual(['wor'])

    const w2 = mount(PromptTree, { props: { nodes: [], definitions: defs, types } })
    await w2.find('[data-test="root-add"]').trigger('click')
    await w2.find('[data-test="omni-input"]').setValue('Lore')
    await w2.find('[data-test="omni-new-type"]').trigger('click')
    expect(w2.emitted('createType')![0]).toEqual(['Lore'])
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
    // drag only arms when it starts on the grip handle
    await rows[0].find('[data-test="drag-handle"]').trigger('mousedown')
    await rows[0].trigger('dragstart')
    await rows[1].trigger('drop')
    expect(w.emitted('reorder')![0]).toEqual([['b', 'a']])
  })

  it('ignores a drag that did not start on the grip handle', async () => {
    const nodes = [
      n({ id: 'a', kind: 'folder', tag: 'char', definition_id: null, sort_order: 0 }),
      n({ id: 'b', kind: 'folder', tag: 'world', definition_id: null, sort_order: 1 }),
    ]
    const w = mount(PromptTree, { props: { nodes, definitions: defs, types } })
    const rows = w.findAll('[data-test="row-wrap"]')
    await rows[0].trigger('mousedown') // not on the handle
    await rows[0].trigger('dragstart')
    await rows[1].trigger('drop')
    expect(w.emitted('reorder')).toBeUndefined()
  })
})
