import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import MessageList from './MessageList.vue'
import type { Message } from '../api/types'

beforeEach(() => {
  setActivePinia(createPinia())
})

function makeMsg(overrides: Partial<Message> = {}): Message {
  return {
    id: 'm1', session_id: 's1', parent_id: null, role: 'user',
    raw_content: 'Hello', display_content: null, is_hidden: false, is_anchor: false,
    attachments: [], snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    ...overrides,
  }
}

describe('MessageList', () => {
  it('renders a MessageItem per message', () => {
    const msgs = [makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' }), makeMsg({ id: 'm2', role: 'assistant', raw_content: 'hello' })]
    const wrapper = mount(MessageList, { props: { messages: msgs, style: 'bubble' } })
    expect(wrapper.findAll('[data-test="msg-row"]')).toHaveLength(2)
  })

  it('shows empty state when no messages', () => {
    const wrapper = mount(MessageList, { props: { messages: [], style: 'bubble' } })
    expect(wrapper.text()).toContain('No messages yet.')
  })

  it('does not render anchor messages', () => {
    const msgs = [
      makeMsg({ id: 'a', role: 'user', raw_content: '<start>', is_anchor: true }),
      makeMsg({ id: 'b', role: 'assistant', raw_content: 'wake up', is_anchor: false }),
    ]
    const wrapper = mount(MessageList, { props: { messages: msgs, style: 'bubble' } })
    expect(wrapper.findAll('[data-test="msg-row"]')).toHaveLength(1)
    expect(wrapper.text()).not.toContain('<start>')
    expect(wrapper.text()).toContain('wake up')
  })

  it('renders streaming ghost when streaming', () => {
    const wrapper = mount(MessageList, {
      props: { messages: [makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' })], style: 'bubble', isStreaming: true, streamingText: 'partial reply...' },
    })
    expect(wrapper.findAll('[data-test="msg-row"]')).toHaveLength(2)
    expect(wrapper.text()).toContain('partial reply...')
    expect(wrapper.find('[data-test="streaming-cursor"]').exists()).toBe(true)
  })

  it('shows streaming error inline', () => {
    const wrapper = mount(MessageList, {
      props: { messages: [], style: 'bubble', streamingError: 'session not found' },
    })
    expect(wrapper.text()).toContain('session not found')
  })

  it('passes style prop to MessageItem', () => {
    const wrapper = mount(MessageList, {
      props: { messages: [makeMsg({ role: 'assistant' })], style: 'flat' },
    })
    expect(wrapper.text()).toContain('Assistant')
  })

  it('emits copy from MessageItem', async () => {
    const wrapper = mount(MessageList, {
      props: { messages: [makeMsg({ id: 'm1', role: 'assistant', raw_content: 'test' })], style: 'bubble' },
    })
    await wrapper.find('[data-test="copy-btn"]').trigger('click')
    expect(wrapper.emitted('copy')).toBeTruthy()
    expect(wrapper.emitted('copy')![0]).toEqual(['test'])
  })

  it('emits regenerate from MessageItem', async () => {
    const wrapper = mount(MessageList, {
      props: { messages: [makeMsg({ role: 'assistant' })], style: 'bubble' },
    })
    await wrapper.find('[data-test="regenerate-btn"]').trigger('click')
    expect(wrapper.emitted('regenerate')).toBeTruthy()
  })

  it('forwards identity to MessageItem, including the streaming ghost', () => {
    const identity = { assistant: { name: 'Neo', avatar: 'a.png' }, user: { name: 'Me', avatar: 'u.png' } }
    const wrapper = mount(MessageList, {
      props: {
        messages: [makeMsg({ role: 'assistant' })],
        style: 'flat',
        identity,
        isStreaming: true,
        streamingText: 'partial',
      },
    })
    expect(wrapper.text()).toContain('Neo')
    expect(wrapper.text()).not.toContain('Assistant')
  })
})
