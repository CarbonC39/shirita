import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import Composer from './Composer.vue'
import * as client from '../api/client'

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
    expect(wrapper.emitted('send')![0]).toEqual(['hello world', []])
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

  it('uploads a picked file and emits its asset id with the send', async () => {
    vi.spyOn(client, 'uploadAsset').mockResolvedValue({ id: 'a1', name: 'pic', path: 'pic.png', url: '/assets/pic.png' })
    const wrapper = mount(Composer, { props: { disabled: false } })
    const fileInput = wrapper.find('input[type="file"]')
    const file = new File(['bytes'], 'pic.png', { type: 'image/png' })
    Object.defineProperty(fileInput.element, 'files', { value: [file] })
    await fileInput.trigger('change')
    await wrapper.vm.$nextTick()
    await wrapper.find('[data-test="send-btn"]').trigger('click')
    expect(wrapper.emitted('send')![0]).toEqual(['', ['a1']])
  })

  it('enables send with only an attachment and no text', async () => {
    vi.spyOn(client, 'uploadAsset').mockResolvedValue({ id: 'a1', name: 'pic', path: 'pic.png', url: '/assets/pic.png' })
    const wrapper = mount(Composer, { props: { disabled: false } })
    expect((wrapper.find('[data-test="send-btn"]').element as HTMLButtonElement).disabled).toBe(true)
    const fileInput = wrapper.find('input[type="file"]')
    const file = new File(['bytes'], 'pic.png', { type: 'image/png' })
    Object.defineProperty(fileInput.element, 'files', { value: [file] })
    await fileInput.trigger('change')
    await wrapper.vm.$nextTick()
    expect((wrapper.find('[data-test="send-btn"]').element as HTMLButtonElement).disabled).toBe(false)
  })
})
