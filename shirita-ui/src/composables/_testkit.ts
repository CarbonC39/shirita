import { mount } from '@vue/test-utils'
import { defineComponent } from 'vue'

/**
 * Run a composable inside a real component instance so composition hooks that
 * depend on inject() (e.g. vue-i18n's `useI18n`) resolve, then hand its public
 * API back for assertion. i18n is installed globally via test/setup.ts, so the
 * host picks it up automatically; pinia is provided per-test where needed.
 */
export function invokeComposable<T>(setup: () => T): T {
  let api!: T
  const Host = defineComponent({
    name: 'ComposableHost',
    setup() {
      api = setup()
      return {}
    },
    render() {
      return null
    },
  })
  mount(Host)
  return api
}
