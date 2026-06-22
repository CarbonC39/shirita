import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import * as client from '../api/client'
import { i18n } from '../i18n'
import SettingsView from './SettingsView.vue'

describe('SettingsView language switcher', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
    i18n.global.locale.value = 'en'
    // onMounted: settings.load() -> getSettings, then listDefinitions.
    // Empty settings means no API key, so no live model fetch fires.
    vi.spyOn(client, 'getSettings').mockResolvedValue({})
    vi.spyOn(client, 'listDefinitions').mockResolvedValue([])
  })

  it('renders a 4-option locale switcher and switches locale on change', async () => {
    const wrapper = mount(SettingsView)
    await flushPromises()

    const switcher = wrapper.get('[data-test="locale-switcher"]')
    const options = switcher.findAll('option')
    expect(options.map((o) => o.text())).toEqual([
      'English',
      '简体中文',
      '繁體中文',
      '日本語',
    ])

    await switcher.setValue('zh-Hant') // 繁體中文
    expect(i18n.global.locale.value).toBe('zh-Hant')
    expect(localStorage.getItem('ui.locale')).toBe('zh-Hant')
  })
})

// Regression test: a manual tester reported the "Notify on reply (background
// tab)" toggle as unclickable ("点不开"). Root cause was that the click DID
// register, but ensureNotifyPermission() silently resolves to a non-granted
// result whenever the Notification API is unsupported or permission is
// denied, and the old handler reverted the toggle with zero feedback — so
// every click looked like a dead no-op. The fix surfaces an explanatory
// error message instead of silently snapping back.
describe('SettingsView notify toggle', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
    i18n.global.locale.value = 'en'
    vi.spyOn(client, 'getSettings').mockResolvedValue({})
    vi.spyOn(client, 'listDefinitions').mockResolvedValue([])
  })

  function findToggle(wrapper: ReturnType<typeof mount>) {
    const label = wrapper.findAll('span').find((s) => s.text().includes('Notify on reply'))!
    const row = label.element.closest('div')!
    return wrapper.findAll('[data-test="toggle"]').find((t) => t.element === row.querySelector('[data-test="toggle"]'))!
  }

  it('explains why the toggle reverts when notifications are unsupported', async () => {
    vi.stubGlobal('Notification', undefined)
    const wrapper = mount(SettingsView)
    await flushPromises()

    const toggle = findToggle(wrapper)
    expect(toggle.attributes('aria-checked')).toBe('false')

    await toggle.trigger('click')
    await flushPromises()

    expect(toggle.attributes('aria-checked')).toBe('false')
    expect(wrapper.text()).toContain('Desktop notifications are not supported')
  })

  it('explains why the toggle reverts when permission was denied', async () => {
    vi.stubGlobal('Notification', { permission: 'denied' })
    const wrapper = mount(SettingsView)
    await flushPromises()

    const toggle = findToggle(wrapper)
    await toggle.trigger('click')
    await flushPromises()

    expect(toggle.attributes('aria-checked')).toBe('false')
    expect(wrapper.text()).toContain('Notification permission was denied')
  })

  it('stays on when permission is granted', async () => {
    vi.stubGlobal('Notification', { permission: 'granted' })
    const wrapper = mount(SettingsView)
    await flushPromises()

    const toggle = findToggle(wrapper)
    await toggle.trigger('click')
    await flushPromises()

    expect(toggle.attributes('aria-checked')).toBe('true')
  })
})
