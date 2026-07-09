import { describe, it, expect, beforeEach, vi } from 'vitest'

vi.mock('../stores/ui', () => {
  const s = { theme: 'dark' as string }
  return { useUiStore: () => s }
})

import { useTheme } from './useTheme'
import { invokeComposable } from './_testkit'
import { useUiStore } from '../stores/ui'

function matchMedia(matches: boolean) {
  return vi.fn(() => ({ matches, addEventListener: vi.fn(), removeEventListener: vi.fn() }))
}

describe('useTheme', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.documentElement.classList.remove('dark')
    window.matchMedia = matchMedia(false) as any
  })

  it('applies the dark class for theme "dark"', () => {
    useUiStore().theme = 'dark'
    invokeComposable(() => useTheme())
    expect(document.documentElement.classList.contains('dark')).toBe(true)
  })

  it('removes the dark class for theme "light"', () => {
    useUiStore().theme = 'light'
    invokeComposable(() => useTheme())
    expect(document.documentElement.classList.contains('dark')).toBe(false)
  })

  it('follows the OS preference while on "system"', () => {
    window.matchMedia = matchMedia(true) as any
    useUiStore().theme = 'system'
    invokeComposable(() => useTheme())
    expect(document.documentElement.classList.contains('dark')).toBe(true)

    document.documentElement.classList.remove('dark')
    window.matchMedia = matchMedia(false) as any
    useUiStore().theme = 'system'
    invokeComposable(() => useTheme())
    expect(document.documentElement.classList.contains('dark')).toBe(false)
  })
})
