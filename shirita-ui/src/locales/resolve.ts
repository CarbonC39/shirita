export type AppLocale = 'en' | 'zh-Hans' | 'zh-Hant' | 'ja'

export const SUPPORTED: AppLocale[] = ['en', 'zh-Hans', 'zh-Hant', 'ja']

/** Map any BCP-47 tag to a supported locale; null if unsupported. */
export function normalizeLocale(
  tag: string | null | undefined,
): AppLocale | null {
  if (!tag) return null
  const t = tag.toLowerCase()
  if (t.startsWith('zh')) {
    // Traditional: explicit `hant`, or the TW/HK/MO regions.
    if (
      t.includes('hant') ||
      /\b(tw|hk|mo)\b/.test(t) ||
      t.endsWith('-tw') ||
      t.endsWith('-hk') ||
      t.endsWith('-mo')
    ) {
      return 'zh-Hant'
    }
    return 'zh-Hans'
  }
  if (t.startsWith('ja')) return 'ja'
  if (t.startsWith('en')) return 'en'
  return null
}

/** Startup value: localStorage first, then browser language, then en. */
export function resolveInitialLocale(): AppLocale {
  const saved = normalizeLocale(localStorage.getItem('ui.locale'))
  if (saved) return saved
  const nav = typeof navigator !== 'undefined' ? navigator.language : null
  return normalizeLocale(nav) ?? 'en'
}
