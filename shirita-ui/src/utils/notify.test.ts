import { describe, it, expect, vi, beforeEach } from 'vitest'
import { notifyReplyDone } from './notify'

describe('notifyReplyDone', () => {
  beforeEach(() => { vi.restoreAllMocks() })

  it('does not notify when the tab is visible', () => {
    const ctor = vi.fn()
    vi.stubGlobal('Notification', Object.assign(ctor, { permission: 'granted' }))
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true })
    notifyReplyDone('t', 'b')
    expect(ctor).not.toHaveBeenCalled()
  })

  it('notifies when hidden and permitted', () => {
    const ctor = vi.fn()
    vi.stubGlobal('Notification', Object.assign(ctor, { permission: 'granted' }))
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true })
    notifyReplyDone('Neo', 'hello')
    expect(ctor).toHaveBeenCalledWith('Neo', expect.objectContaining({ body: 'hello' }))
  })

  it('does not notify when permission denied', () => {
    const ctor = vi.fn()
    vi.stubGlobal('Notification', Object.assign(ctor, { permission: 'denied' }))
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true })
    notifyReplyDone('t', 'b')
    expect(ctor).not.toHaveBeenCalled()
  })
})
