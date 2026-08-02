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
    vi.spyOn(client, 'uploadAsset').mockResolvedValue({ id: 'a1', name: 'pic', path: 'pic.png', url: '/assets/pic.png', kind: 'background' })
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
    vi.spyOn(client, 'uploadAsset').mockResolvedValue({ id: 'a1', name: 'pic', path: 'pic.png', url: '/assets/pic.png', kind: 'background' })
    const wrapper = mount(Composer, { props: { disabled: false } })
    expect((wrapper.find('[data-test="send-btn"]').element as HTMLButtonElement).disabled).toBe(true)
    const fileInput = wrapper.find('input[type="file"]')
    const file = new File(['bytes'], 'pic.png', { type: 'image/png' })
    Object.defineProperty(fileInput.element, 'files', { value: [file] })
    await fileInput.trigger('change')
    await wrapper.vm.$nextTick()
    expect((wrapper.find('[data-test="send-btn"]').element as HTMLButtonElement).disabled).toBe(false)
  })

  it('reserves no token/status row for an empty draft', () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    expect(wrapper.find('[data-test="draft-tokens"]').exists()).toBe(false)
  })

  it('shows draft-token info for non-empty text without replacing the send control', async () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    await wrapper.find('textarea').setValue('hello world')
    await wrapper.vm.$nextTick()
    expect(wrapper.find('[data-test="draft-tokens"]').exists()).toBe(true)
    expect(wrapper.find('[data-test="send-btn"]').exists()).toBe(true)
    expect(wrapper.find('textarea').exists()).toBe(true)
  })

  it('resets the textarea height to its minimum after send', async () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const ta = wrapper.find('textarea').element as HTMLTextAreaElement
    ta.style.height = '120px'
    await wrapper.find('textarea').setValue('a message')
    await wrapper.find('[data-test="send-btn"]').trigger('click')
    await wrapper.vm.$nextTick()
    // Empty after send: autosize clamps back to the natural row height.
    expect((wrapper.find('textarea').element as HTMLTextAreaElement).value).toBe('')
    expect(Number.parseFloat((wrapper.find('textarea').element as HTMLTextAreaElement).style.height)).toBeLessThan(120)
  })

  it('keeps the textarea as the flexible element in the main control row', () => {
    const wrapper = mount(Composer, { props: { disabled: false } })
    const ta = wrapper.find('textarea')
    expect(ta.attributes('class')).toContain('flex-1')
    // A stable hook exists for styling the textarea without utility strings.
    expect(ta.classes()).toContain('app-composer-textarea')
  })

  it('keeps Stop in the same control slot as Send without changing Composer width', () => {
    const send = mount(Composer, { props: { disabled: false } })
    const stop = mount(Composer, { props: { disabled: false, streaming: true } })
    expect(send.find('[data-test="send-btn"]').exists()).toBe(true)
    expect(stop.find('[data-test="stop-btn"]').exists()).toBe(true)
    expect(stop.find('[data-test="send-btn"]').exists()).toBe(false)
    // Both occupy the same flex row; neither forces a width change.
    expect(send.find('textarea').classes()).toContain('flex-1')
    expect(stop.find('textarea').classes()).toContain('flex-1')
  })
})
