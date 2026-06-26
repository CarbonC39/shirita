import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { setActivePinia, createPinia } from 'pinia'

const { push } = vi.hoisted(() => ({ push: vi.fn() }))
vi.mock('vue-router', () => ({ useRouter: () => ({ push }) }))

vi.mock('../api/client', () => ({
  createSession: vi.fn().mockResolvedValue({ id: 'c9' }),
}))

const templates = [
  { id: 't1', name: 'Default', meta: {} },
  { id: 't2', name: 'Other', meta: { default: true } },
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

  it('adds, removes, and reorders mount-pack chips and posts them in order', async () => {
    const EntityPicker = (await import('../components/EntityPicker.vue')).default
    const w = mount(NewChatView)
    await flushPromises()

    const packPicker = w
      .findAllComponents(EntityPicker)
      .find((p) => p.attributes('data-test') === 'pack-picker')!

    // add Alice then Lorebook
    packPicker.vm.$emit('select', 'p1')
    packPicker.vm.$emit('select', 'p2')
    await flushPromises()
    let chips = w.findAll('[data-test="pack-chip"]')
    expect(chips.map((c) => c.text())).toEqual(['Alice', 'Lorebook'])

    // adding a duplicate is a no-op
    packPicker.vm.$emit('select', 'p1')
    await flushPromises()
    expect(w.findAll('[data-test="pack-chip"]').length).toBe(2)

    // reorder: drag Alice (first) onto Lorebook (second) → Lorebook, Alice
    chips = w.findAll('[data-test="pack-chip"]')
    await chips[0].find('[data-test="drag-handle"]').trigger('mousedown')
    await chips[0].trigger('dragstart')
    await chips[1].trigger('drop')
    await flushPromises()
    expect(w.findAll('[data-test="pack-chip"]').map((c) => c.text())).toEqual(['Lorebook', 'Alice'])

    // remove the first chip (now Lorebook)
    await w.findAll('[data-test="pack-chip-remove"]')[0].trigger('click')
    await flushPromises()
    expect(w.findAll('[data-test="pack-chip"]').map((c) => c.text())).toEqual(['Alice'])

    // create posts the surviving pack id with blank name → falls back to pack name
    await w.find('[data-test="create-chat"]').trigger('click')
    await flushPromises()
    expect(api.createSession).toHaveBeenCalledWith('Alice', 't2', null, ['p1'])
  })

  it('auto-selects the template flagged default and creates a chat with no packs', async () => {
    const w = mount(NewChatView)
    await flushPromises()
    await w.find('[data-test="chat-name"]').setValue('My chat')
    await w.find('[data-test="create-chat"]').trigger('click')
    await flushPromises()
    expect(api.createSession).toHaveBeenCalledWith('My chat', 't2', null, [])
    expect(push).toHaveBeenCalledWith('/chat/c9')
  })
})
