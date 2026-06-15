import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import { setActivePinia, createPinia } from 'pinia'
import * as client from '../api/client'
import HomeView from './HomeView.vue'

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: HomeView },
      { path: '/chat/:id', component: { template: '<div />' } },
      { path: '/new', component: { template: '<div />' } },
    ],
  })
}

describe('HomeView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.restoreAllMocks()
  })

  it('renders a card per session from the api', async () => {
    vi.spyOn(client, 'listSessions').mockResolvedValue([
      { id: 's1', name: 'Neo', avatar: null, override_config: {}, current_state: {}, mounted_definitions: [] },
      { id: 's2', name: 'Trinity', avatar: null, override_config: {}, current_state: {}, mounted_definitions: [] },
    ])
    const router = makeRouter()
    router.push('/')
    await router.isReady()

    const wrapper = mount(HomeView, { global: { plugins: [router] } })
    await flushPromises()

    expect(wrapper.text()).toContain('Neo')
    expect(wrapper.text()).toContain('Trinity')
    expect(wrapper.findAll('a[href^="/chat/"]')).toHaveLength(2)
  })

  it('edit mode swaps card menus for drag + delete affordances', async () => {
    vi.spyOn(client, 'listSessions').mockResolvedValue([
      { id: 's1', name: 'Neo', avatar: null, override_config: {}, current_state: {}, mounted_definitions: [] },
    ])
    const router = makeRouter()
    router.push('/')
    await router.isReady()

    const wrapper = mount(HomeView, { global: { plugins: [router] } })
    await flushPromises()

    expect(wrapper.find('[data-test="chat-menu"]').exists()).toBe(true)
    await wrapper.find('[data-test="edit-toggle"]').trigger('click')
    expect(wrapper.find('[data-test="chat-menu"]').exists()).toBe(false)
    expect(wrapper.find('[data-test="chat-delete"]').exists()).toBe(true)
  })

  it('shows an empty state when there are no sessions', async () => {
    vi.spyOn(client, 'listSessions').mockResolvedValue([])
    const router = makeRouter()
    router.push('/')
    await router.isReady()

    const wrapper = mount(HomeView, { global: { plugins: [router] } })
    await flushPromises()

    expect(wrapper.text()).toContain('No conversations yet.')
  })
})
