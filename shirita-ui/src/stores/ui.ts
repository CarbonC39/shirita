import { defineStore } from 'pinia'
import { i18n } from '../i18n'
import { resolveInitialLocale, type AppLocale } from '../locales/resolve'

export type MessageStyle = 'bubble' | 'flat'
export type Theme = 'light' | 'dark' | 'system'

export const useUiStore = defineStore('ui', {
  state: () => ({
    messageStyle:
      (localStorage.getItem('ui.messageStyle') as MessageStyle) || 'flat',
    theme: (localStorage.getItem('ui.theme') as Theme) || 'system',
    // App-wide background image (relative asset path). Cached locally so it
    // paints immediately on load; also mirrored to server settings.
    background: localStorage.getItem('ui.background') || '',
    // The conversation you're "in" — drives the Book page's local section and
    // the shell's Chat tab. Not persisted; set as you navigate.
    activeChatId: null as string | null,
    // UI language. Persisted to localStorage (key `ui.locale`); resolved on
    // boot from localStorage -> navigator.language -> en. Mirrors `theme`.
    locale: resolveInitialLocale() as AppLocale,
    // Center content column width (px). Cached locally, mirrored to server setting.
    contentWidth: Number(localStorage.getItem('ui.contentWidth')) || 760,
  }),
  actions: {
    setActiveChatId(id: string | null) {
      this.activeChatId = id
    },
    setMessageStyle(style: MessageStyle) {
      this.messageStyle = style
      localStorage.setItem('ui.messageStyle', style)
    },
    setTheme(theme: Theme) {
      this.theme = theme
      localStorage.setItem('ui.theme', theme)
    },
    setBackground(path: string) {
      this.background = path
      if (path) localStorage.setItem('ui.background', path)
      else localStorage.removeItem('ui.background')
    },
    setContentWidth(px: number) {
      this.contentWidth = px
      localStorage.setItem('ui.contentWidth', String(px))
    },
    setLocale(locale: AppLocale) {
      this.locale = locale
      localStorage.setItem('ui.locale', locale)
      i18n.global.locale.value = locale
    },
  },
})
