import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import MessageItem from './MessageItem.vue'
import type { Message } from '../api/types'

function makeMsg(overrides: Partial<Message> = {}): Message {
  return {
    id: 'm1', session_id: 's1', parent_id: null, role: 'user',
    raw_content: 'Hello world', display_content: null, is_hidden: false,
    snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    ...overrides,
  }
}

describe('MessageItem', () => {
  it('renders user message in bubble mode', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'user' }), style: 'bubble' },
    })
    expect(wrapper.text()).toContain('Hello world')
    const row = wrapper.find('[data-test="msg-row"]')
    expect(row.classes()).toContain('justify-end')
  })

  it('renders assistant message in bubble mode', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    expect(wrapper.text()).toContain('Hello world')
    const row = wrapper.find('[data-test="msg-row"]')
    expect(row.classes()).toContain('justify-start')
  })

  it('renders assistant avatar in bubble mode', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    expect(wrapper.find('[data-test="assistant-avatar"]').exists()).toBe(true)
  })

  it('shows no avatar for user in bubble mode', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'user' }), style: 'bubble' },
    })
    expect(wrapper.find('[data-test="assistant-avatar"]').exists()).toBe(false)
  })

  it('renders in flat mode with role label', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'flat' },
    })
    expect(wrapper.text()).toContain('Assistant')
    expect(wrapper.text()).toContain('Hello world')
  })

  it('shows action buttons for assistant messages', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    const actions = wrapper.find('[data-test="message-actions"]')
    expect(actions.exists()).toBe(true)
    expect(actions.find('[data-test="copy-btn"]').exists()).toBe(true)
    expect(actions.find('[data-test="regenerate-btn"]').exists()).toBe(true)
  })

  it('does not show action buttons for user messages', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'user' }), style: 'bubble' },
    })
    expect(wrapper.find('[data-test="message-actions"]').exists()).toBe(false)
  })

  it('emits copy with raw_content', async () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    await wrapper.find('[data-test="copy-btn"]').trigger('click')
    expect(wrapper.emitted('copy')).toBeTruthy()
    expect(wrapper.emitted('copy')![0]).toEqual(['Hello world'])
  })

  it('emits regenerate on button click', async () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    await wrapper.find('[data-test="regenerate-btn"]').trigger('click')
    expect(wrapper.emitted('regenerate')).toBeTruthy()
  })

  it('shows streaming cursor when isStreaming is true', () => {
    const wrapper = mount(MessageItem, {
      props: {
        message: makeMsg({ role: 'assistant', raw_content: 'partial' }),
        style: 'bubble',
        isStreaming: true,
      },
    })
    expect(wrapper.find('[data-test="streaming-cursor"]').exists()).toBe(true)
  })

  it('shows swipe stub', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    const swipe = wrapper.find('[data-test="swipe-indicator"]')
    expect(swipe.exists()).toBe(true)
    expect(swipe.text()).toContain('1/1')
  })
})
