import { describe, it, expect } from 'vitest'
import { nextTick } from 'vue'
import { mount } from '@vue/test-utils'
import PanelView from './PanelView.vue'

function shadowOf(w: ReturnType<typeof mount>): ShadowRoot {
  return (w.element as HTMLElement).shadowRoot as ShadowRoot
}

describe('PanelView', () => {
  it('renders sanitized html bound to values inside a shadow root', async () => {
    const w = mount(PanelView, { props: { html: '<span data-bind="hp">x</span>', css: '', values: { hp: 100 } } })
    await nextTick()
    const sr = shadowOf(w)
    expect(sr).toBeTruthy()
    expect(sr.querySelector('[data-bind="hp"]')!.textContent).toBe('100')
  })

  it('interpolates {{var}} and updates on a values change', async () => {
    const w = mount(PanelView, { props: { html: '<p>HP: {{hp}}</p>', css: '', values: { hp: 100 } } })
    await nextTick()
    const sr = shadowOf(w)
    expect(sr.querySelector('p')!.textContent).toContain('100')
    await w.setProps({ values: { hp: 90 } })
    await nextTick()
    expect(sr.querySelector('p')!.textContent).toContain('90')
  })

  it('hides data-show elements when the var is falsy', async () => {
    const w = mount(PanelView, { props: { html: '<span data-show="poisoned">poison</span>', css: '', values: { poisoned: false } } })
    await nextTick()
    const el = shadowOf(w).querySelector('[data-show]') as HTMLElement
    expect(el.style.display).toBe('none')
    await w.setProps({ values: { poisoned: true } })
    await nextTick()
    expect((shadowOf(w).querySelector('[data-show]') as HTMLElement).style.display).toBe('')
  })

  it('preserves an opened <details> across a value change (morph, not rebuild)', async () => {
    const w = mount(PanelView, {
      props: { html: '<details><summary>s</summary><span data-bind="hp">x</span></details>', css: '', values: { hp: 1 } },
    })
    await nextTick()
    const sr = shadowOf(w)
    ;(sr.querySelector('details') as HTMLDetailsElement).open = true // user opens it
    await w.setProps({ values: { hp: 2 } })
    await nextTick()
    expect((sr.querySelector('details') as HTMLDetailsElement).open).toBe(true) // still open
    expect(sr.querySelector('[data-bind="hp"]')!.textContent).toBe('2')         // and updated
  })

  it('forces containment styles on the host', () => {
    const w = mount(PanelView, { props: { html: '', css: '', values: {} } })
    const style = (w.element as HTMLElement).getAttribute('style') || ''
    expect(style).toContain('position: relative')
    expect(style).toContain('overflow: hidden')
    expect(style).toContain('contain: content')
  })
})
