import { describe, it, expect, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { i18n } from './i18n'

const Probe = defineComponent({
  setup() {
    return () => h('span', i18n.global.t('shell.settings'))
  },
})

describe('i18n locale switching', () => {
  afterEach(() => {
    i18n.global.locale.value = 'en'
  })

  it('re-renders $t output when the global locale changes', async () => {
    const wrapper = mount(Probe) // global i18n plugin comes from setup.ts
    expect(wrapper.text()).toBe('Settings')
    i18n.global.locale.value = 'zh-Hant'
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toBe('設定')
    i18n.global.locale.value = 'ja'
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toBe('設定')
  })
})
