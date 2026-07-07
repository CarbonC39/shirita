import { describe, it, expect, vi, beforeEach } from 'vitest'
import { notifyReplyDone, ensureNotifyPermission } from './notify'

describe('notifyReplyDone', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    // Ensure Tauri path is not taken in these tests
    vi.stubGlobal('__TAURI__', undefined)
  })

  it('does not notify when the tab is visible', async () => {
    const ctor = vi.fn()
    vi.stubGlobal('Notification', Object.assign(ctor, { permission: 'granted' }))
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true })
    await notifyReplyDone('t', 'b')
    expect(ctor).not.toHaveBeenCalled()
  })

  it('notifies when hidden and permitted', async () => {
    const ctor = vi.fn()
    vi.stubGlobal('Notification', Object.assign(ctor, { permission: 'granted' }))
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true })
    await notifyReplyDone('Neo', 'hello')
    expect(ctor).toHaveBeenCalledWith('Neo', expect.objectContaining({ body: 'hello' }))
  })

  it('does not notify when permission denied', async () => {
    const ctor = vi.fn()
    vi.stubGlobal('Notification', Object.assign(ctor, { permission: 'denied' }))
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true })
    await notifyReplyDone('t', 'b')
    expect(ctor).not.toHaveBeenCalled()
  })
})

describe('ensureNotifyPermission', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    // Ensure Tauri path is not taken in these tests
    vi.stubGlobal('__TAURI__', undefined)
  })

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
