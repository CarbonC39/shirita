import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Definition, Template, DefType, Pack } from '../api/types'
import { listDefinitions, listTemplates, listTypes, listPacks, createType as apiCreateType, deleteType as apiDeleteType } from '../api/client'

export const useLibraryStore = defineStore('library', () => {
  const definitions = ref<Definition[]>([])
  const templates = ref<Template[]>([])
  const containerTypes = ref<DefType[]>([])
  const packs = ref<Pack[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function loadDefinitions() {
    try { definitions.value = await listDefinitions() } catch (e) { error.value = (e as Error).message }
  }

  async function loadTemplates() {
    try { templates.value = await listTemplates() } catch (e) { error.value = (e as Error).message }
  }

  async function loadTypes() {
    // builtin types first, then custom ones (by sort) — keeps Character/User/World
    // ahead of user-added types everywhere they're listed.
    try {
      const types = await listTypes()
      types.sort((a, b) => Number(b.builtin) - Number(a.builtin) || a.sort - b.sort)
      containerTypes.value = types
    } catch (e) { error.value = (e as Error).message }
  }

  async function loadPacks() {
    try { packs.value = await listPacks() } catch (e) { error.value = (e as Error).message }
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
    try { await Promise.all([loadDefinitions(), loadTemplates(), loadTypes(), loadPacks()]) } catch (e) { error.value = (e as Error).message }
    finally { loading.value = false }
  }

  return { definitions, templates, containerTypes, packs, loading, error, loadDefinitions, loadTemplates, loadTypes, loadPacks, addType, removeType, loadAll }
})
