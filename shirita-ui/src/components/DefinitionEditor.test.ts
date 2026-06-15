import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import DefinitionEditor from './DefinitionEditor.vue'

const def = { id: 'd', type: 'world', name: 'Zion', content: '', meta: { trigger: { mode: 'keyword', keys: ['zion'], probability: 100 } } }

describe('DefinitionEditor trigger', () => {
  it('shows the trigger editor for a world definition with the existing keyword', () => {
    const w = mount(DefinitionEditor, { props: { definition: def, allDefinitions: [def], active: true } })
    expect(w.find('[data-test="trigger-editor"]').exists()).toBe(true)
    expect(w.text()).toContain('zion')
  })

  it('hides the trigger editor for a prompt definition', () => {
    const p = { ...def, type: 'prompt', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: p, allDefinitions: [p], active: true } })
    expect(w.find('[data-test="trigger-editor"]').exists()).toBe(false)
  })
})

describe('DefinitionEditor reveal', () => {
  it('hides the editor body until a definition is active', () => {
    const d = { id: 'd', type: 'char', name: 'Neo', content: '', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d] } })
    // picker is always present; body (type chips, save) is not until active
    expect(w.findAll('[data-test="type-chip"]')).toHaveLength(0)
    expect(w.find('[data-test="save-btn"]').exists()).toBe(false)
  })
})

describe('DefinitionEditor type chips', () => {
  it('renders type chips from the provided types plus prompt', () => {
    const types = [
      { id: 'char', label: 'Character', sort: 0, builtin: true, created_at: '' },
      { id: 'world', label: 'World', sort: 1, builtin: true, created_at: '' },
    ]
    const d = { id: 'd', type: 'char', name: 'Neo', content: '', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], types, active: true } })
    const chips = w.findAll('[data-test="type-chip"]').map((b) => b.text())
    expect(chips).toEqual(['Character', 'World', 'Prompt'])
  })

  it('only offers delete on custom (non-builtin) types', () => {
    const types = [
      { id: 'char', label: 'Character', sort: 0, builtin: true, created_at: '' },
      { id: 'faction', label: 'Faction', sort: 1, builtin: false, created_at: '' },
    ]
    const d = { id: 'd', type: 'char', name: 'Neo', content: '', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], types, active: true } })
    // one delete button — for the custom 'faction' type only (char + prompt are builtin)
    expect(w.findAll('[data-test="type-delete"]')).toHaveLength(1)
  })

  it('emits create-type with the typed name', async () => {
    const d = { id: 'd', type: 'char', name: 'Neo', content: '', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], types: [], active: true } })
    await w.find('[data-test="type-new"]').trigger('click')
    await w.find('[data-test="type-new-input"]').setValue('Faction')
    await w.find('[data-test="type-new-input"]').trigger('keyup.enter')
    expect(w.emitted('create-type')![0]).toEqual(['Faction'])
  })
})
