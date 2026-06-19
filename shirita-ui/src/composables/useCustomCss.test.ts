import { describe, it, expect, beforeEach } from 'vitest'
import { applyCustomCss, bootCustomCss } from './useCustomCss'

describe('useCustomCss', () => {
  beforeEach(() => { document.head.innerHTML = ''; localStorage.clear() })

  it('creates a single style element and updates its text', () => {
    applyCustomCss('.app-chat-column { color: red }')
    const el = document.getElementById('user-custom-css') as HTMLStyleElement
    expect(el).toBeTruthy()
    expect(el.textContent).toContain('color: red')
    applyCustomCss('.app-composer { color: blue }')
    expect(document.querySelectorAll('#user-custom-css')).toHaveLength(1) // reused, not duplicated
    expect(document.getElementById('user-custom-css')!.textContent).toContain('blue')
  })

  it('caches to localStorage', () => {
    applyCustomCss('.x{}')
    expect(localStorage.getItem('ui.customCss')).toBe('.x{}')
  })

  it('bootCustomCss reads from cache', () => {
    localStorage.setItem('ui.customCss', '.cached { color: green }')
    bootCustomCss()
    const el = document.getElementById('user-custom-css') as HTMLStyleElement
    expect(el).toBeTruthy()
    expect(el.textContent).toContain('green')
  })
})
