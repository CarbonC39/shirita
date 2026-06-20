import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import EntityPicker from './EntityPicker.vue'

const items = [
  { id: 't1', name: 'RP preset' },
  { id: 't2', name: 'Assistant' },
]

describe('EntityPicker', () => {
  it('lists items and emits select on click', async () => {
    const w = mount(EntityPicker, { props: { items, placeholder: 'pick…', createLabel: 'New' } })
    await w.find('[data-test="entity-search"]').trigger('focus')
    const rows = w.findAll('[data-test="entity-item"]')
    expect(rows.length).toBe(2)
    await rows[0].trigger('mousedown')
    expect(w.emitted('select')![0]).toEqual(['t1'])
  })

  it('filters by query (case-insensitive)', async () => {
    const w = mount(EntityPicker, { props: { items, placeholder: 'pick…', createLabel: 'New' } })
    await w.find('[data-test="entity-search"]').trigger('focus')
    await w.find('[data-test="entity-search"]').setValue('assist')
    const rows = w.findAll('[data-test="entity-item"]')
    expect(rows.length).toBe(1)
    expect(rows[0].text()).toContain('Assistant')
  })

  it('emits create with the trimmed query', async () => {
    const w = mount(EntityPicker, { props: { items, placeholder: 'pick…', createLabel: 'New' } })
    await w.find('[data-test="entity-search"]').trigger('focus')
    await w.find('[data-test="entity-search"]').setValue('  Villain ')
    await w.find('[data-test="entity-create"]').trigger('mousedown')
    expect(w.emitted('create')![0]).toEqual(['Villain'])
  })
})
