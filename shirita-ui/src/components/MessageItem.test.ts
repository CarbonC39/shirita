import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import MessageItem from './MessageItem.vue'
import type { Message } from '../api/types'

beforeEach(() => {
  setActivePinia(createPinia())
})

function makeMsg(overrides: Partial<Message> = {}): Message {
  return {
    id: 'm1', session_id: 's1', parent_id: null, role: 'user',
    raw_content: 'Hello world', display_content: null, is_hidden: false, is_anchor: false,
    attachments: [], snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
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

  it('renders display_content when present (hiding control tags)', () => {
    const wrapper = mount(MessageItem, {
      props: {
        message: makeMsg({ role: 'assistant', raw_content: 'Hit. <state_update action="SUB" key="hp" value="5"/>', display_content: 'Hit.' }),
        style: 'bubble',
      },
    })
    expect(wrapper.text()).toContain('Hit.')
    expect(wrapper.text()).not.toContain('state_update')
  })

  it('renders display_content in flat mode too (hiding control tags)', () => {
    const wrapper = mount(MessageItem, {
      props: {
        message: makeMsg({ role: 'assistant', raw_content: 'Hit. <state_update action="SUB" key="hp" value="5"/>', display_content: 'Hit.' }),
        style: 'flat',
      },
    })
    expect(wrapper.text()).toContain('Hit.')
    expect(wrapper.text()).not.toContain('state_update')
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

  it('shows copy/edit/hide for user messages but no regenerate or swipe', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'user' }), style: 'bubble' },
    })
    const actions = wrapper.find('[data-test="message-actions"]')
    expect(actions.exists()).toBe(true)
    expect(actions.find('[data-test="copy-btn"]').exists()).toBe(true)
    expect(actions.find('[data-test="edit-btn"]').exists()).toBe(true)
    expect(actions.find('[data-test="hide-btn"]').exists()).toBe(true)
    expect(actions.find('[data-test="regenerate-btn"]').exists()).toBe(false)
    expect(actions.find('[data-test="swipe-indicator"]').exists()).toBe(false)
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

  it('hides the swipe indicator when there is only one sibling', () => {
    const wrapper = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble', siblingCount: 1 },
    })
    expect(wrapper.find('[data-test="swipe-indicator"]').exists()).toBe(false)
  })

  it('shows the real swipe count and emits swipe on the arrows', async () => {
    const msg = makeMsg({ id: 'b2', parent_id: 'a', role: 'assistant', raw_content: 'hi', created_at: '2' })
    const w = mount(MessageItem, { props: { message: msg, style: 'bubble', siblingIndex: 1, siblingCount: 2 } })
    expect(w.find('[data-test="swipe-indicator"]').text()).toContain('2/2')
    await w.find('[data-test="swipe-prev"]').trigger('click')
    expect(w.emitted('swipe')![0]).toEqual([-1])
  })

  it('shows the token estimate only when the tokens prop is set', () => {
    const withTokens = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble', tokens: 1234 },
    })
    expect(withTokens.find('[data-test="convo-tokens"]').text()).toContain('1,234')

    const withoutTokens = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble' },
    })
    expect(withoutTokens.find('[data-test="convo-tokens"]').exists()).toBe(false)
  })

  it('edits in place and emits edit-save', async () => {
    const msg = makeMsg({ id: 'u', parent_id: null, role: 'user', raw_content: 'hello', created_at: '1' })
    const w = mount(MessageItem, { props: { message: msg, style: 'flat' } })
    await w.find('[data-test="edit-btn"]').trigger('click')
    const ta = w.find('[data-test="edit-area"]')
    await ta.setValue('hello edited')
    await w.find('[data-test="edit-save"]').trigger('click')
    expect(w.emitted('edit-save')![0]).toEqual(['hello edited'])
  })
})

describe('MessageItem identity', () => {
  const identity = { assistant: { name: 'Neo', avatar: 'a.png' }, user: { name: 'Me', avatar: 'u.png' } }

  it('renders the assistant identity avatar and name in bubble mode', () => {
    const w = mount(MessageItem, {
      props: { message: makeMsg({ role: 'assistant' }), style: 'bubble', identity },
    })
    const img = w.find('[data-test="assistant-avatar"] img')
    expect(img.exists()).toBe(true)
    expect(img.attributes('src')).toContain('a.png')
  })

  it('uses the identity names in flat mode', () => {
    const a = mount(MessageItem, { props: { message: makeMsg({ role: 'assistant' }), style: 'flat', identity } })
    expect(a.text()).toContain('Neo')
    const u = mount(MessageItem, { props: { message: makeMsg({ role: 'user' }), style: 'flat', identity } })
    expect(u.text()).toContain('Me')
  })
})
