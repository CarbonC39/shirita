import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import Composer from './Composer.vue'

describe('Composer', () => {
  it('renders a text input and a send button', () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    expect(wrapper.find('textarea').exists()).toBe(true)
    expect(wrapper.find('[data-test="send-btn"]').exists()).toBe(true)
  })

  it('emits send with trimmed text on button click', async () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const textarea = wrapper.find('textarea')
    await textarea.setValue('  hello world  ')
    await wrapper.find('[data-test="send-btn"]').trigger('click')
    expect(wrapper.emitted('send')).toBeTruthy()
    expect(wrapper.emitted('send')![0]).toEqual(['hello world'])
    expect((textarea.element as HTMLTextAreaElement).value).toBe('')
  })

  it('emits send on Enter (without Shift)', async () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const textarea = wrapper.find('textarea')
    await textarea.setValue('hi')
    await textarea.trigger('keydown', { key: 'Enter', shiftKey: false })
    expect(wrapper.emitted('send')).toBeTruthy()
  })

  it('does not send on Shift+Enter', async () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const textarea = wrapper.find('textarea')
    await textarea.setValue('hi')
    await textarea.trigger('keydown', { key: 'Enter', shiftKey: true })
    expect(wrapper.emitted('send')).toBeFalsy()
  })

  it('does not send empty text', async () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const textarea = wrapper.find('textarea')
    await textarea.setValue('   ')
    await wrapper.find('[data-test="send-btn"]').trigger('click')
    expect(wrapper.emitted('send')).toBeFalsy()
  })

  it('disables input and send button when disabled', () => {
    const wrapper = mount(Composer, { props: { disabled: true } })
    expect((wrapper.find('textarea').element as HTMLTextAreaElement).disabled).toBe(true)
    expect((wrapper.find('[data-test="send-btn"]').element as HTMLButtonElement).disabled).toBe(true)
  })

  it('shows muted styling on send button when text is empty', () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const btn = wrapper.find('[data-test="send-btn"]')
    expect(btn.classes()).toContain('text-muted')
    expect(btn.classes()).not.toContain('text-primary')
  })
})
