import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import { setActivePinia, createPinia } from 'pinia'
import * as client from '../api/client'
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
  })

  it('loads messages on mount', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([{
      id: 'm1', session_id: 's1', parent_id: null, role: 'user',
      raw_content: 'hi', display_content: null, is_hidden: false,
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
      raw_content: 'hello', display_content: null, is_hidden: false,
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

  it('shows error state', async () => {
    vi.spyOn(client, 'listMessages').mockRejectedValue(new Error('Not found'))
    const router = makeRouter()
    router.push('/chat/s1')
    await router.isReady()
    const wrapper = mount(ChatView, { global: { plugins: [router] } })
    await flushPromises()
    expect(wrapper.text()).toContain('Not found')
  })

  it('calls send on composer submit', async () => {
    vi.spyOn(client, 'listMessages').mockResolvedValue([{
      id: 'm1', session_id: 's1', parent_id: null, role: 'user',
      raw_content: 'hi', display_content: null, is_hidden: false,
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
    expect(sendSpy).toHaveBeenCalledWith('s1', 'hello')
  })
})
