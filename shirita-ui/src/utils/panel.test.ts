import { describe, it, expect } from 'vitest'
import { sanitizePanelHtml, fenceCss } from './panel'

describe('sanitizePanelHtml', () => {
  it('strips <script>, on* handlers and javascript: urls', () => {
    const out = sanitizePanelHtml(
      '<div onclick="evil()">hi<script>alert(1)</script><a href="javascript:alert(1)">x</a></div>',
    )
    expect(out).not.toContain('<script')
    expect(out).not.toContain('onclick')
    expect(out).not.toContain('javascript:')
  })

  it('keeps safe structure, details/summary, and data-* bindings/actions', () => {
    const out = sanitizePanelHtml(
      '<details data-show="poisoned"><summary>S</summary>' +
      '<span data-bind="hp">x</span>' +
      '<button data-diff-key="hp" data-diff-op="sub" data-diff-value="1">-</button></details>',
    )
    expect(out).toContain('<details')
    expect(out).toContain('data-show="poisoned"')
    expect(out).toContain('data-bind="hp"')
    expect(out).toContain('data-diff-key="hp"')
  })

  it('drops remote img src but keeps /assets', () => {
    expect(sanitizePanelHtml('<img src="https://evil.com/x.png">')).not.toContain('evil.com')
    expect(sanitizePanelHtml('<img src="/assets/a.png">')).toContain('/assets/a.png')
  })
})

describe('fenceCss', () => {
  it('strips @import, position:fixed/sticky, remote url(), expression(), behavior', () => {
    const out = fenceCss(
      '@import url(http://x);a{position:fixed}b{background:url(https://e/x.png)}' +
      'c{width:expression(1)}d{behavior:url(#x)}',
    )
    expect(out).not.toMatch(/@import/i)
    expect(out).not.toMatch(/position\s*:\s*fixed/i)
    expect(out).not.toMatch(/url\(\s*https?:/i)
    expect(out).not.toMatch(/expression\(/i)
    expect(out).not.toMatch(/behavior\s*:/i)
  })

  it('leaves safe css and local urls intact', () => {
    const out = fenceCss('.x{color:red;background:url(/assets/b.png)}')
    expect(out).toContain('color:red')
    expect(out).toContain('/assets/b.png')
  })
})
