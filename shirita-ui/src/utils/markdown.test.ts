import { describe, it, expect } from 'vitest'
import { parseMarkdown, containsHtmlCard } from './markdown'

// Helper: a single-paragraph document wraps its inline content in a paragraph
// block, so tests can focus on the inline parsing shape.
function para(...children: object[]): { type: 'paragraph'; children: object[] } {
  return { type: 'paragraph', children }
}

describe('parseMarkdown', () => {
  it('returns a single paragraph for plain prose (newlines preserved as text)', () => {
    expect(parseMarkdown('hello\nworld')).toEqual([para({ type: 'text', value: 'hello\nworld' })])
  })

  it('parses **bold**', () => {
    expect(parseMarkdown('a **b** c')).toEqual([
      para(
        { type: 'text', value: 'a ' },
        { type: 'strong', children: [{ type: 'text', value: 'b' }] },
        { type: 'text', value: ' c' },
      ),
    ])
  })

  it('parses *italic* and _italic_', () => {
    expect(parseMarkdown('*i*')).toEqual([para({ type: 'em', children: [{ type: 'text', value: 'i' }] })])
    expect(parseMarkdown('_j_')).toEqual([para({ type: 'em', children: [{ type: 'text', value: 'j' }] })])
  })

  it('does not treat ** as italic', () => {
    expect(parseMarkdown('**b**')).toEqual([
      para({ type: 'strong', children: [{ type: 'text', value: 'b' }] }),
    ])
  })

  it('parses ~~strikethrough~~', () => {
    expect(parseMarkdown('~~x~~')).toEqual([para({ type: 'del', children: [{ type: 'text', value: 'x' }] })])
  })

  it('parses `inline code` without parsing its contents', () => {
    expect(parseMarkdown('`a*b*c`')).toEqual([para({ type: 'code', value: 'a*b*c' })])
  })

  it('nests emphasis (bold containing italic)', () => {
    expect(parseMarkdown('**a _b_**')).toEqual([
      para({
        type: 'strong',
        children: [
          { type: 'text', value: 'a ' },
          { type: 'em', children: [{ type: 'text', value: 'b' }] },
        ],
      }),
    ])
  })

  it('parses a safe link', () => {
    expect(parseMarkdown('[t](https://x.com)')).toEqual([
      para({ type: 'link', href: 'https://x.com', children: [{ type: 'text', value: 't' }] }),
    ])
  })

  it('renders a javascript: link as plain text (no link node)', () => {
    // eslint-disable-next-line no-script-url
    expect(parseMarkdown('[t](javascript:alert(1))')).toEqual([
      para({ type: 'text', value: '[t](javascript:alert(1))' }),
    ])
  })

  it('parses a fenced code block with a language, leaving its body literal', () => {
    expect(parseMarkdown('before\n```js\nlet x = **1**\n```\nafter')).toEqual([
      para({ type: 'text', value: 'before' }),
      { type: 'codeblock', lang: 'js', value: 'let x = **1**\n' },
      para({ type: 'text', value: 'after' }),
    ])
  })

  it('parses ATX headings (# .. ######)', () => {
    expect(parseMarkdown('# Title')).toEqual([
      { type: 'heading', level: 1, children: [{ type: 'text', value: 'Title' }] },
    ])
    expect(parseMarkdown('### Sub')).toEqual([
      { type: 'heading', level: 3, children: [{ type: 'text', value: 'Sub' }] },
    ])
  })

  it('parses a horizontal rule (---, ***, ___)', () => {
    expect(parseMarkdown('a\n\n---\n\nb')).toEqual([
      para({ type: 'text', value: 'a' }),
      { type: 'hr' },
      para({ type: 'text', value: 'b' }),
    ])
  })

  it('parses unordered and ordered lists', () => {
    expect(parseMarkdown('- one\n- two')).toEqual([
      { type: 'list', ordered: false, items: [
        { children: [{ type: 'text', value: 'one' }] },
        { children: [{ type: 'text', value: 'two' }] },
      ] },
    ])
    expect(parseMarkdown('1. one\n2. two')).toEqual([
      { type: 'list', ordered: true, items: [
        { children: [{ type: 'text', value: 'one' }] },
        { children: [{ type: 'text', value: 'two' }] },
      ] },
    ])
  })

  it('parses a blockquote', () => {
    expect(parseMarkdown('> quoted')).toEqual([
      { type: 'blockquote', children: [para({ type: 'text', value: 'quoted' })] },
    ])
  })

  it('parses a task list item', () => {
    expect(parseMarkdown('- [x] done\n- [ ] todo')).toEqual([
      { type: 'list', ordered: false, items: [
        { children: [{ type: 'text', value: 'done' }], checked: true },
        { children: [{ type: 'text', value: 'todo' }], checked: false },
      ] },
    ])
  })
})

describe('containsHtmlCard', () => {
  it('is true for a raw HTML document', () => {
    expect(containsHtmlCard('<!DOCTYPE html><html><body>x</body></html>')).toBe(true)
  })

  it('is true for a fenced ```html code block', () => {
    expect(containsHtmlCard('before\n```html\n<div>x</div>\n```\nafter')).toBe(true)
  })

  it('is true for an unlabeled fenced block whose content looks like a document', () => {
    expect(containsHtmlCard('```\n<!doctype html><html></html>\n```')).toBe(true)
  })

  it('is false for plain text and non-html fenced code', () => {
    expect(containsHtmlCard('just text')).toBe(false)
    expect(containsHtmlCard('```js\nconst x = 1\n```')).toBe(false)
  })
})
