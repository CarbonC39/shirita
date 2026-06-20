import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import NodeRow from './NodeRow.vue'
import type { Definition, PromptNode } from '../api/types'

const defs: Record<string, Definition> = {
  d1: { id: 'd1', type: 'char', name: 'Neo', content: 'body', meta: {} },
}
function node(p: Partial<PromptNode>): PromptNode {
  return { id: 'n1', owner_kind: 'template', owner_id: 't', parent_id: null, sort_order: 0,
    kind: 'ref', tag: null, definition_id: 'd1', enabled: true, created_at: '', meta: {}, ...p }
}

describe('NodeRow', () => {
  it('emits delete when the delete button is clicked', async () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 0, isExpanded: false } })
    await w.find('[data-test="node-delete"]').trigger('click')
    expect(w.emitted('delete')).toBeTruthy()
  })

  it('folder rows show an add button beside delete and emit add', async () => {
    const folder = node({ kind: 'folder', tag: 'char', definition_id: null })
    const w = mount(NodeRow, { props: { node: folder, definitions: defs, depth: 0, isExpanded: false } })
    const addBtn = w.find('[data-test="node-add"]')
    expect(addBtn.exists()).toBe(true)
    await addBtn.trigger('click')
    expect(w.emitted('add')).toBeTruthy()
  })

  it('ref rows have no add button', () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 0, isExpanded: false } })
    expect(w.find('[data-test="node-add"]').exists()).toBe(false)
  })

  it('history row shows the Chat history label and no delete button', () => {
    const h = node({ kind: 'history', definition_id: null, tag: null })
    const w = mount(NodeRow, { props: { node: h, definitions: defs, depth: 0, isExpanded: false } })
    expect(w.text()).toContain('Chat history')
    expect(w.find('[data-test="node-delete"]').exists()).toBe(false)
  })

  it('content row shows the mounted-packs label, an enable toggle, and no delete/add', () => {
    const c = node({ kind: 'content', definition_id: null, tag: null })
    const w = mount(NodeRow, { props: { node: c, definitions: defs, depth: 0, isExpanded: false } })
    expect(w.text()).toContain('Mounted packs')
    expect(w.find('[data-test="enable-checkbox"]').exists()).toBe(true)
    expect(w.find('[data-test="node-delete"]').exists()).toBe(false)
    expect(w.find('[data-test="node-add"]').exists()).toBe(false)
  })

  it('shows the trigger editor in an expanded container ref', () => {
    const worldDefs = { d1: { id: 'd1', type: 'world', name: 'Zion', content: 'b', meta: { trigger: { mode: 'keyword', keys: ['zion'], probability: 100 } } } }
    const ref = node({ kind: 'ref', definition_id: 'd1' })
    const w = mount(NodeRow, { props: { node: ref, definitions: worldDefs, depth: 1, isExpanded: true } })
    expect(w.find('[data-test="trigger-editor"]').exists()).toBe(true)
  })
})
