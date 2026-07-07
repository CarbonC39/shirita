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
    // Open the dropdown via the standalone toggle button
    await w.find('button').trigger('click')
    // Items are rendered as buttons in the dropdown
    const itemBtns = w.findAll('button').filter(b => b.text() === 'RP preset' || b.text() === 'Assistant')
    expect(itemBtns.length).toBe(2)
    await itemBtns[0].trigger('click')
    expect(w.emitted('select')![0]).toEqual(['t1'])
  })

  it('filters by query (case-insensitive)', async () => {
    const w = mount(EntityPicker, { props: { items, placeholder: 'pick…', createLabel: 'New' } })
    await w.find('button').trigger('click')
    await w.find('input').setValue('assist')
    const itemBtns = w.findAll('button').filter(b => b.text() === 'Assistant')
    expect(itemBtns.length).toBe(1)
    expect(itemBtns[0].text()).toContain('Assistant')
  })

  it('enters create mode and emits the entered name', async () => {
    const w = mount(EntityPicker, { props: { items: [], placeholder: 'pick…', createLabel: 'New Template' } })
    await w.find('button').trigger('click')
    // With empty items, the "create" option appears
    const createBtn = w.findAll('button').find(b => b.text().includes('New Template'))
    expect(createBtn).toBeTruthy()
    await createBtn!.trigger('click')
    // Now in create mode: name input + X + Check icons
    const input = w.find('input')
    expect(input.exists()).toBe(true)
    await input.setValue('Villain')
    // Click the Check (Confirm) button
    const confirmBtn = w.findAll('button').find(b => b.attributes('title') === 'Confirm')
    expect(confirmBtn).toBeTruthy()
    await confirmBtn!.trigger('click')
    expect(w.emitted('create')![0]).toEqual(['Villain'])
  })

  it('shows selectedLabel in the toggle button', () => {
    const w = mount(EntityPicker, { props: { items, selectedLabel: 'My Template' } })
    expect(w.find('button').text()).toBe('My Template')
  })
})
