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

  async function fetchModels() {
    modelsLoading.value = true; modelsError.value = null
    try {
      const result = await fetchProviderModels()
      if (result.data && Array.isArray(result.data)) {
        models.value = result.data.map((m: { id: string }) => m.id).sort()
      } else if (result.error) {
        modelsError.value = result.error
      }
    } catch (e) { modelsError.value = (e as Error).message }
    finally { modelsLoading.value = false }
  }

  return { data, loading, error, testStatus, testError, models, modelsLoading, modelsError, load, save, testConnection, fetchModels }
})
