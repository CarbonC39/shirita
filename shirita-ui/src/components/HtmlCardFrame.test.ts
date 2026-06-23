import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import HtmlCardFrame from './HtmlCardFrame.vue'

function tokenOf(w: ReturnType<typeof mount>): string {
  const iframe = w.find('iframe').element as HTMLIFrameElement
  const srcdoc = iframe.getAttribute('srcdoc') || ''
  const m = srcdoc.match(/token: '([a-z0-9]+)'/)
  if (!m) throw new Error('token not found in srcdoc')
  return m[1]
}

function postReport(data: unknown) {
  window.dispatchEvent(new MessageEvent('message', { data }))
}

describe('HtmlCardFrame', () => {
  it('starts at the default 640px height before any size report', () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('640px')
  })

  it('resizes to a reported height carrying its own token', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    postReport({ source: 'shirita-html-card', token: tokenOf(w), height: 1200 })
    await w.vm.$nextTick()
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('1200px')
  })

  it('ignores a message carrying a different token', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    postReport({ source: 'shirita-html-card', token: 'not-the-real-token', height: 1200 })
    await w.vm.$nextTick()
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('640px')
  })

  it('ignores a message with a different source tag', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    postReport({ source: 'something-else', token: tokenOf(w), height: 1200 })
    await w.vm.$nextTick()
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('640px')
  })

  it('clamps a too-small reported height up to 80px', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    postReport({ source: 'shirita-html-card', token: tokenOf(w), height: 50 })
    await w.vm.$nextTick()
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('80px')
  })

  it('clamps a too-large reported height down to 4000px', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    postReport({ source: 'shirita-html-card', token: tokenOf(w), height: 9000 })
    await w.vm.$nextTick()
    const iframe = w.find('iframe').element as HTMLIFrameElement
    expect(iframe.style.height).toBe('4000px')
  })

  it('removes the message listener on unmount', async () => {
    const w = mount(HtmlCardFrame, { props: { html: '<p>hi</p>' } })
    const token = tokenOf(w)
    w.unmount()
    expect(() => postReport({ source: 'shirita-html-card', token, height: 1200 })).not.toThrow()
  })

  it('two instances do not cross-react to each other’s reports', async () => {
    const a = mount(HtmlCardFrame, { props: { html: '<p>a</p>' } })
    const b = mount(HtmlCardFrame, { props: { html: '<p>b</p>' } })
    postReport({ source: 'shirita-html-card', token: tokenOf(a), height: 999 })
    await a.vm.$nextTick()
    await b.vm.$nextTick()
    expect((a.find('iframe').element as HTMLIFrameElement).style.height).toBe('999px')
    expect((b.find('iframe').element as HTMLIFrameElement).style.height).toBe('640px')
  })
})
