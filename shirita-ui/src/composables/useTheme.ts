import { watchEffect } from 'vue'
import { useUiStore } from '../stores/ui'

// Reflect the chosen theme onto <html> as a `.dark` class so the dark token
// overrides in styles.css take effect. 'system' follows the OS preference live;
// 'light'/'dark' force it. Call once from the app root.
export function useTheme() {
  const ui = useUiStore()
  const media = window.matchMedia('(prefers-color-scheme: dark)')

  function apply() {
    const dark = ui.theme === 'dark' || (ui.theme === 'system' && media.matches)
    document.documentElement.classList.toggle('dark', dark)
  }

  watchEffect(apply) // re-applies whenever ui.theme changes (and runs immediately)
  media.addEventListener('change', apply) // follow OS changes while on 'system'
}
