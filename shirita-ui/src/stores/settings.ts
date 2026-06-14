import { defineStore } from 'pinia'
import { ref } from 'vue'
import { getSettings, updateSettings, testProviderConnection, fetchProviderModels } from '../api/client'

export const useSettingsStore = defineStore('settings', () => {
  const data = ref<Record<string, unknown>>({})
  const loading = ref(false)
  const error = ref<string | null>(null)
  const testStatus = ref<'idle' | 'testing' | 'ok' | 'fail'>('idle')
  const testError = ref<string | null>(null)
  const models = ref<string[]>([])
  const modelsLoading = ref(false)
  const modelsError = ref<string | null>(null)

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

  // Use a hardcoded list (no live fetch) — for sources with no API key.
  function useFallbackModels(list: string[]) {
    modelsLoading.value = false; modelsError.value = null; models.value = [...list]
  }

  async function fetchModels() {
    modelsLoading.value = true; modelsError.value = null
    try {
      const result = await fetchProviderModels() as { data?: { id: string }[]; error?: unknown }
      if (result.data && Array.isArray(result.data)) {
        models.value = result.data.map((m) => m.id).filter(Boolean).sort()
        if (models.value.length === 0) modelsError.value = 'No models returned by this endpoint'
      } else if (result.error != null) {
        // upstream may return a string or a { message } error object
        const e = result.error as { message?: string }
        modelsError.value = typeof result.error === 'string' ? result.error : (e.message || JSON.stringify(result.error))
      } else {
        modelsError.value = 'Unexpected response from /models'
      }
    } catch (e) { modelsError.value = (e as Error).message }
    finally { modelsLoading.value = false }
  }

  return { data, loading, error, testStatus, testError, models, modelsLoading, modelsError, load, save, testConnection, fetchModels, useFallbackModels }
})
