// Single source of truth for message actions. Both the desktop inline row and
// the mobile viewport-level action sheet render from this map, so the two
// surfaces cannot drift. Swipe (previous/next variation) is a composite control
// (arrows + count) and is handled separately by the components.

import { Copy, RefreshCw, GitFork, Pencil, Eye, EyeOff, Trash2 } from 'lucide-vue-next'
import type { Component } from 'vue'

export type MessageActionKey =
  | 'copy'
  | 'regenerate'
  | 'fork'
  | 'edit'
  | 'toggle-hidden'
  | 'delete'

export interface MessageAction {
  key: MessageActionKey
  /** i18n key for the action's label. `toggle-hidden` swaps between hide/unhide. */
  labelKey: string
  icon: Component
  /** Only assistant messages expose this action. */
  assistantOnly: boolean
  /** Stable `data-test` value used by tests and custom CSS. */
  testId: string
}

export const messageActions: MessageAction[] = [
  { key: 'regenerate', labelKey: 'chat.regenerate', icon: RefreshCw, assistantOnly: true, testId: 'regenerate-btn' },
  { key: 'fork', labelKey: 'chat.fork', icon: GitFork, assistantOnly: true, testId: 'fork-btn' },
  { key: 'copy', labelKey: 'chat.copy', icon: Copy, assistantOnly: false, testId: 'copy-btn' },
  { key: 'edit', labelKey: 'chat.edit', icon: Pencil, assistantOnly: false, testId: 'edit-btn' },
  { key: 'toggle-hidden', labelKey: 'chat.hide', icon: Eye, assistantOnly: false, testId: 'hide-btn' },
  { key: 'delete', labelKey: 'chat.delete', icon: Trash2, assistantOnly: false, testId: 'delete-btn' },
]

/** Actions valid for a given message role. */
export function actionsFor(role: string): MessageAction[] {
  return messageActions.filter((a) => !a.assistantOnly || role === 'assistant')
}
