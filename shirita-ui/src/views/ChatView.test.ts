import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import { setActivePinia, createPinia } from 'pinia'
import * as client from '../api/client'
import { useChatStore } from '../stores/chat'
import ChatView from './ChatView.vue'

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/chat/:id', component: ChatView },
      { path: '/', component: { template: '<div />' } },
    ],
  })
}

describe('ChatView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.restoreAllMocks()
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's1', active_leaf_id: null } as never)
    vi.spyOn(client, 'getSessionState').mockResolvedValue({
      schema: [{ name: 'hp', type: 'number', initial: 100, scope: 'template' }],
      values: { hp: 100 },
    } as never)
  })
  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('loads messages on mount', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([{
      id: 'm1', session_id: 's1', parent_id: null, role: 'user',
      raw_content: 'hi', display_content: null, is_hidden: false, is_anchor: false, attachments: [],
      snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    }])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(client.listMessages).toHaveBeenCalledWith('s1')
  })

  it('renders loaded messages', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([{
      id: 'm1', session_id: 's1', parent_id: null, role: 'user',
      raw_content: 'hello', display_content: null, is_hidden: false, is_anchor: false, attachments: [],
      snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    }])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(wrapper.text()).toContain('hello')
  })

  it('shows loading state', async () => {
    vi.spyOn(client, 'listMessages').mockReturnValue(new Promise(() => {}))
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(wrapper.text()).toContain('Loading')
  })

  it('shows initial load error with a working retry action', async () => {
    vi.spyOn(client, 'listMessages')
      .mockRejectedValueOnce(new Error('Not found'))
      .mockResolvedValueOnce([{
        id: 'm1', session_id: 's1', parent_id: null, role: 'user',
        raw_content: 'hello', display_content: null, is_hidden: false, is_anchor: false, attachments: [],
        snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
      }])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    // Initial failure has no cached transcript, so show the full error + retry.
    expect(wrapper.find('[data-test="load-error"]').text()).toContain('Not found')
    await wrapper.find('[data-test="retry-load"]').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('hello')
    expect(wrapper.find('[data-test="load-error"]').exists()).toBe(false)
  })

  it('keeps a cached transcript visible when a refresh fails', async () => {
    const initial = [{
      id: 'm1', session_id: 's1', parent_id: null, role: 'user' as const,
      raw_content: 'hello', display_content: null, is_hidden: false, is_anchor: false, attachments: [],
      snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    }]
    vi.spyOn(client, 'listMessages').mockResolvedValue(initial)
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    // A background refresh fails after the transcript loaded.
    vi.spyOn(client, 'listMessages').mockRejectedValueOnce(new Error('boom'))
    await wrapper.vm.$nextTick()
    await useChatStore().loadMessages('s1')
    await flushPromises()
    // Refresh failure is non-destructive: the transcript stays on screen.
    expect(wrapper.find('[data-test="refresh-error"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('boom')
    expect(wrapper.text()).toContain('hello')
  })

  it('dismisses a generation error without starting a new turn', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    async function* stream(): AsyncGenerator<client.SseEvent> {
      yield { type: 'error', message: 'provider exploded' }
    }
    vi.spyOn(client, 'sendMessage').mockReturnValue(stream())
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    const textarea = wrapper.find('textarea')
    await textarea.setValue('hello')
    await wrapper.find('[data-test="send-btn"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-test="streaming-error"]').text()).toContain('provider exploded')
    await wrapper.find('[data-test="dismiss-streaming-error"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-test="streaming-error"]').exists()).toBe(false)
  })

  it('calls send on composer submit', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([{
      id: 'm1', session_id: 's1', parent_id: null, role: 'user',
      raw_content: 'hi', display_content: null, is_hidden: false, is_anchor: false, attachments: [],
      snapshot_state: {}, created_at: '',
    }])
    async function* stream(): AsyncGenerator<client.SseEvent> {
      yield { type: 'delta', text: 'ok' }
      yield { type: 'done', message_id: 'a1' }
    }
    const sendSpy = vi.spyOn(client, 'sendMessage').mockReturnValue(stream())

    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    const textarea = wrapper.find('textarea')
    await textarea.setValue('hello')
    await wrapper.find('[data-test="send-btn"]').trigger('click')
    await flushPromises()
    expect(sendSpy).toHaveBeenCalledWith('s1', 'hello', [], expect.any(AbortSignal))
  })

  it('renders the three-region workspace markers', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(wrapper.find('[data-test="chat-bar"]').exists()).toBe(true)
    expect(wrapper.find('[data-test="transcript-region"]').exists()).toBe(true)
    expect(wrapper.find('[data-test="composer-region"]').exists()).toBe(true)
  })

  it('keeps MessageList as the only transcript scroller (workspace region does not scroll)', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([{
      id: 'm1', session_id: 's1', parent_id: null, role: 'user' as const,
      raw_content: 'hello', display_content: null, is_hidden: false, is_anchor: false, attachments: [],
      snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    }])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    const transcript = wrapper.find('[data-test="transcript-region"]').element as HTMLElement
    // The workspace region must not itself be the vertical scroll owner.
    expect(transcript.classList.contains('overflow-y-auto')).toBe(false)
    // Exactly one transcript scroller is mounted (MessageList's).
    expect(wrapper.findAll('[data-test="message-scroll"]')).toHaveLength(1)
  })

  it('shows a details trigger when session state declares variables', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(wrapper.find('[data-test="details-trigger"]').exists()).toBe(true)
  })

  it('opens details showing panels and grouped variables', async () => {
    vi.spyOn(client, 'getSessionPanels').mockResolvedValue([
      { id: 'F', name: 'Status', html: '<b>hi</b>', css: '', caps: {}, min_messages: 0 },
    ])
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    await wrapper.find('[data-test="details-trigger"]').trigger('click')
    await flushPromises()
    const dialog = document.querySelector('[data-test="details-dialog"]')
    expect(dialog).not.toBeNull()
    expect(dialog?.textContent).toContain('Status')
    expect(dialog?.textContent).toContain('hp')
  })

  it('closes details via the close button and returns focus to the trigger', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] }, attachTo: document.body })
    await flushPromises()
    const trigger = wrapper.find('[data-test="details-trigger"]')
    await trigger.trigger('click')
    await flushPromises()
    const closeBtn = document.querySelector('[data-test="details-close"]') as HTMLButtonElement | null
    expect(closeBtn).not.toBeNull()
    closeBtn?.click()
    await flushPromises()
    expect(document.querySelector('[data-test="details-dialog"]')).toBeNull()
    expect((trigger.element as HTMLElement) === document.activeElement).toBe(true)
    wrapper.unmount()
  })

  it('closes details on Escape', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    await wrapper.find('[data-test="details-trigger"]').trigger('click')
    await flushPromises()
    const dialog = document.querySelector('[data-test="details-dialog"]') as HTMLElement | null
    dialog?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(document.querySelector('[data-test="details-dialog"]')).toBeNull()
  })

  it('opening and closing details keeps the draft and message list intact', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([{
      id: 'm1', session_id: 's1', parent_id: null, role: 'user' as const,
      raw_content: 'hello', display_content: null, is_hidden: false, is_anchor: false, attachments: [],
      snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    }])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    const textarea = wrapper.find('textarea')
    await textarea.setValue('draft text')
    const listBefore = wrapper.findAll('[data-test="msg-row"]').length
    await wrapper.find('[data-test="details-trigger"]').trigger('click')
    await flushPromises()
    await document.querySelector('[data-test="details-close"]')?.dispatchEvent(new Event('click'))
    await flushPromises()
    expect((wrapper.find('textarea').element as HTMLTextAreaElement).value).toBe('draft text')
    expect(wrapper.findAll('[data-test="msg-row"]').length).toBe(listBefore)
  })

  it('shows the character name in the header from identity', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    vi.spyOn(client, 'getSessionIdentity').mockResolvedValue({
      assistant: { name: 'Neo', avatar: 'a.png' },
      user: { name: null, avatar: null },
    })
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(wrapper.text()).toContain('Neo')
  })

  it('shows a details trigger when a session panel exists', async () => {
    vi.spyOn(client, 'getSessionPanels').mockResolvedValue([
      { id: 'F', name: 'Status', html: '<b>hi</b>', css: '', caps: {}, min_messages: 0 },
    ])
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const w = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(w.find('[data-test="details-trigger"]').exists()).toBe(true)
  })

  it('hides a panel until the chat reaches its min_messages threshold', async () => {
    // No variables declared either — the trigger must depend only on a
    // visible panel, which is withheld by the min_messages threshold.
    vi.spyOn(client, 'getSessionState').mockResolvedValue({ schema: [], values: {} } as never)
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's1', active_leaf_id: null, mounted_packs: ['p1'] } as never)
    vi.spyOn(client, 'getPack').mockResolvedValue({
      id: 'p1', name: 'Alice', identity: { display_name: null, avatar: null },
      meta: { panel: { html: '<span>x</span>', css: '', caps: {}, min_messages: 2 } },
      created_at: '', updated_at: '',
    } as never)
    vi.spyOn(client, 'getSessionPanels').mockResolvedValue([
      { id: 'p1-panel', name: 'Alice', html: '<span>x</span>', css: '', caps: {}, min_messages: 2 },
    ])
    vi.spyOn(client, 'getSessionIdentity').mockResolvedValue({
      assistant: { name: null, avatar: null },
      user: { name: null, avatar: null },
    })
    const oneMessage = [{
      id: 'm1', session_id: 's1', parent_id: null, role: 'user' as const,
      raw_content: 'hi', display_content: null, is_hidden: false, is_anchor: false, attachments: [],
      snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    }]
    vi.spyOn(client, 'listMessages').mockResolvedValue(oneMessage)
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const w = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(w.find('[data-test="details-trigger"]').exists()).toBe(false)
  })

  it('shows a panel once the chat reaches its min_messages threshold', async () => {
    vi.spyOn(client, 'getSession').mockResolvedValue({ id: 's1', active_leaf_id: null, mounted_packs: ['p1'] } as never)
    vi.spyOn(client, 'getPack').mockResolvedValue({
      id: 'p1', name: 'Alice', identity: { display_name: null, avatar: null },
      meta: { panel: { html: '<span>x</span>', css: '', caps: {}, min_messages: 2 } },
      created_at: '', updated_at: '',
    } as never)
    vi.spyOn(client, 'getSessionPanels').mockResolvedValue([
      { id: 'p1-panel', name: 'Alice', html: '<span>x</span>', css: '', caps: {}, min_messages: 2 },
    ])
    vi.spyOn(client, 'getSessionIdentity').mockResolvedValue({
      assistant: { name: null, avatar: null },
      user: { name: null, avatar: null },
    })
    const twoMessages = [0, 1].map((i) => ({
      id: `m${i}`, session_id: 's1', parent_id: null, role: 'user' as const,
      raw_content: 'hi', display_content: null, is_hidden: false, is_anchor: false, attachments: [],
      snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    }))
    vi.spyOn(client, 'listMessages').mockResolvedValue(twoMessages)
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const w = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(w.find('[data-test="details-trigger"]').exists()).toBe(true)
  })

  it('prefers $assistant_name and $avatar overrides over the resolved identity', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([])
    vi.spyOn(client, 'getSessionIdentity').mockResolvedValue({
      assistant: { name: 'Neo', avatar: 'a.png' },
      user: { name: null, avatar: null },
    })
    vi.spyOn(client, 'getSessionState').mockResolvedValue({
      schema: [],
      values: { $assistant_name: 'Trinity', $avatar: 'b.png' },
    } as never)
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(wrapper.text()).toContain('Trinity')
    expect(wrapper.text()).not.toContain('Neo')
  })
})
