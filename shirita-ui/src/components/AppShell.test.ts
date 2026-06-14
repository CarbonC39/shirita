import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import AppShell from './AppShell.vue'

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
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
      global: { plugins: [router] },
      slots: { default: '<p>content</p>' },
    })
    expect(wrapper.findAll('nav a')).toHaveLength(3)
    expect(wrapper.text()).toContain('content')
  })

  it('marks the book section active in grayscale and others muted', async () => {
    const router = makeRouter()
    router.push('/book')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: [router] } })
    const links = wrapper.findAll('nav a')
    expect(links[1].classes()).toContain('text-ink')        // active book
    expect(links[0].classes()).toContain('text-muted/40')   // inactive chat
    expect(links[2].classes()).toContain('text-muted/40')   // inactive settings
  })

  it('renders a footer with project name', async () => {
    const router = makeRouter()
    router.push('/')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: [router] } })
    const footer = wrapper.find('footer')
    expect(footer.exists()).toBe(true)
    expect(footer.text()).toContain('Shirita')
  })
})
