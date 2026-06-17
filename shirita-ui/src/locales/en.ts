// Source of truth. Every other locale's key set must match this exactly
// (enforced by parity.test.ts). en is also the i18n fallbackLocale.
// NB: NO `as const` — the schema's leaf type must be `string` so the other
// locales can hold their own strings. `as const` would pin values to literals
// (e.g. save: 'Save') and reject translations at compile time.
const en = {
  common: {
    save: 'Save',
    cancel: 'Cancel',
    delete: 'Delete',
    duplicate: 'Duplicate',
    add: 'Add',
    close: 'Close',
    import: 'Import',
    export: 'Export',
    // Plural example: en distinguishes 1 vs many; zh/ja use a single form.
    tokensEstimate: '~{count} token | ~{count} tokens',
  },
  shell: {
    chats: 'Chats',
    new: 'New',
    book: 'Book',
    settings: 'Settings',
  },
  settings: {
    title: 'Settings',
    language: 'Language',
  },
}

export default en

export type MessageSchema = typeof en
