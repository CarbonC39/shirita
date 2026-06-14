import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import DefinitionEditor from './DefinitionEditor.vue'

const def = { id: 'd', type: 'world', name: 'Zion', content: '', meta: { trigger: { mode: 'keyword', keys: ['zion'], probability: 100 } } }

describe('DefinitionEditor trigger', () => {
  it('shows the trigger editor for a world definition with the existing keyword', () => {
    const w = mount(DefinitionEditor, { props: { definition: def, allDefinitions: [def] } })
    expect(w.find('[data-test="trigger-editor"]').exists()).toBe(true)
    expect(w.text()).toContain('zion')
  })

  it('hides the trigger editor for a prompt definition', () => {
    const p = { ...def, type: 'prompt', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: p, allDefinitions: [p] } })
    expect(w.find('[data-test="trigger-editor"]').exists()).toBe(false)
  })
})

describe('DefinitionEditor type chips', () => {
  it('renders type chips from the provided types plus prompt', () => {
    const types = [
      { id: 'char', label: 'Character', sort: 0, builtin: true, created_at: '' },
      { id: 'world', label: 'World', sort: 1, builtin: true, created_at: '' },
    ]
    const d = { id: 'd', type: 'char', name: 'Neo', content: '', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], types } })
    const chips = w.findAll('[data-test="type-chip"]').map((b) => b.text())
    expect(chips).toEqual(['Character', 'World', 'Prompt'])
  })
})
