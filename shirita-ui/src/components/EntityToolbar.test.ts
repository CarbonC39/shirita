import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import EntityToolbar from './EntityToolbar.vue'
import EntityPicker from './EntityPicker.vue'

const props = (overrides: Record<string, any> = {}) => ({
  items: [] as { id: string; name: string }[],
  placeholder: 'Pick…',
  createLabel: 'New Template',
  selectedLabel: '',
  selectedId: null as string | null,
  entityName: '',
  ...overrides,
})

/** Drive the picker dropdown open and click the "+ New X" affordance, which is
 *  what emits `intent-create` and hands creation over to the toolbar. */
async function startCreate(w: ReturnType<typeof mount>) {
  // First button in the toolbar is the EntityPicker toggle.
  await w.find('button').trigger('click')
  const newBtn = w.findAll('button').find((b) => b.text().includes('New Template'))
  expect(newBtn).toBeTruthy()
  await newBtn!.trigger('click')
}

describe('EntityToolbar — create flow (real DOM)', () => {
  beforeEach(() => localStorage.clear())

  it('on intent-create: hides the picker search box and reveals the name input', async () => {
    const w = mount(EntityToolbar, { props: props() })
    expect(w.findComponent(EntityPicker).exists()).toBe(true)
    expect(w.find('[data-test="create-name-input"]').exists()).toBe(false)
    await startCreate(w)
    // The picker (search box) is unmounted in creation mode…
    expect(w.findComponent(EntityPicker).exists()).toBe(false)
    // …and the name text box appears.
    expect(w.find('[data-test="create-name-input"]').exists()).toBe(true)
  })

  it('typing then Enter emits create with the trimmed name and exits creation', async () => {
    const w = mount(EntityToolbar, { props: props() })
    await startCreate(w)
    const input = w.find('[data-test="create-name-input"]')
    await input.setValue('  Villain  ')
    await input.trigger('keydown', { key: 'Enter' })
    expect(w.emitted('create')).toEqual([['Villain']])
    // creation mode closed after confirming
    expect(w.find('[data-test="create-name-input"]').exists()).toBe(false)
    expect(w.findComponent(EntityPicker).exists()).toBe(true)
  })

  it('clicking the ✔ confirm button emits create too', async () => {
    const w = mount(EntityToolbar, { props: props() })
    await startCreate(w)
    await w.find('[data-test="create-name-input"]').setValue('Hero')
    await w.find('[data-test="create-confirm"]').trigger('click')
    expect(w.emitted('create')).toEqual([['Hero']])
  })

  it('does not emit create for a blank name', async () => {
    const w = mount(EntityToolbar, { props: props() })
    await startCreate(w)
    await w.find('[data-test="create-name-input"]').setValue('   ')
    await w.find('[data-test="create-confirm"]').trigger('click')
    expect(w.emitted('create')).toBeUndefined()
    // still in creation mode
    expect(w.find('[data-test="create-name-input"]').exists()).toBe(true)
  })

  it('Esc cancels creation without emitting', async () => {
    const w = mount(EntityToolbar, { props: props() })
    await startCreate(w)
    await w.find('[data-test="create-name-input"]').setValue('Ghost')
    await w.find('[data-test="create-name-input"]').trigger('keydown', { key: 'Escape' })
    expect(w.emitted('create')).toBeUndefined()
    expect(w.findComponent(EntityPicker).exists()).toBe(true)
  })

  it('the X button cancels creation without emitting', async () => {
    const w = mount(EntityToolbar, { props: props() })
    await startCreate(w)
    await w.find('[data-test="create-cancel"]').trigger('click')
    expect(w.emitted('create')).toBeUndefined()
    expect(w.findComponent(EntityPicker).exists()).toBe(true)
  })
})

describe('EntityToolbar — rename + action buttons', () => {
  beforeEach(() => localStorage.clear())

  it('enters rename mode from the pencil and emits rename on Enter', async () => {
    const w = mount(EntityToolbar, { props: props({ selectedId: 't1', entityName: 'Old' }) })
    // pencil is the first action button; enabled because selectedId is set
    await w.find('[title="Rename"]').trigger('click')
    const input = w.find('[data-test="rename-input"]')
    expect(input.exists()).toBe(true)
    await input.setValue('New')
    await input.trigger('keydown', { key: 'Enter' })
    expect(w.emitted('rename')).toEqual([['New']])
  })

  it('rename ignores a blank name', async () => {
    const w = mount(EntityToolbar, { props: props({ selectedId: 't1', entityName: 'Old' }) })
    await w.find('[title="Rename"]').trigger('click')
    await w.find('[data-test="rename-input"]').setValue('   ')
    await w.find('[data-test="rename-input"]').trigger('keydown', { key: 'Enter' })
    expect(w.emitted('rename')).toBeUndefined()
  })

  it('forwards import / export / duplicate / delete / toggle-default button clicks', async () => {
    const w = mount(EntityToolbar, {
      props: props({ selectedId: 't1', showDefault: true }),
    })
    await w.find('[title="Import"]').trigger('click')
    await w.find('[title="Export"]').trigger('click')
    await w.find('[title="Duplicate"]').trigger('click')
    await w.find('[title="Delete"]').trigger('click')
    await w.find('[title="Default template"]').trigger('click')
    expect(w.emitted('import')).toHaveLength(1)
    expect(w.emitted('export')).toHaveLength(1)
    expect(w.emitted('duplicate')).toHaveLength(1)
    expect(w.emitted('delete')).toHaveLength(1)
    expect(w.emitted('toggle-default')).toHaveLength(1)
  })

  it('disables the selection-bound action buttons when nothing is selected', () => {
    const w = mount(EntityToolbar, { props: props({ selectedId: null, showDefault: true }) })
    expect(w.find('[title="Rename"]').attributes('disabled')).toBeDefined()
    expect(w.find('[title="Export"]').attributes('disabled')).toBeDefined()
    expect(w.find('[title="Delete"]').attributes('disabled')).toBeDefined()
    // import is always available
    expect(w.find('[title="Import"]').attributes('disabled')).toBeUndefined()
  })
})
