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
