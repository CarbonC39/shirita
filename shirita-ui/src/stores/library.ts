import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Definition, Template, DefType } from '../api/types'
import { listDefinitions, listTemplates, listTypes, createType as apiCreateType, deleteType as apiDeleteType } from '../api/client'

export const useLibraryStore = defineStore('library', () => {
  const definitions = ref<Definition[]>([])
  const templates = ref<Template[]>([])
  const containerTypes = ref<DefType[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function loadDefinitions() {
    try { definitions.value = await listDefinitions() } catch (e) { error.value = (e as Error).message }
  }

  async function loadTemplates() {
    try { templates.value = await listTemplates() } catch (e) { error.value = (e as Error).message }
  }

  async function loadTypes() {
    try { containerTypes.value = await listTypes() } catch (e) { error.value = (e as Error).message }
  }

  async function addType(id: string, label: string) {
    const created = await apiCreateType({ id, label, sort: containerTypes.value.length })
    containerTypes.value = [...containerTypes.value, created]
    return created
  }

  async function removeType(id: string) {
    await apiDeleteType(id)
    containerTypes.value = containerTypes.value.filter((t) => t.id !== id)
  }

  async function loadAll() {
    loading.value = true; error.value = null
    try { await Promise.all([loadDefinitions(), loadTemplates(), loadTypes()]) } catch (e) { error.value = (e as Error).message }
    finally { loading.value = false }
  }

  return { definitions, templates, containerTypes, loading, error, loadDefinitions, loadTemplates, loadTypes, addType, removeType, loadAll }
})
