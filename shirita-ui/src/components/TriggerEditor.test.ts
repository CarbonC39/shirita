import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import TriggerEditor from './TriggerEditor.vue'

const base = { mode: 'constant' as const, keys: [] as string[], probability: 100 }

describe('TriggerEditor', () => {
  it('switches mode and shows keyword input only for keyword mode', async () => {
    const w = mount(TriggerEditor, { props: { modelValue: base } })
    expect(w.find('[data-test="trigger-keys"]').exists()).toBe(false)
    // click the Keyword segment
    await w.findAll('[data-test="segmented"] button').find((b) => b.text() === 'Keyword')!.trigger('click')
    expect(w.emitted('update:modelValue')!.at(-1)![0]).toMatchObject({ mode: 'keyword' })
  })

  it('adds a keyword chip on Enter', async () => {
    const w = mount(TriggerEditor, { props: { modelValue: { ...base, mode: 'keyword' } } })
    const input = w.find('[data-test="trigger-keys"] input')
    await input.setValue('zion')
    await input.trigger('keydown.enter')
    expect(w.emitted('update:modelValue')!.at(-1)![0]).toMatchObject({ keys: ['zion'] })
  })
})
