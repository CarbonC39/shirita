import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import VariablesEditor from './VariablesEditor.vue'
import type { VarDecl } from '../api/types'

describe('VariablesEditor', () => {
  it('emits update when a row is added', async () => {
    const w = mount(VariablesEditor, { props: { modelValue: [] as VarDecl[] } })
    await w.find('[data-test="add-var"]').trigger('click')
    const ev = w.emitted('update:modelValue')!.at(-1)![0] as VarDecl[]
    expect(ev).toHaveLength(1)
    expect(ev[0].type).toBe('number')
  })

  it('emits update when a row is removed', async () => {
    const w = mount(VariablesEditor, {
      props: { modelValue: [{ name: 'hp', type: 'number', initial: 100 }] as VarDecl[] },
    })
    await w.find('[data-test="remove-var"]').trigger('click')
    const ev = w.emitted('update:modelValue')!.at(-1)![0] as VarDecl[]
    expect(ev).toHaveLength(0)
  })
})
