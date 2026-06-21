import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'

const { push } = vi.hoisted(() => ({ push: vi.fn() }))
vi.mock('vue-router', () => ({ useRouter: () => ({ push }) }))

vi.mock('../api/client', () => ({
  createSession: vi.fn().mockResolvedValue({ id: 'c9' }),
}))

const templates = [
  { id: 't1', name: 'Default' },
  { id: 't2', name: 'Other' },
]
const packs = [
  { id: 'p1', name: 'Alice', identity: { avatar: '', display_name: '' }, meta: {} },
  { id: 'p2', name: 'Lorebook', identity: { avatar: '', display_name: '' }, meta: {} },
]
vi.mock('../stores/library', () => ({
  useLibraryStore: () => ({ templates, packs, loadAll: vi.fn() }),
}))

import NewChatView from './NewChatView.vue'
import * as api from '../api/client'

describe('NewChatView (single screen)', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    ;(api.createSession as any).mockClear()
    push.mockClear()
  })

  it('renders the screen with a template picker and a create button', async () => {
    const w = mount(NewChatView)
    await flushPromises()
    expect(w.find('[data-test="new-chat"]').exists()).toBe(true)
    expect(w.find('[data-test="template-picker"]').exists()).toBe(true)
    expect(w.find('[data-test="create-chat"]').exists()).toBe(true)
  })

  it('defaults the template to the first one and creates a chat with no packs', async () => {
    const w = mount(NewChatView)
    await flushPromises()
    await w.find('[data-test="chat-name"]').setValue('My chat')
    await w.find('[data-test="create-chat"]').trigger('click')
    await flushPromises()
    expect(api.createSession).toHaveBeenCalledWith('My chat', 't1', null, [])
    expect(push).toHaveBeenCalledWith('/chat/c9')
  })
})
