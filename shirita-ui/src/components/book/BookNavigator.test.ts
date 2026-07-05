import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import BookNavigator from './BookNavigator.vue'
import SessionTemplateRoot from './SessionTemplateRoot.vue'
import { LOCAL_BOOK_KEY } from './types'
import { blankDefHolder } from './_testkit'

// BookNavigator itself doesn't read the API, but its rendered children do:
// SessionTemplateRoot reads templateNodes, and DefinitionView (mounted when the
// stack top is a definition target) calls editLocal on mount. blankDefHolder
// gives every descendant a complete, no-op LocalBookApi.
function mountNavigator() {
  return mount(BookNavigator, {
    props: { rootTarget: { kind: 'sessionRoot' } },
    global: { provide: { [LOCAL_BOOK_KEY as symbol]: blankDefHolder() } },
  })
}

describe('BookNavigator', () => {
  it('renders only the root level initially and shows no back button', () => {
    const w = mountNavigator()
    expect(w.find('[data-test="nav-back"]').exists()).toBe(false)
    expect(w.find('[data-test="nav-level-sessionRoot"]').exists()).toBe(true)
  })

  it('pushes a definition target on the drill event and shows back', async () => {
    const w = mountNavigator()
    // Emit on the child component instance directly — find() by CSS selector
    // returns a DOMWrapper whose .vm is undefined in this VTU/jsdom setup.
    w.findComponent(SessionTemplateRoot).vm.$emit('drill', { kind: 'definition', definitionId: 'd1' })
    await w.vm.$nextTick()
    expect(w.find('[data-test="nav-level-definition"]').exists()).toBe(true)
    expect(w.find('[data-test="nav-back"]').exists()).toBe(true)
  })

  it('pop returns to the previous level and sets backward direction', async () => {
    const w = mountNavigator()
    w.findComponent(SessionTemplateRoot).vm.$emit('drill', { kind: 'definition', definitionId: 'd1' })
    await w.vm.$nextTick()
    await w.find('[data-test="nav-back"]').trigger('click')
    expect(w.find('[data-test="nav-level-sessionRoot"]').exists()).toBe(true)
    expect(w.find('[data-test="nav-level-definition"]').exists()).toBe(false)
  })
})
