import { describe, it, expect, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import ChatDetailsDrawer from './ChatDetailsDrawer.vue'
import type { SessionPanel, VarDecl } from '../api/types'

const panels: SessionPanel[] = [
  { id: 'F', name: 'Status', html: '<b>hi</b>', css: '', caps: { write: true }, min_messages: 0 },
]
const schema: VarDecl[] = [
  { name: '$avatar', type: 'string', initial: '', scope: 'system' },
  { name: 'hp', type: 'number', initial: 100, scope: 'template' },
]

function mountDrawer(overrides: Partial<Record<string, unknown>> = {}) {
  return mount(ChatDetailsDrawer, {
    props: { open: true, panels, schema, values: { hp: 95 }, sessionId: 'session-1', ...overrides },
    attachTo: document.body,
  })
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('ChatDetailsDrawer', () => {
  it('renders nothing when closed', () => {
    const w = mountDrawer({ open: false })
    expect(document.querySelector('[data-test="details-dialog"]')).toBeNull()
    expect(w.find('[data-test="chat-details"]').exists()).toBe(false)
  })

  it('renders panels and grouped variables when open', () => {
    mountDrawer()
    const dialog = document.querySelector('[data-test="details-dialog"]')
    expect(dialog?.textContent).toContain('Status')
    expect(dialog?.querySelector('[data-test="var-system"]')?.textContent).toContain('$avatar')
    expect(dialog?.querySelector('[data-test="var-custom"]')?.textContent).toContain('hp')
    expect(dialog?.querySelector('[data-test="var-custom"]')?.textContent).toContain('95')
  })

  it('exposes dialog semantics and a labelled close button', () => {
    mountDrawer()
    const dialog = document.querySelector('[data-test="details-dialog"]') as HTMLElement
    expect(dialog.getAttribute('role')).toBe('dialog')
    expect(dialog.getAttribute('aria-modal')).toBe('true')
    expect(dialog.getAttribute('aria-label')).toBeTruthy()
    expect(document.querySelector('[data-test="details-close"]')?.getAttribute('aria-label')).toBeTruthy()
  })

  it('emits close on the close button', () => {
    const w = mountDrawer()
    document.querySelector('[data-test="details-close"]')?.dispatchEvent(new Event('click'))
    expect(w.emitted('close')).toBeTruthy()
  })

  it('emits close on Escape', () => {
    const w = mountDrawer()
    const dialog = document.querySelector('[data-test="details-dialog"]') as HTMLElement
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(w.emitted('close')).toBeTruthy()
  })

  it('emits close on backdrop click', () => {
    const w = mountDrawer()
    document.querySelector('[data-test="details-backdrop"]')?.dispatchEvent(new Event('click'))
    expect(w.emitted('close')).toBeTruthy()
  })

  it('emits panel-action with the panel and the action', () => {
    const w = mountDrawer()
    // PanelView renders its own action affordances; drive the contract through
    // the wired handler by simulating what PanelView would emit.
    const panelView = w.findComponent({ name: 'PanelView' })
    ;(panelView.vm as { $emit: (e: string, ...a: unknown[]) => void }).$emit('action', { kind: 'diff', op: 'sub', key: 'hp', value: '5' })
    const emitted = w.emitted('panel-action')
    expect(emitted).toBeTruthy()
    expect(emitted![0][0]).toEqual(panels[0])
    expect(emitted![0][1]).toMatchObject({ kind: 'diff', key: 'hp' })
  })

  it('restores focus to the close button on open', async () => {
    mountDrawer()
    await new Promise((r) => setTimeout(r, 0))
    const active = document.activeElement as HTMLElement | null
    expect(active?.getAttribute('data-test')).toBe('details-close')
  })
})
