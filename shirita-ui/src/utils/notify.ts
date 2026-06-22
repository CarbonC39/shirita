// Fire a desktop notification only when the tab is backgrounded and the user
// has granted permission. Guarded so SSR/no-Notification environments no-op.
export function notifyReplyDone(title: string, body: string): void {
  if (typeof Notification === 'undefined') return
  if (Notification.permission !== 'granted') return
  if (document.visibilityState !== 'hidden') return
  try { new Notification(title, { body }) } catch { /* ignore */ }
}

// Result distinguishes *why* permission wasn't granted so the UI can explain
// itself instead of silently reverting the toggle (looked like a dead click).
export type NotifyPermissionResult = 'granted' | 'denied' | 'unsupported'

export async function ensureNotifyPermission(): Promise<NotifyPermissionResult> {
  if (typeof Notification === 'undefined') return 'unsupported'
  if (Notification.permission === 'granted') return 'granted'
  if (Notification.permission === 'denied') return 'denied'
  const result = await Notification.requestPermission()
  return result === 'granted' ? 'granted' : 'denied'
}
