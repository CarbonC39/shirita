import { describe, it, expect } from 'vitest'
import { parseMarkdown } from './markdown'

describe('parseMarkdown', () => {
  it('returns a single text node for plain prose (newlines preserved as text)', () => {
    expect(parseMarkdown('hello\nworld')).toEqual([{ type: 'text', value: 'hello\nworld' }])
  })

  it('parses **bold**', () => {
    expect(parseMarkdown('a **b** c')).toEqual([
      { type: 'text', value: 'a ' },
      { type: 'strong', children: [{ type: 'text', value: 'b' }] },
      { type: 'text', value: ' c' },
    ])
  })

  it('parses *italic* and _italic_', () => {
    expect(parseMarkdown('*i*')).toEqual([{ type: 'em', children: [{ type: 'text', value: 'i' }] }])
    expect(parseMarkdown('_j_')).toEqual([{ type: 'em', children: [{ type: 'text', value: 'j' }] }])
  })

  it('does not treat ** as italic', () => {
    expect(parseMarkdown('**b**')).toEqual([{ type: 'strong', children: [{ type: 'text', value: 'b' }] }])
  })

  it('parses ~~strikethrough~~', () => {
    expect(parseMarkdown('~~x~~')).toEqual([{ type: 'del', children: [{ type: 'text', value: 'x' }] }])
  })

  it('parses `inline code` without parsing its contents', () => {
    expect(parseMarkdown('`a*b*c`')).toEqual([{ type: 'code', value: 'a*b*c' }])
  })

  it('nests emphasis (bold containing italic)', () => {
    expect(parseMarkdown('**a _b_**')).toEqual([
      {
        type: 'strong',
        children: [
          { type: 'text', value: 'a ' },
          { type: 'em', children: [{ type: 'text', value: 'b' }] },
        ],
      },
    ])
  })

  it('parses a safe link', () => {
    expect(parseMarkdown('[t](https://x.com)')).toEqual([
      { type: 'link', href: 'https://x.com', children: [{ type: 'text', value: 't' }] },
    ])
  })

  it('renders a javascript: link as plain text (no link node)', () => {
    // eslint-disable-next-line no-script-url
    expect(parseMarkdown('[t](javascript:alert(1))')).toEqual([
      { type: 'text', value: '[t](javascript:alert(1))' },
    ])
  })

  it('parses a fenced code block with a language, leaving its body literal', () => {
    expect(parseMarkdown('before\n```js\nlet x = **1**\n```\nafter')).toEqual([
      { type: 'text', value: 'before\n' },
      { type: 'codeblock', lang: 'js', value: 'let x = **1**\n' },
      { type: 'text', value: '\nafter' },
    ])
  })
})
