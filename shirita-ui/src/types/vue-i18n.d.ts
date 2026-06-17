import type { MessageSchema } from '../locales/en'

declare module 'vue-i18n' {
  // Constrain t / $t keys to the en structure: completion, spell-check,
  // and a compile error on a missing/typo'd key.
  export interface DefineLocaleMessage extends MessageSchema {}
}

export {}
