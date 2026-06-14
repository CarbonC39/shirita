import { describe, it, expect } from 'vitest'
import { estimateTokens, formatTokens } from './tokens'

describe('estimateTokens', () => {
  it('is zero for empty input', () => {
    expect(estimateTokens('')).toBe(0)
  })

  it('grows roughly with length', () => {
    const short = estimateTokens('hello world')
    const long = estimateTokens('hello world '.repeat(50))
    expect(long).toBeGreaterThan(short)
    expect(short).toBeGreaterThanOrEqual(1)
  })

  it('counts CJK characters more densely than latin chars/4', () => {
    // 10 CJK chars ≈ 10 tokens, far more than 10 latin chars (~3).
    expect(estimateTokens('你好世界一二三四五六')).toBeGreaterThanOrEqual(9)
    expect(estimateTokens('abcdefghij')).toBeLessThan(5)
  })
})

describe('formatTokens', () => {
  it('groups thousands', () => {
    expect(formatTokens(1234)).toBe('1,234')
  })
})
