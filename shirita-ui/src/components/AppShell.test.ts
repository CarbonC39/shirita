import { describe, it, expect } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import AppShell from './AppShell.vue'

function plugins(router: ReturnType<typeof makeRouter>) {
  return [router, createPinia()]
}

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/chat/:id', name: 'chat', component: { template: '<div />' } },
      { path: '/book', component: { template: '<div />' } },
      { path: '/settings', component: { template: '<div />' } },
    ],
  })
}

describe('AppShell', () => {
  function host(wrapper: ReturnType<typeof mount>) {
    return wrapper.find('[data-test="route-host"]')
  }

  it('route host exposes ordinary-page scrolling mode on /book', async () => {
    const router = makeRouter()
    router.push('/book')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    expect(host(wrapper).attributes('data-layout')).toBe('page')
  })

  it('route host exposes the workspace (non-scrolling) mode on /chat/:id', async () => {
    const router = makeRouter()
    router.push('/chat/abc')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    expect(host(wrapper).attributes('data-layout')).toBe('workspace')
  })

  it('chat does not render the mobile breadcrumb row', async () => {
    const router = makeRouter()
    router.push('/chat/abc')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    expect(wrapper.find('[data-test="mobile-crumbs"]').exists()).toBe(false)
  })

  it('a non-chat route with crumbs still renders the mobile breadcrumb row', async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/chat/:id', name: 'chat', component: { template: '<div />' } },
        { path: '/new', meta: { crumbs: [{ label: 'chat.title', to: '/' }, { label: 'shell.new' }] }, component: { template: '<div />' } },
      ],
    })
    router.push('/new')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    expect(wrapper.find('[data-test="mobile-crumbs"]').exists()).toBe(true)
  })

  it('renders three nav links and a slot', async () => {
    const router = makeRouter()
    router.push('/')
    await router.isReady()
    const wrapper = mount(AppShell, {
      global: { plugins: plugins(router) },
      slots: { default: '<p>content</p>' },
    })
    expect(wrapper.findAll('nav a')).toHaveLength(3)
    expect(wrapper.text()).toContain('content')
  })

  it('marks the active section dark and inactive ones in lighter grayscale', async () => {
    const router = makeRouter()
    router.push('/book')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    const links = wrapper.findAll('nav a')
    expect(links[1].classes()).toContain('text-ink')      // active book
    expect(links[0].classes()).toContain('text-muted')    // inactive chat
    expect(links[2].classes()).toContain('text-muted')    // inactive settings
  })

  it('points the chat icon at the current conversation when inside one', async () => {
    const router = makeRouter()
    router.push('/chat/abc')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    const chatLink = wrapper.findAll('nav a')[0]
    expect(chatLink.attributes('href')).toContain('/chat/abc')
  })

  it('points the chat icon at the list when not in a conversation', async () => {
    const router = makeRouter()
    router.push('/settings')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    const chatLink = wrapper.findAll('nav a')[0]
    expect(chatLink.attributes('href')).toBe('/')
  })

  it('keeps the chat icon on the conversation while you browse book and settings', async () => {
    const router = makeRouter()
    router.push('/chat/abc')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    const chatLink = () => wrapper.findAll('nav a')[0]
    expect(chatLink().attributes('href')).toContain('/chat/abc')

    router.push('/book')
    await flushPromises()
    expect(chatLink().attributes('href')).toContain('/chat/abc')

    router.push('/settings')
    await flushPromises()
    expect(chatLink().attributes('href')).toContain('/chat/abc')
  })

  it('forgets the conversation once you return to the list', async () => {
    const router = makeRouter()
    router.push('/chat/abc')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    const chatLink = () => wrapper.findAll('nav a')[0]

    router.push('/')
    await flushPromises()
    expect(chatLink().attributes('href')).toBe('/')

    router.push('/settings')
    await flushPromises()
    expect(chatLink().attributes('href')).toBe('/')
  })

  it('renders the brand mark as an image, not a letter', async () => {
    const router = makeRouter()
    router.push('/')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    const img = wrapper.find('[data-test="brand"] img')
    expect(img.exists()).toBe(true)
    expect(img.attributes('alt')).toBe('Shirita')
  })

  it('has no footer', async () => {
    const router = makeRouter()
    router.push('/')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    expect(wrapper.find('footer').exists()).toBe(false)
  })
})
