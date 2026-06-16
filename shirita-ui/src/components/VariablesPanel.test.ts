import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import VariablesPanel from './VariablesPanel.vue'
import type { VarDecl } from '../api/types'

const schema: VarDecl[] = [
  { name: '$avatar', type: 'string', initial: '', scope: 'system' },
  { name: 'hp', type: 'number', initial: 100, scope: 'template' },
  { name: 'alarmed', type: 'bool', initial: false, scope: 'template' },
]

describe('VariablesPanel', () => {
  it('renders nothing when the schema is empty', () => {
    const w = mount(VariablesPanel, { props: { schema: [], values: {} } })
    expect(w.find('[data-test="variables-panel"]').exists()).toBe(false)
  })

  it('reveals System and Custom groups on toggle with formatted values', async () => {
    const w = mount(VariablesPanel, { props: { schema, values: { '$avatar': '', hp: 95, alarmed: true } } })
    expect(w.find('[data-test="variables-panel"]').exists()).toBe(true)
    // collapsed by default
    expect(w.find('[data-test="var-system"]').exists()).toBe(false)
    await w.find('[data-test="variables-toggle"]').trigger('click')
    expect(w.find('[data-test="var-system"]').text()).toContain('$avatar')
    const custom = w.find('[data-test="var-custom"]')
    expect(custom.text()).toContain('hp')
    expect(custom.text()).toContain('95')
    expect(custom.text()).toContain('✓') // alarmed=true
  })
})
