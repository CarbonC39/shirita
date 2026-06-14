import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { createRouter, createMemoryHistory } from 'vue-router'
import ChatCard from './ChatCard.vue'

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/chat/:id', component: { template: '<div />' } }],
  })
}

const session = { id: 's1', name: 'Neo', avatar: null, override_config: {}, current_state: {}, mounted_definitions: [] }

describe('ChatCard', () => {
  it('links to the conversation', () => {
    const w = mount(ChatCard, { props: { session }, global: { plugins: [makeRouter()] } })
    expect(w.find('a').attributes('href')).toContain('/chat/s1')
  })

  it('opens a menu and emits actions without navigating', async () => {
    const w = mount(ChatCard, { props: { session }, global: { plugins: [makeRouter()] } })
    expect(w.find('[data-test="chat-delete"]').exists()).toBe(false)
    await w.find('[data-test="chat-menu"]').trigger('click')
    expect(w.find('[data-test="chat-delete"]').exists()).toBe(true)
    await w.find('[data-test="chat-delete"]').trigger('click')
    expect(w.emitted('delete')![0]).toEqual(['s1'])
  })
})
