import { describe, it, expect, afterEach, vi } from 'vitest'
import { normalizeLocale, resolveInitialLocale, SUPPORTED } from './resolve'

describe('normalizeLocale', () => {
  it.each([
    ['zh-CN', 'zh-Hans'],
    ['zh', 'zh-Hans'],
    ['zh-Hans', 'zh-Hans'],
    ['zh-SG', 'zh-Hans'],
    ['zh-TW', 'zh-Hant'],
    ['zh-HK', 'zh-Hant'],
    ['zh-MO', 'zh-Hant'],
    ['zh-Hant', 'zh-Hant'],
    ['zh-Hant-HK', 'zh-Hant'],
    ['ja-JP', 'ja'],
    ['ja', 'ja'],
    ['en-US', 'en'],
    ['en', 'en'],
  ])('maps %s -> %s', (input, expected) => {
    expect(normalizeLocale(input)).toBe(expected)
  })

  it.each([['fr'], ['de-DE'], [''], [null], [undefined]])(
    'returns null for unsupported %s',
    (input) => {
      expect(normalizeLocale(input as string | null | undefined)).toBeNull()
    },
  )
})

describe('SUPPORTED', () => {
  it('lists the four app locales', () => {
    expect(SUPPORTED).toEqual(['en', 'zh-Hans', 'zh-Hant', 'ja'])
  })
})

describe('resolveInitialLocale', () => {
  afterEach(() => {
    localStorage.clear()
    vi.unstubAllGlobals()
  })

  it('prefers a valid localStorage ui.locale', () => {
    localStorage.setItem('ui.locale', 'ja')
    expect(resolveInitialLocale()).toBe('ja')
  })

  it('falls back to navigator.language when no localStorage', () => {
    vi.stubGlobal('navigator', { language: 'zh-TW' })
    expect(resolveInitialLocale()).toBe('zh-Hant')
  })

  it('falls back to en when nothing matches', () => {
    vi.stubGlobal('navigator', { language: 'fr-FR' })
    expect(resolveInitialLocale()).toBe('en')
  })
})
