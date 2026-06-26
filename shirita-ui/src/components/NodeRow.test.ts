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

  it('keeps the delete button visible (not opacity-0) for touch devices', () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 0, isExpanded: false } })
    const del = w.get('[data-test="node-delete"]')
    expect(del.classes()).not.toContain('text-muted/0')
    expect(del.classes()).toContain('text-muted/40')
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

  it('ref rows have no select-mode switch', () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 0, isExpanded: false } })
    expect(w.find('[data-test="select-mode"]').exists()).toBe(false)
  })

  it('folder select-mode defaults to All and toggles to one via updateNodeMeta', async () => {
    const folder = node({ kind: 'folder', tag: 'style', definition_id: null, meta: {} })
    const w = mount(NodeRow, { props: { node: folder, definitions: defs, depth: 0, isExpanded: false } })
    const btn = w.find('[data-test="select-mode"]')
    expect(btn.exists()).toBe(true)
    expect(btn.text()).toBe('All')
    await btn.trigger('click')
    expect(w.emitted('updateNodeMeta')![0]).toEqual([{ select: 'one' }])
  })

  it('folder select-mode reads an existing meta.select=one as Single', () => {
    const folder = node({ kind: 'folder', tag: 'style', definition_id: null, meta: { select: 'one' } })
    const w = mount(NodeRow, { props: { node: folder, definitions: defs, depth: 0, isExpanded: false } })
    expect(w.find('[data-test="select-mode"]').text()).toBe('Single')
  })

  it('content row shows the mounted-packs label, an enable toggle, and no delete/add', () => {
    const c = node({ kind: 'content', definition_id: null, tag: null })
    const w = mount(NodeRow, { props: { node: c, definitions: defs, depth: 0, isExpanded: false } })
    expect(w.text()).toContain('Mounted packs')
    expect(w.find('[data-test="enable-checkbox"]').exists()).toBe(true)
    expect(w.find('[data-test="node-delete"]').exists()).toBe(false)
    expect(w.find('[data-test="node-add"]').exists()).toBe(false)
  })

  it('renders a radio enable control for a ref when single-select is set', () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 1, isExpanded: false, singleSelect: true } })
    expect(w.find('[data-test="enable-radio"]').exists()).toBe(true)
    expect(w.find('[data-test="enable-checkbox"]').exists()).toBe(false)
  })

  it('keeps the square checkbox when single-select is not set', () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 1, isExpanded: false } })
    expect(w.find('[data-test="enable-checkbox"]').exists()).toBe(true)
    expect(w.find('[data-test="enable-radio"]').exists()).toBe(false)
  })

  it('radio still emits toggleEnabled on click', async () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 1, isExpanded: false, singleSelect: true } })
    await w.find('[data-test="enable-radio"]').trigger('click')
    expect(w.emitted('toggleEnabled')).toBeTruthy()
  })

  it('shows the trigger editor in an expanded container ref', () => {
    const worldDefs = { d1: { id: 'd1', type: 'world', name: 'Zion', content: 'b', meta: { trigger: { mode: 'keyword', keys: ['zion'], probability: 100 } } } }
    const ref = node({ kind: 'ref', definition_id: 'd1' })
    const w = mount(NodeRow, { props: { node: ref, definitions: worldDefs, depth: 1, isExpanded: true } })
    expect(w.find('[data-test="trigger-editor"]').exists()).toBe(true)
  })
})

describe('NodeRow regex editing', () => {
  const rxDefs: Record<string, Definition> = {
    r1: { id: 'r1', type: 'regex_rule', name: 'Clean', content: '', meta: { pattern: 'a', replacement: 'b' } },
  }
  function rxNode(): PromptNode {
    return { id: 'n1', owner_kind: 'template', owner_id: 't', parent_id: null, sort_order: 0,
      kind: 'ref', tag: null, definition_id: 'r1', enabled: true, created_at: '', meta: {} }
  }

  it('shows find/replace inputs and no content textarea for a regex ref', () => {
    const w = mount(NodeRow, { props: { node: rxNode(), definitions: rxDefs, depth: 0, isExpanded: true } })
    expect(w.find('[data-test="regex-find"]').exists()).toBe(true)
    expect(w.find('[data-test="node-content"]').exists()).toBe(false)
  })

  it('emits updateDefMeta when the pattern changes', async () => {
    const w = mount(NodeRow, { props: { node: rxNode(), definitions: rxDefs, depth: 0, isExpanded: true } })
    await w.find('[data-test="regex-find"]').setValue('xyz')
    const ev = w.emitted('updateDefMeta')
    expect(ev).toBeTruthy()
    expect((ev![ev!.length - 1][0] as Record<string, unknown>).pattern).toBe('xyz')
  })

  it('emits updateDefName when the name changes', async () => {
    const w = mount(NodeRow, { props: { node: rxNode(), definitions: rxDefs, depth: 0, isExpanded: true } })
    await w.find('[data-test="regex-name"]').setValue('Renamed')
    const ev = w.emitted('updateDefName')
    expect(ev).toBeTruthy()
    expect(ev![ev!.length - 1][0]).toBe('Renamed')
  })
})
