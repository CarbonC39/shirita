// Fire a desktop notification only when the tab is backgrounded and the user
// has granted permission. Guarded so SSR/no-Notification environments no-op.
export function notifyReplyDone(title: string, body: string): void {
  if (typeof Notification === 'undefined') return
  if (Notification.permission !== 'granted') return
  if (document.visibilityState !== 'hidden') return
  try { new Notification(title, { body }) } catch { /* ignore */ }
}

export async function ensureNotifyPermission(): Promise<boolean> {
  if (typeof Notification === 'undefined') return false
  if (Notification.permission === 'granted') return true
  if (Notification.permission === 'denied') return false
  return (await Notification.requestPermission()) === 'granted'
}
