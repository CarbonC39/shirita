import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import PromptTree from './PromptTree.vue'
import EntityPicker from './EntityPicker.vue'
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
    kind: 'ref', tag: null, definition_id: null, enabled: true, created_at: '', meta: {}, ...p }
}

describe('PromptTree omnibox add flow', () => {
  // The root omnibox is an EntityPicker whose items carry composite ids
  // ("container:<type>" | "prompt:<defId>" | "brick:<type>"); selecting one
  // dispatches to the matching tree mutation.
  async function openOmni(w: ReturnType<typeof mount>) {
    await w.find('[data-test="root-add"]').trigger('click') // rootOpen
    await w.find('[data-test="root-omnibox"] button').trigger('click') // open EntityPicker
  }
  async function pickOmni(w: ReturnType<typeof mount>, name: string) {
    await openOmni(w)
    const btn = w.findAll('[data-test="root-omnibox"] button').find((b) => b.text() === name)
    expect(btn, `omni item "${name}" missing`).toBeTruthy()
    await btn!.trigger('click')
  }

  it('lists container types and prompt defs together, excluding added containers', async () => {
    const nodes = [n({ id: 'f-char', kind: 'folder', tag: 'char', definition_id: null })]
    const w = mount(PromptTree, { props: { nodes, definitions: defs, types } })
    await openOmni(w)
    const items = w.findAll('[data-test="root-omnibox"] button').map((b) => b.text())
    expect(items.some((t) => t.includes('World'))).toBe(true)
    expect(items.some((t) => t.includes('Main'))).toBe(true)
    expect(items.some((t) => t.includes('Character'))).toBe(false) // char folder already exists
  })

  it('emits addContainer when a container item is chosen', async () => {
    const w = mount(PromptTree, { props: { nodes: [], definitions: defs, types } })
    await pickOmni(w, 'Character')
    expect(w.emitted('addContainer')![0]).toEqual(['char'])
  })

  it('emits addPrompt when a prompt item is chosen', async () => {
    const w = mount(PromptTree, { props: { nodes: [], definitions: defs, types } })
    await pickOmni(w, 'Main')
    expect(w.emitted('addPrompt')![0]).toEqual(['p1'])
  })

  it('carries a typed query into createNewPrompt via the create box', async () => {
    const w = mount(PromptTree, { props: { nodes: [], definitions: defs, types } })
    await w.find('[data-test="root-add"]').trigger('click')
    // intent-create (a query with no match) hands off to the inline name input.
    w.findComponent(EntityPicker).vm.$emit('intent-create', 'wor')
    await w.vm.$nextTick()
    const input = w.find('[data-test="omni-create-input"]')
    expect(input.exists()).toBe(true)
    await input.trigger('keydown', { key: 'Enter' })
    expect(w.emitted('createNewPrompt')![0]).toEqual(['wor'])
  })

  it('offers a Variables brick at the root and emits createNewInContainer(null, "variables")', async () => {
    const w = mount(PromptTree, { props: { nodes: [], definitions: defs, types } })
    await pickOmni(w, 'Variables')
    expect(w.emitted('createNewInContainer')![0]).toEqual([null, 'variables'])
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

  it('passes single-select to the children of a select=one folder', async () => {
    const nodes = [
      n({ id: 'f', kind: 'folder', tag: 'style', definition_id: null, meta: { select: 'one' } }),
      n({ id: 'c', kind: 'ref', parent_id: 'f', definition_id: 'c1' }),
    ]
    const w = mount(PromptTree, { props: { nodes, definitions: defs, types } })
    await w.find('[data-test="expand-btn"]').trigger('click')
    expect(w.find('[data-test="enable-radio"]').exists()).toBe(true)
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

  it('arms dataTransfer on dragstart so native DnD actually carries the drag past the source row', async () => {
    // Real browsers (Firefox in particular) only continue a native HTML5 drag
    // past the source element if dragstart calls dataTransfer.setData; without
    // it the row looks draggable but dragover/drop never fire on other rows.
    // jsdom doesn't enforce this, so we assert the call directly.
    const nodes = [
      n({ id: 'a', kind: 'folder', tag: 'char', definition_id: null, sort_order: 0 }),
      n({ id: 'b', kind: 'folder', tag: 'world', definition_id: null, sort_order: 1 }),
    ]
    const w = mount(PromptTree, { props: { nodes, definitions: defs, types } })
    const rows = w.findAll('[data-test="row-wrap"]')
    const setData = vi.fn()
    await rows[0].find('[data-test="drag-handle"]').trigger('mousedown')
    await rows[0].trigger('dragstart', { dataTransfer: { setData, effectAllowed: '' } })
    expect(setData).toHaveBeenCalledWith('text/plain', 'a')
  })
})

describe('PromptTree regex brick', () => {
  it('offers a Regex brick that creates a regex_rule at root', async () => {
    const w = mount(PromptTree, { props: { nodes: [], definitions: [], types: [] } })
    await w.find('[data-test="root-add"]').trigger('click')
    await w.find('[data-test="root-omnibox"] button').trigger('click')
    const btn = w.findAll('[data-test="root-omnibox"] button').find((b) => b.text() === 'Regex')
    expect(btn).toBeTruthy()
    await btn!.trigger('click')
    expect(w.emitted('createNewInContainer')![0]).toEqual([null, 'regex_rule'])
  })
})

describe('PromptTree openDefinition', () => {
  it('emits openDefinition with the definition_id when a ref row is clicked', async () => {
    const makeNode = (id: string, definitionId: string | null) => ({
      id, owner_kind: 'session' as const, owner_id: 's', parent_id: null,
      sort_order: 0, kind: 'ref' as const, tag: null, definition_id: definitionId,
      enabled: true, created_at: '', meta: {},
    })
    const w = mount(PromptTree, {
      props: { nodes: [makeNode('n1', 'def-1')], definitions: [], types: [] },
    })
    await w.find('[data-test="node-row-n1"]').trigger('click')
    expect(w.emitted('openDefinition')).toEqual([['def-1']])
  })
})
