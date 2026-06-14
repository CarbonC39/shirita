import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import NodeRow from './NodeRow.vue'
import type { Definition, PromptNode } from '../api/types'

const defs: Record<string, Definition> = {
  d1: { id: 'd1', type: 'char', name: 'Neo', content: 'body', meta: {} },
}
function node(p: Partial<PromptNode>): PromptNode {
  return { id: 'n1', owner_kind: 'template', owner_id: 't', parent_id: null, sort_order: 0,
    kind: 'ref', tag: null, definition_id: 'd1', enabled: true, created_at: '', ...p }
}

describe('NodeRow', () => {
  it('emits delete when the delete button is clicked', async () => {
    const w = mount(NodeRow, { props: { node: node({}), definitions: defs, depth: 0, isExpanded: false } })
    await w.find('[data-test="node-delete"]').trigger('click')
    expect(w.emitted('delete')).toBeTruthy()
  })

  it('history row shows the Chat history label and no delete button', () => {
    const h = node({ kind: 'history', definition_id: null, tag: null })
    const w = mount(NodeRow, { props: { node: h, definitions: defs, depth: 0, isExpanded: false } })
    expect(w.text()).toContain('Chat history')
    expect(w.find('[data-test="node-delete"]').exists()).toBe(false)
  })
})
