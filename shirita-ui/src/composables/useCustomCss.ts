import { watch } from 'vue'
import { useSettingsStore } from '../stores/settings'

const STYLE_ID = 'user-custom-css'
const CACHE_KEY = 'ui.customCss'

// Idempotently set the custom-CSS <style> text and refresh the cache. Exported
// for direct use at boot (before mount) and for tests.
export function applyCustomCss(css: string): void {
  let el = document.getElementById(STYLE_ID) as HTMLStyleElement | null
  if (!el) {
    el = document.createElement('style')
    el.id = STYLE_ID
    document.head.appendChild(el)
  }
  el.textContent = css
  try { localStorage.setItem(CACHE_KEY, css) } catch { /* ignore */ }
}

// Paint immediately from the localStorage cache (call before app.mount to avoid
// FOUC), then reconcile with the server value once settings load.
export function bootCustomCss(): void {
  applyCustomCss(localStorage.getItem(CACHE_KEY) || '')
}

export function useCustomCss(): void {
  const settings = useSettingsStore()
  watch(
    () => settings.data.custom_css,
    (css) => { if (typeof css === 'string') applyCustomCss(css) },
    { immediate: true },
  )
}
