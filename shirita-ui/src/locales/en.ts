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
    loading: 'Loading…',
    // Plural: en distinguishes 1 vs many; zh/ja use a single form. `{tokens}`
    // is the pre-formatted display value; pass the raw number as the plural
    // choice: t('common.tokensEstimate', { tokens: formatTokens(n) }, n).
    tokensEstimate: '~{tokens} token | ~{tokens} tokens',
  },
  shell: {
    chats: 'Chats',
    new: 'New',
    book: 'Book',
    settings: 'Settings',
  },
  home: {
    empty: 'No conversations yet.',
    importTitle: 'Import a conversation',
    newChatAria: 'New chat',
    done: 'Done',
    reorderDelete: 'Reorder & delete',
    deleteConfirm: 'Delete this conversation and all its messages?',
  },
  newChat: {
    namePlaceholder: 'Name',
    next: 'Next',
    skip: 'Skip',
  },
  prompt: {
    untitled: 'Untitled',
    subtitle: 'Choose a prompt template and configure the tree.',
    template: 'Template',
    none: 'None (start empty)',
    creating: 'Creating…',
    create: 'Create conversation',
    deleteContainerConfirm:
      'Delete this container and its {count} item? | Delete this container and its {count} items?',
  },
  chat: {
    back: 'Back',
    title: 'Chat',
  },
  settings: {
    title: 'Settings',
    language: 'Language',
  },
}

export default en

export type MessageSchema = typeof en
