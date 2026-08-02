import { describe, it, expect, afterEach } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import MessageActionsSheet from './MessageActionsSheet.vue'
import type { Message } from '../api/types'

function makeMsg(overrides: Partial<Message> = {}): Message {
  return {
    id: 'm1', session_id: 's1', parent_id: null, role: 'user',
    raw_content: 'Hello world', display_content: null, is_hidden: false, is_anchor: false,
    attachments: [], snapshot_state: {}, created_at: '2025-01-01T00:00:00Z',
    ...overrides,
  }
}

afterEach(() => {
  document.body.innerHTML = ''
})

function mountSheet(props: { message: Message; siblingIndex?: number; siblingCount?: number }) {
  return mount(MessageActionsSheet, { props, attachTo: document.body })
}

describe('MessageActionsSheet', () => {
  it('is a teleported viewport-level surface, not a child of message content', async () => {
    const w = mountSheet({ message: makeMsg({ role: 'assistant' }) })
    const sheet = document.querySelector('[data-test="message-action-sheet"]')
    expect(sheet).not.toBeNull()
    // Direct child of <body>, not nested under the mounted wrapper.
    expect(sheet?.parentElement).toBe(document.body)
    w.unmount()
  })

  it('exposes the assistant action set and an accessible name', () => {
    mountSheet({ message: makeMsg({ role: 'assistant' }) })
    const dialog = document.querySelector('[data-test="action-dialog"]')
    expect(dialog?.getAttribute('role')).toBe('dialog')
    expect(dialog?.getAttribute('aria-modal')).toBe('true')
    expect(dialog?.getAttribute('aria-label')).toBeTruthy()
    expect(document.querySelector('[data-test="regenerate-btn"]')).not.toBeNull()
    expect(document.querySelector('[data-test="fork-btn"]')).not.toBeNull()
    expect(document.querySelector('[data-test="copy-btn"]')).not.toBeNull()
    expect(document.querySelector('[data-test="edit-btn"]')).not.toBeNull()
    expect(document.querySelector('[data-test="hide-btn"]')).not.toBeNull()
    expect(document.querySelector('[data-test="delete-btn"]')).not.toBeNull()
  })

  it('omits regenerate/fork for user messages', () => {
    mountSheet({ message: makeMsg({ role: 'user' }) })
    expect(document.querySelector('[data-test="regenerate-btn"]')).toBeNull()
    expect(document.querySelector('[data-test="fork-btn"]')).toBeNull()
    expect(document.querySelector('[data-test="copy-btn"]')).not.toBeNull()
  })

  it('emits action and close when an action is chosen', async () => {
    const w = mountSheet({ message: makeMsg({ role: 'user' }) })
    ;(document.querySelector('[data-test="delete-btn"]') as HTMLElement).click()
    expect(w.emitted('action')![0]).toEqual(['delete'])
    expect(w.emitted('close')).toBeTruthy()
    w.unmount()
  })

  it('keeps swipe disabled boundaries and emits swipe with the delta', async () => {
    const msg = makeMsg({ id: 'b2', parent_id: 'a', role: 'assistant', created_at: '2' })
    const w = mountSheet({ message: msg, siblingIndex: 0, siblingCount: 2 })
    const prev = document.querySelector('[data-test="action-swipe-prev"]') as HTMLButtonElement
    expect(prev.disabled).toBe(true)
    const next = document.querySelector('[data-test="action-swipe-next"]') as HTMLButtonElement
    expect(next.disabled).toBe(false)
    next.click()
    expect(w.emitted('swipe')![0]).toEqual([1])
    expect(w.emitted('close')).toBeTruthy()
    w.unmount()
  })

  it('closes on Escape without firing an action', async () => {
    const w = mountSheet({ message: makeMsg({ role: 'user' }) })
    const dialog = document.querySelector('[data-test="action-dialog"]') as HTMLElement
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(w.emitted('close')).toBeTruthy()
    expect(w.emitted('action')).toBeFalsy()
    w.unmount()
  })

  it('closes on backdrop and on the close button without firing an action', async () => {
    const w = mountSheet({ message: makeMsg({ role: 'user' }) })
    ;(document.querySelector('[data-test="action-backdrop"]') as HTMLElement).click()
    expect(w.emitted('close')).toBeTruthy()
    expect(w.emitted('action')).toBeFalsy()
    w.unmount()
    document.body.innerHTML = ''

    const w2 = mountSheet({ message: makeMsg({ role: 'user' }) })
    ;(document.querySelector('[data-test="action-close"]') as HTMLElement).click()
    expect(w2.emitted('close')).toBeTruthy()
    expect(w2.emitted('action')).toBeFalsy()
    w2.unmount()
  })
})
