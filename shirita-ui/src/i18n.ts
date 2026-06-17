import { createI18n } from 'vue-i18n'
import en from './locales/en'
import zhHans from './locales/zh-Hans'
import zhHant from './locales/zh-Hant'
import ja from './locales/ja'
import { resolveInitialLocale } from './locales/resolve'

export const i18n = createI18n({
  legacy: false,
  locale: resolveInitialLocale(),
  fallbackLocale: 'en',
  messages: { en, 'zh-Hans': zhHans, 'zh-Hant': zhHant, ja },
})
