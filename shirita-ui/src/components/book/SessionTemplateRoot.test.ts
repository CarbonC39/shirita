import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import SessionTemplateRoot from './SessionTemplateRoot.vue'
import { LOCAL_BOOK_KEY, type LocalBookApi } from './types'

function mountWith(api: Partial<LocalBookApi>) {
  return mount(SessionTemplateRoot, {
    global: { provide: { [LOCAL_BOOK_KEY as symbol]: api } },
  })
}

describe('SessionTemplateRoot', () => {
  it('renders the template tree with only template-sourced nodes', () => {
    const api = {
      templateNodes: { value: [] },
      definitions: [],
      types: [],
      tree: {},
    } as any
    const w = mountWith(api)
    expect(w.findComponent({ name: 'PromptTree' }).exists()).toBe(true)
  })

  it('emits drill with a definition target when a tree row is opened', async () => {
    const api = {
      templateNodes: { value: [{ id: 'n1', owner_kind: 'session', owner_id: 's', parent_id: null,
        sort_order: 0, kind: 'ref', tag: null, definition_id: 'def-1', enabled: true, created_at: '', meta: {} }] },
      definitions: [{ id: 'def-1', type: 'prompt', name: 'D1', content: 'c', meta: {} }],
      types: [],
      tree: {},
    } as any
    const w = mountWith(api)
    await w.find('[data-test="node-row-n1"]').trigger('click')
    expect(w.emitted('drill')).toEqual([[{ kind: 'definition', definitionId: 'def-1' }]])
  })
})
