import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'
import MessageList from './MessageList.vue'
import type { Message } from '../api/types'

// jsdom has no layout engine and no `scrollTo` implementation, so the scroll
// metrics are defined per-instance after mount and `scrollTo` is mocked on the
// prototype so every mount (including the initial bottom scroll) records its
// call instead of throwing.
beforeEach(() => {
  setActivePinia(createPinia())
  ;(HTMLElement.prototype as unknown as { scrollTo: ReturnType<typeof vi.fn> }).scrollTo = vi.fn()
})

const SCROLL = 48 // must match the threshold in MessageList.vue

function setMetrics(el: HTMLElement, scrollHeight: number, clientHeight: number) {
  Object.defineProperty(el, 'scrollHeight', { configurable: true, value: scrollHeight })
  Object.defineProperty(el, 'clientHeight', { configurable: true, value: clientHeight })
}

function scrollTo(el: HTMLElement, scrollTop: number) {
  el.scrollTop = scrollTop
  el.dispatchEvent(new Event('scroll'))
}

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

  it('shows the token count only on the last visible message', () => {
    const msgs = [makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' }), makeMsg({ id: 'm2', role: 'assistant', raw_content: 'hello' })]
    const wrapper = mount(MessageList, { props: { messages: msgs, style: 'bubble', tokens: 42 } })
    const rows = wrapper.findAll('[data-test="msg-row"]')
    expect(rows[0].find('[data-test="convo-tokens"]').exists()).toBe(false)
    expect(rows[1].find('[data-test="convo-tokens"]').text()).toContain('42')
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

  describe('scroll anchoring', () => {
    const scrollSel = '[data-test="message-scroll"]'

    function scroller(wrapper: ReturnType<typeof mount>) {
      const el = wrapper.find(scrollSel).element as HTMLElement
      setMetrics(el, 1000, 500)
      return el
    }

    it('scrolls to the bottom on initial mount of a populated transcript', async () => {
      const wrapper = mount(MessageList, {
        props: { messages: [makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' })], style: 'bubble' },
      })
      const el = scroller(wrapper)
      await flushPromises()
      expect(HTMLElement.prototype.scrollTo).toHaveBeenCalled()
      // Following the bottom means the target equals the full scroll height.
      expect((HTMLElement.prototype.scrollTo as ReturnType<typeof vi.fn>).mock.calls[0][0]).toMatchObject({ top: 1000 })
      expect(el).toBeDefined()
    })

    it('keeps following the bottom while streaming text grows', async () => {
      const wrapper = mount(MessageList, {
        props: { messages: [makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' })], style: 'bubble' },
      })
      const el = scroller(wrapper)
      await flushPromises()
      ;(HTMLElement.prototype.scrollTo as ReturnType<typeof vi.fn>).mockClear()
      // User is near the bottom (within threshold), so following stays on.
      scrollTo(el, 500) // distance 1000-500-500 = 0 <= 48
      await wrapper.setProps({ isStreaming: true, streamingText: 'partial...' })
      await flushPromises()
      expect(HTMLElement.prototype.scrollTo).toHaveBeenCalled()
    })

    it('does not scroll on streaming growth after the user scrolls upward', async () => {
      const wrapper = mount(MessageList, {
        props: { messages: [makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' })], style: 'bubble' },
      })
      const el = scroller(wrapper)
      await flushPromises()
      ;(HTMLElement.prototype.scrollTo as ReturnType<typeof vi.fn>).mockClear()
      // User scrolls far above the bottom (distance > threshold) → stop following.
      scrollTo(el, 100) // distance 1000-500-100 = 400 > 48
      await wrapper.setProps({ isStreaming: true, streamingText: 'growing...' })
      await flushPromises()
      expect(HTMLElement.prototype.scrollTo).not.toHaveBeenCalled()
    })

    it('scrolls when a streaming ghost is replaced by a persisted message while following', async () => {
      const wrapper = mount(MessageList, {
        props: {
          messages: [makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' })],
          style: 'bubble',
          isStreaming: true,
          streamingText: 'partial',
        },
      })
      const el = scroller(wrapper)
      await flushPromises()
      ;(HTMLElement.prototype.scrollTo as ReturnType<typeof vi.fn>).mockClear()
      scrollTo(el, 500) // following
      await wrapper.setProps({
        isStreaming: false,
        streamingText: '',
        messages: [
          makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' }),
          makeMsg({ id: 'a1', role: 'assistant', raw_content: 'persisted', parent_id: 'm1' }),
        ],
      })
      await flushPromises()
      expect(HTMLElement.prototype.scrollTo).toHaveBeenCalled()
    })

    it('does not scroll when the same replacement happens while the user is reading above', async () => {
      const wrapper = mount(MessageList, {
        props: {
          messages: [makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' })],
          style: 'bubble',
          isStreaming: true,
          streamingText: 'partial',
        },
      })
      const el = scroller(wrapper)
      await flushPromises()
      ;(HTMLElement.prototype.scrollTo as ReturnType<typeof vi.fn>).mockClear()
      scrollTo(el, 100) // distance > threshold → not following
      await wrapper.setProps({
        isStreaming: false,
        streamingText: '',
        messages: [
          makeMsg({ id: 'm1', role: 'user', raw_content: 'hi' }),
          makeMsg({ id: 'a1', role: 'assistant', raw_content: 'persisted', parent_id: 'm1' }),
        ],
      })
      await flushPromises()
      expect(HTMLElement.prototype.scrollTo).not.toHaveBeenCalled()
    })
  })
})
