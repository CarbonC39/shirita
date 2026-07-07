// ---------------------------------------------------------------------------
// Desktop notifications: Web Notification API (browsers) or Tauri native
// notifications (desktop app). Auto-detects the runtime at call time.
// ---------------------------------------------------------------------------

let tauriNotifyModule: typeof import('@tauri-apps/plugin-notification') | null = null

async function getTauriNotify() {
  if (tauriNotifyModule) return tauriNotifyModule
  if (typeof window !== 'undefined' && (window as any).__TAURI__) {
    try {
      tauriNotifyModule = await import('@tauri-apps/plugin-notification')
    } catch { /* package not installed — gracefully fall through to Web API */ }
  }
  return tauriNotifyModule
}

// Fire a desktop notification only when the tab is backgrounded and the user
// has granted permission. In Tauri the plugin handles the OS notification
// directly; we still guard on the frontend side for consistency.
export async function notifyReplyDone(title: string, body: string): Promise<void> {
  const tauri = await getTauriNotify()
  if (tauri) {
    // Tauri: send the OS notification (the plugin does its own permission check)
    if (document.visibilityState !== 'hidden') return
    const ok = await tauri.isPermissionGranted()
    if (!ok) return
    tauri.sendNotification({ title, body })
    return
  }
  // Web platform fallback
  if (typeof Notification === 'undefined') return
  if (Notification.permission !== 'granted') return
  if (document.visibilityState !== 'hidden') return
  try { new Notification(title, { body }) } catch { /* ignore */ }
}

// Result distinguishes *why* permission wasn't granted so the UI can explain
// itself instead of silently reverting the toggle (looked like a dead click).
export type NotifyPermissionResult = 'granted' | 'denied' | 'unsupported'

export async function ensureNotifyPermission(): Promise<NotifyPermissionResult> {
  const tauri = await getTauriNotify()
  if (tauri) {
    const ok = await tauri.isPermissionGranted()
    if (ok) return 'granted'
    const result = await tauri.requestPermission()
    return result === 'granted' ? 'granted' : 'denied'
  }
  if (typeof Notification === 'undefined') return 'unsupported'
  if (Notification.permission === 'granted') return 'granted'
  if (Notification.permission === 'denied') return 'denied'
  const result = await Notification.requestPermission()
  return result === 'granted' ? 'granted' : 'denied'
}
