import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import MarkdownText from './MarkdownText.vue'

describe('MarkdownText', () => {
  it('renders **bold** as a <strong> element', () => {
    const w = mount(MarkdownText, { props: { text: 'a **b** c' } })
    expect(w.find('strong').exists()).toBe(true)
    expect(w.find('strong').text()).toBe('b')
    expect(w.text()).toBe('a b c')
  })

  it('renders *italic* as <em> and `code` as <code>', () => {
    const w = mount(MarkdownText, { props: { text: '*i* `x`' } })
    expect(w.find('em').text()).toBe('i')
    expect(w.find('code').text()).toBe('x')
  })

  it('renders a fenced code block as <pre>', () => {
    const w = mount(MarkdownText, { props: { text: '```\nhi\n```' } })
    expect(w.find('pre').exists()).toBe(true)
    expect(w.find('pre').text()).toContain('hi')
  })

  it('renders a safe link as an <a> with a blank target', () => {
    const w = mount(MarkdownText, { props: { text: '[t](https://x.com)' } })
    const a = w.find('a')
    expect(a.exists()).toBe(true)
    expect(a.attributes('href')).toBe('https://x.com')
    expect(a.attributes('rel')).toContain('noopener')
  })

  it('does NOT create a link for a javascript: url', () => {
    // eslint-disable-next-line no-script-url
    const w = mount(MarkdownText, { props: { text: '[t](javascript:alert(1))' } })
    expect(w.find('a').exists()).toBe(false)
    expect(w.text()).toContain('javascript:alert(1)')
  })

  it('never injects raw HTML (angle brackets stay literal text)', () => {
    const w = mount(MarkdownText, { props: { text: 'a <img src=x onerror=alert(1)> b' } })
    expect(w.find('img').exists()).toBe(false)
    expect(w.text()).toContain('<img src=x onerror=alert(1)>')
  })

  it('renders a full HTML document in a sandboxed iframe', () => {
    const html = '<!DOCTYPE html><html><head></head><body>hi</body></html>'
    const w = mount(MarkdownText, { props: { text: html } })
    const frame = w.find('iframe')
    expect(frame.exists()).toBe(true)
    expect(frame.attributes('sandbox')).toBe('allow-scripts')
    // The frame prepends a theme-color base <style> (so cards without their own
    // background don't default to browser white); the original markup follows verbatim.
    expect(frame.attributes('srcdoc')).toContain(html)
  })

  it('renders a fenced HTML document in a sandboxed iframe, not a <pre>', () => {
    const html = '<!DOCTYPE html><html><body>hi</body></html>'
    const w = mount(MarkdownText, { props: { text: '```html\n' + html + '\n```' } })
    expect(w.find('iframe').exists()).toBe(true)
    expect(w.find('pre').exists()).toBe(false)
  })

  it('renders a fenced ```html block in a sandboxed iframe even without a doctype/<html> wrapper', () => {
    // The common real-world ST "HTML card" shape: a bare <div>+<style> snippet,
    // not a full document — the ```html fence alone is the author's signal.
    const snippet = '<div class="card">hi</div>\n<style>.card{color:red}</style>'
    const w = mount(MarkdownText, { props: { text: '```html\n' + snippet + '\n```' } })
    const frame = w.find('iframe')
    expect(frame.exists()).toBe(true)
    expect(w.find('pre').exists()).toBe(false)
    expect(frame.attributes('srcdoc')).toContain('<div class="card">hi</div>')
    expect(frame.attributes('srcdoc')).toContain('<style>.card{color:red}</style>')
  })
})
