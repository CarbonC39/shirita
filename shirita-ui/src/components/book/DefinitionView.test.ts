import { describe, it, expect, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { ref, reactive } from 'vue'
import DefinitionView from './DefinitionView.vue'
import { LOCAL_BOOK_KEY, type LocalBookApi } from './types'
import { blankDefHolder } from './_testkit'

describe('DefinitionView (local override)', () => {
  it('loads the definition into the editor on mount via editLocal', async () => {
    const editLocal = vi.fn()
    const api = {
      definitions: [{ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: {} }],
      types: [],
      localDefs: { value: {} },
      localEditDef: reactive({ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: {} }),
      localDefActive: ref(true),
      localSavedTick: ref(0),
      editLocal,
      saveLocal: vi.fn(),
      revertLocal: vi.fn(),
      defName: () => 'N',
    } as any
    mount(DefinitionView, {
      props: { definitionId: 'd1' },
      global: { provide: { [LOCAL_BOOK_KEY as symbol]: api } },
    })
    await flushPromises()
    expect(editLocal).toHaveBeenCalledWith('d1')
  })

  it('saves via the injected saveLocal handler', async () => {
    const saveLocal = vi.fn().mockResolvedValue(undefined)
    const api = blankDefHolder({ saveLocal })
    const w = mount(DefinitionView, {
      props: { definitionId: 'd1' },
      global: { provide: { [LOCAL_BOOK_KEY as symbol]: api } },
    })
    await flushPromises()
    await w.findComponent({ name: 'DefinitionEditor' }).vm.$emit('save')
    await flushPromises()
    expect(saveLocal).toHaveBeenCalled()
  })

  it('shows the revert button only when the definition has a local override and calls revertLocal on click', async () => {
    const revertLocal = vi.fn().mockResolvedValue(undefined)
    const api = blankDefHolder({
      revertLocal,
      localDefs: ref({ d1: { name: 'N' } }) as any,
    })
    const w = mount(DefinitionView, {
      props: { definitionId: 'd1' },
      global: { provide: { [LOCAL_BOOK_KEY as symbol]: api } },
    })
    await flushPromises()
    // Override present: revert button renders.
    const btn = w.find('[data-test="def-revert"]')
    expect(btn.exists()).toBe(true)
    await btn.trigger('click')
    expect(revertLocal).toHaveBeenCalledWith('d1')
  })

  it('hides the revert button when no local override exists', async () => {
    const api = blankDefHolder({ localDefs: ref({}) as any })
    const w = mount(DefinitionView, {
      props: { definitionId: 'd1' },
      global: { provide: { [LOCAL_BOOK_KEY as symbol]: api } },
    })
    await flushPromises()
    expect(w.find('[data-test="def-revert"]').exists()).toBe(false)
  })
})
