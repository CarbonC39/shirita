import { describe, it, expect, vi, beforeEach } from 'vitest'
import { notifyReplyDone, ensureNotifyPermission } from './notify'

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

describe('ensureNotifyPermission', () => {
  beforeEach(() => { vi.restoreAllMocks() })

  it('reports unsupported when the Notification API is unavailable', async () => {
    vi.stubGlobal('Notification', undefined)
    await expect(ensureNotifyPermission()).resolves.toBe('unsupported')
  })

  it('reports granted immediately when already granted', async () => {
    vi.stubGlobal('Notification', { permission: 'granted' })
    await expect(ensureNotifyPermission()).resolves.toBe('granted')
  })

  it('reports denied without prompting when already denied', async () => {
    const requestPermission = vi.fn()
    vi.stubGlobal('Notification', { permission: 'denied', requestPermission })
    await expect(ensureNotifyPermission()).resolves.toBe('denied')
    expect(requestPermission).not.toHaveBeenCalled()
  })

  it('prompts when permission is default and reports the user choice', async () => {
    const requestPermission = vi.fn().mockResolvedValue('granted')
    vi.stubGlobal('Notification', { permission: 'default', requestPermission })
    await expect(ensureNotifyPermission()).resolves.toBe('granted')

    const requestPermissionDenied = vi.fn().mockResolvedValue('denied')
    vi.stubGlobal('Notification', { permission: 'default', requestPermission: requestPermissionDenied })
    await expect(ensureNotifyPermission()).resolves.toBe('denied')
  })
})
