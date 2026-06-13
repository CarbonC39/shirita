import { defineStore } from 'pinia'

export type MessageStyle = 'bubble' | 'flat'
export type Theme = 'light' | 'dark' | 'system'

export const useUiStore = defineStore('ui', {
  state: () => ({
    messageStyle:
      (localStorage.getItem('ui.messageStyle') as MessageStyle) || 'bubble',
    theme: (localStorage.getItem('ui.theme') as Theme) || 'system',
  }),
  actions: {
    setMessageStyle(style: MessageStyle) {
      this.messageStyle = style
      localStorage.setItem('ui.messageStyle', style)
    },
    setTheme(theme: Theme) {
      this.theme = theme
      localStorage.setItem('ui.theme', theme)
    },
  },
})
