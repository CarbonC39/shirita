import { describe, it, expect } from 'vitest'
import { splitThinking } from './thinking'

describe('splitThinking', () => {
  it('returns one text segment when there is no think block', () => {
    expect(splitThinking('hello')).toEqual([{ type: 'text', content: 'hello' }])
  })

  it('splits a closed think block from the answer', () => {
    expect(splitThinking('<think>reason</think>answer')).toEqual([
      { type: 'think', content: 'reason', open: false },
      { type: 'text', content: 'answer' },
    ])
  })

  it('keeps surrounding text in order', () => {
    expect(splitThinking('a<think>r</think>b')).toEqual([
      { type: 'text', content: 'a' },
      { type: 'think', content: 'r', open: false },
      { type: 'text', content: 'b' },
    ])
  })

  it('treats an unclosed think block (mid-stream) as open', () => {
    expect(splitThinking('<think>still going')).toEqual([
      { type: 'think', content: 'still going', open: true },
    ])
  })

  it('returns nothing for empty input', () => {
    expect(splitThinking('')).toEqual([])
  })
})
