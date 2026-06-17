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
    saved: 'Saved',
    saving: 'Saving…',
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
    crumb: 'Prompt',
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
  composer: {
    attach: 'Attach',
    placeholder: 'Type a message…',
  },
  book: {
    localHeading: 'This conversation',
    localChangedLabel: 'Changed here',
    syncToGlobal: 'Sync to global',
    revertToGlobal: 'Revert to global',
    followsTemplate: 'This conversation follows its template.',
    customizeLocally: 'Customize locally',
    variablesThisChat: 'Variables (this chat)',
    variables: 'Variables',
    selectTemplate: 'Select a template…',
    newTemplate: '+ New template',
    onConflict: 'On conflict',
    conflictSkip: 'Skip',
    conflictOverwrite: 'Overwrite',
    conflictDuplicate: 'Duplicate',
    importTitle: 'Import card / world / template (.png, .json)',
    exportTemplateTitle: 'Export template (enabled part)',
    importSummary:
      'Imported: {created} created, {skipped} skipped, {overwritten} overwritten.',
    templateNamePlaceholder: 'Template name',
    draftHint: 'Save this template to start building its node tree.',
    exportDefinition: 'Export this definition',
    promoteConfirm: 'Sync this definition to the global library?',
    deleteType: 'Delete type "{id}"?',
    deleteTypeInUse:
      'Delete type "{id}"? Definitions using it will keep the type id but it won\'t be selectable.',
  },
  settings: {
    title: 'Settings',
    language: 'Language',
    provider: 'Provider',
    source: 'Source',
    baseUrl: 'Base URL',
    apiKey: 'API Key',
    model: 'Model',
    selectModel: '— select model —',
    modelsHint: 'Add a Base URL and API key to load models.',
    fetching: 'Fetching…',
    modelsLive: '{count} model | {count} models',
    modelsLiveTitle: 'Fetched live from your provider',
    modelsCommon: '{count} common',
    modelsCommonTitle:
      "Built-in suggestions — add an API key to fetch your provider's models",
    streamResponses: 'Stream responses',
    testing: 'Testing…',
    testConnection: 'Test connection',
    generation: 'Generation',
    temperature: 'Temperature',
    topP: 'Top P',
    frequencyPenalty: 'Frequency penalty',
    presencePenalty: 'Presence penalty',
    maxResponseTokens: 'Max response tokens',
    appearance: 'Appearance',
    messageStyle: 'Message style',
    styleBubble: 'Bubble',
    styleFlat: 'Flat',
    theme: 'Theme',
    themeLight: 'Light',
    themeDark: 'Dark',
    themeSystem: 'System',
    background: 'Background',
    customCss: 'Custom CSS',
    fullscreen: 'Fullscreen',
    regex: 'Regex',
    addRule: '+ Add rule',
    about: 'About',
    aboutText: 'Shirita — a SillyTavern alternative.',
    exportAll: 'Export all data',
    importAll: 'Import all data',
  },
}

export default en

export type MessageSchema = typeof en
