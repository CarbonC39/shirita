import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
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
    expect(links[0].classes()).toContain('text-ink/25')   // inactive chat
    expect(links[2].classes()).toContain('text-ink/25')   // inactive settings
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

  it('has no footer', async () => {
    const router = makeRouter()
    router.push('/')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: plugins(router) } })
    expect(wrapper.find('footer').exists()).toBe(false)
  })
})
