import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import DefinitionEditor from './DefinitionEditor.vue'
import AssetPicker from './AssetPicker.vue'

// AssetPicker uses the media Pinia store, so component mounts need an active Pinia.
const plugins = { plugins: [createPinia()] }

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

describe('DefinitionEditor wrap_in_tag', () => {
  it('emits update:meta with wrap_in_tag toggled', async () => {
    const d = { id: 'd1', type: 'char', name: 'Alice', content: 'body', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true } })
    const box = w.get('[data-test="wrap-in-tag"]')
    await box.trigger('click')
    const events = w.emitted('update:meta')
    expect(events).toBeTruthy()
    const last = events![events!.length - 1][0] as Record<string, unknown>
    expect(last.wrap_in_tag).toBe(true)
  })
})

describe('DefinitionEditor header actions', () => {
  const d = { id: 'd1', type: 'char', name: 'Alice', content: '', meta: {} }

  it('emits import and export when the header icons are clicked', async () => {
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true } })
    await w.get('[data-test="import-btn"]').trigger('click')
    await w.get('[data-test="export-btn"]').trigger('click')
    expect(w.emitted('import')).toBeTruthy()
    expect(w.emitted('export')).toBeTruthy()
  })

  it('hides the header action row when headerActions is false', () => {
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], headerActions: false } })
    expect(w.find('[data-test="import-btn"]').exists()).toBe(false)
    expect(w.find('[data-test="export-btn"]').exists()).toBe(false)
    expect(w.find('[data-test="delete-btn"]').exists()).toBe(false)
  })
})

describe('DefinitionEditor persona avatar', () => {
  it('shows an avatar picker for persona and emits update:meta on pick', async () => {
    const d = { id: 'p1', type: 'persona', name: 'Me', content: '', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true }, global: plugins })
    const wrap = w.find('[data-test="persona-avatar"]')
    expect(wrap.exists()).toBe(true)
    wrap.findComponent(AssetPicker).vm.$emit('update:modelValue', 'u.png')
    await w.vm.$nextTick()
    const ev = w.emitted('update:meta')
    expect(ev).toBeTruthy()
    expect((ev![ev!.length - 1][0] as Record<string, unknown>).avatar).toBe('u.png')
  })

  it('does not show the avatar picker for a char definition', () => {
    const d = { id: 'c1', type: 'char', name: 'Neo', content: '', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true } })
    expect(w.find('[data-test="persona-avatar"]').exists()).toBe(false)
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

describe('DefinitionEditor disabled state with no selection', () => {
  const d = { id: 'd', type: 'char', name: 'Neo', content: '', meta: {} }

  it('disables rename/export/duplicate/delete and keeps import enabled when nothing is selected', () => {
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: false } })
    expect((w.get('[data-test="rename-btn"]').element as HTMLButtonElement).disabled).toBe(true)
    expect((w.get('[data-test="export-btn"]').element as HTMLButtonElement).disabled).toBe(true)
    expect((w.get('[data-test="duplicate-btn"]').element as HTMLButtonElement).disabled).toBe(true)
    expect((w.get('[data-test="delete-btn"]').element as HTMLButtonElement).disabled).toBe(true)
    expect((w.get('[data-test="import-btn"]').element as HTMLButtonElement).disabled).toBe(false)
  })

  it('does not emit delete/export/duplicate when clicked while disabled', async () => {
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: false } })
    await w.get('[data-test="delete-btn"]').trigger('click')
    await w.get('[data-test="export-btn"]').trigger('click')
    await w.get('[data-test="duplicate-btn"]').trigger('click')
    expect(w.emitted('delete')).toBeFalsy()
    expect(w.emitted('export')).toBeFalsy()
    expect(w.emitted('duplicate')).toBeFalsy()
  })

  it('enables all action buttons once a definition is active', () => {
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true } })
    expect((w.get('[data-test="rename-btn"]').element as HTMLButtonElement).disabled).toBe(false)
    expect((w.get('[data-test="export-btn"]').element as HTMLButtonElement).disabled).toBe(false)
    expect((w.get('[data-test="duplicate-btn"]').element as HTMLButtonElement).disabled).toBe(false)
    expect((w.get('[data-test="delete-btn"]').element as HTMLButtonElement).disabled).toBe(false)
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
    expect(chips).toEqual(['Character', 'World', 'Prompt', 'Message'])
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

describe('DefinitionEditor html preview', () => {
  it('shows a PanelView preview for html definitions', () => {
    const def = { id: 'h', type: 'html', name: 'm', content: '<b>hi</b>', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: def, allDefinitions: [def], active: true } })
    expect(w.find('[data-test="html-preview"]').exists()).toBe(true)
  })

  it('does not show the preview for non-html definitions', () => {
    const def = { id: 'c', type: 'css', name: 'm', content: 'body{}', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: def, allDefinitions: [def], active: true } })
    expect(w.find('[data-test="html-preview"]').exists()).toBe(false)
  })

  it('html/css are leaf bricks: content editor + (html) preview, no container UI', () => {
    for (const type of ['html', 'css'] as const) {
      const def = { id: type, type, name: 'm', content: 'x', meta: {} }
      const w = mount(DefinitionEditor, { props: { definition: def, allDefinitions: [def], active: true } })
      // content editor is present
      expect(w.find('textarea').exists()).toBe(true)
      // html shows the preview; css does not
      expect(w.find('[data-test="html-preview"]').exists()).toBe(type === 'html')
      // no container UI: trigger editor, scan-depth control, or wrap-in-tag toggle
      expect(w.find('[data-test="trigger-editor"]').exists()).toBe(false)
      expect(w.find('[data-test="scan-depth"]').exists()).toBe(false)
      expect(w.find('[data-test="wrap-in-tag"]').exists()).toBe(false)
    }
  })
})

describe('DefinitionEditor message type', () => {
  it('shows depth/role fields for first_message and hides world-info fields', () => {
    const d = { id: 'm1', type: 'first_message', name: 'Greeting', content: 'Hi', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true } })
    expect(w.find('[data-test="message-type-fields"]').exists()).toBe(true)
    expect(w.find('[data-test="trigger-editor"]').exists()).toBe(false)
    expect(w.find('[data-test="scan-depth"]').exists()).toBe(false)
    expect(w.find('[data-test="wrap-in-tag"]').exists()).toBe(false)
  })

  it('clearing the depth input removes meta.depth (greeting mode)', async () => {
    const d = { id: 'm1', type: 'first_message', name: 'Note', content: 'x', meta: { depth: 3, role: 'system' } }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true } })
    await w.get('[data-test="message-depth"]').setValue('')
    const last = w.emitted('update:meta')!.at(-1)![0] as Record<string, unknown>
    expect('depth' in last).toBe(false)
  })

  it('setting depth and role emits both in meta', async () => {
    const d = { id: 'm1', type: 'first_message', name: 'Note', content: 'x', meta: {} }
    const w = mount(DefinitionEditor, { props: { definition: d, allDefinitions: [d], active: true } })
    await w.get('[data-test="message-depth"]').setValue('4')
    let last = w.emitted('update:meta')!.at(-1)![0] as Record<string, unknown>
    expect(last.depth).toBe(4)
    await w.get('[data-test="message-role"]').setValue('user')
    last = w.emitted('update:meta')!.at(-1)![0] as Record<string, unknown>
    expect(last.role).toBe('user')
  })
})
