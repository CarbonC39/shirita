// Compact "time ago" for the home conversation list. Returns short forms like
// "now", "5m", "3h", "2d", then falls back to a calendar date for older items.
export function relativeTime(iso?: string): string {
  if (!iso) return ''
  const then = new Date(iso).getTime()
  if (Number.isNaN(then)) return ''
  const secs = Math.max(0, (Date.now() - then) / 1000)
  if (secs < 45) return 'now'
  const mins = secs / 60
  if (mins < 60) return `${Math.round(mins)}m`
  const hours = mins / 60
  if (hours < 24) return `${Math.round(hours)}h`
  const days = hours / 24
  if (days < 7) return `${Math.round(days)}d`
  return new Date(then).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}
