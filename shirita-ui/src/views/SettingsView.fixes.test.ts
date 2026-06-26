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

describe('SettingsView max output', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
    i18n.global.locale.value = 'en'
  })

  it('shows a max-tokens slider defaulting to 8192 with Unlimited off', async () => {
    mockEmptySettings()
    const w = mount(SettingsView)
    await flushPromises()
    const max = w.findAllComponents(SliderControl).find((s) => s.props('label') === 'Max response tokens')
    expect(max).toBeTruthy()
    expect(max!.props('modelValue')).toBe(8192)
    expect(w.get('[data-test="max-unlimited"] [data-test="toggle"]').attributes('aria-checked')).toBe('false')
  })

  it('treats stored 0 as Unlimited and hides the slider', async () => {
    vi.spyOn(client, 'getSettings').mockResolvedValue({ provider_max_tokens: 0 })
    vi.spyOn(client, 'listDefinitions').mockResolvedValue([])
    vi.spyOn(client, 'getRegexScopes').mockResolvedValue([])
    const w = mount(SettingsView)
    await flushPromises()
    expect(w.get('[data-test="max-unlimited"] [data-test="toggle"]').attributes('aria-checked')).toBe('true')
    expect(w.findAllComponents(SliderControl).some((s) => s.props('label') === 'Max response tokens')).toBe(false)
  })

  it('toggling Unlimited on stores 0 and hides the slider', async () => {
    mockEmptySettings()
    const w = mount(SettingsView)
    await flushPromises()
    await w.get('[data-test="max-unlimited"] [data-test="toggle"]').trigger('click')
    await flushPromises()
    expect(w.get('[data-test="max-unlimited"] [data-test="toggle"]').attributes('aria-checked')).toBe('true')
    expect(w.findAllComponents(SliderControl).some((s) => s.props('label') === 'Max response tokens')).toBe(false)
  })
})
