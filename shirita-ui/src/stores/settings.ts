import { defineStore } from 'pinia'
import { ref } from 'vue'
import { getSettings, updateSettings, testProviderConnection } from '../api/client'

export const useSettingsStore = defineStore('settings', () => {
  const data = ref<Record<string, unknown>>({})
  const loading = ref(false)
  const error = ref<string | null>(null)
  const testStatus = ref<'idle' | 'testing' | 'ok' | 'fail'>('idle')
  const testError = ref<string | null>(null)

  async function load() {
    loading.value = true; error.value = null
    try { data.value = await getSettings() } catch (e) { error.value = (e as Error).message }
    finally { loading.value = false }
  }

  async function save(patch: Record<string, unknown>) {
    try { await updateSettings(patch); data.value = { ...data.value, ...patch } } catch (e) { error.value = (e as Error).message; throw e }
  }

  async function testConnection() {
    testStatus.value = 'testing'; testError.value = null
    try { const result = await testProviderConnection(); testStatus.value = result.ok ? 'ok' : 'fail'; testError.value = result.error || null }
    catch (e) { testStatus.value = 'fail'; testError.value = (e as Error).message }
  }

  return { data, loading, error, testStatus, testError, load, save, testConnection }
})
