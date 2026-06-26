import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import * as client from '../api/client'
import { i18n } from '../i18n'
import SettingsView from './SettingsView.vue'
import SliderControl from '../components/SliderControl.vue'

function mockEmptySettings() {
  vi.spyOn(client, 'getSettings').mockResolvedValue({})
  vi.spyOn(client, 'listDefinitions').mockResolvedValue([])
  vi.spyOn(client, 'getRegexScopes').mockResolvedValue([])
}

describe('SettingsView generation defaults', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
    i18n.global.locale.value = 'en'
  })

  it('defaults temperature to 1', async () => {
    mockEmptySettings()
    const w = mount(SettingsView)
    await flushPromises()
    const temp = w.findAllComponents(SliderControl).find((s) => s.props('label') === 'Temperature')!
    expect(temp.props('modelValue')).toBe(1)
  })
})
