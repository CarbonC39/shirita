import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useUiStore } from './ui'

describe('ui store', () => {
  beforeEach(() => {
    localStorage.clear()
    setActivePinia(createPinia())
  })

  it('defaults to bubble style and system theme', () => {
    const ui = useUiStore()
    expect(ui.messageStyle).toBe('bubble')
    expect(ui.theme).toBe('system')
  })

  it('persists message style to localStorage', () => {
    const ui = useUiStore()
    ui.setMessageStyle('flat')
    expect(ui.messageStyle).toBe('flat')
    expect(localStorage.getItem('ui.messageStyle')).toBe('flat')
  })

  it('defaults content width to 760 and persists changes', () => {
    const ui = useUiStore()
    expect(ui.contentWidth).toBe(760)
    ui.setContentWidth(900)
    expect(ui.contentWidth).toBe(900)
    expect(localStorage.getItem('ui.contentWidth')).toBe('900')
  })

  it('tracks the active chat id', () => {
    const ui = useUiStore()
    expect(ui.activeChatId).toBeNull()
    ui.setActiveChatId('abc')
    expect(ui.activeChatId).toBe('abc')
    ui.setActiveChatId(null)
    expect(ui.activeChatId).toBeNull()
  })
})
