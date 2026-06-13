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

  it('marks the book section active on /book', async () => {
    const router = makeRouter()
    router.push('/book')
    await router.isReady()
    const wrapper = mount(AppShell, { global: { plugins: [router] } })
    const bookLink = wrapper.findAll('nav a')[1]
    expect(bookLink.classes()).toContain('text-ink')
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
