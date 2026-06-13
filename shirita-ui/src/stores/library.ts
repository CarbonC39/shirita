import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Definition, Template } from '../api/types'
import { listDefinitions, listTemplates } from '../api/client'

export const useLibraryStore = defineStore('library', () => {
  const definitions = ref<Definition[]>([])
  const templates = ref<Template[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function loadDefinitions() {
    try { definitions.value = await listDefinitions() } catch (e) { error.value = (e as Error).message }
  }

  async function loadTemplates() {
    try { templates.value = await listTemplates() } catch (e) { error.value = (e as Error).message }
  }

  async function loadAll() {
    loading.value = true; error.value = null
    try { await Promise.all([loadDefinitions(), loadTemplates()]) } catch (e) { error.value = (e as Error).message }
    finally { loading.value = false }
  }

  return { definitions, templates, loading, error, loadDefinitions, loadTemplates, loadAll }
})
