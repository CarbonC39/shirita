import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Session } from '../api/types'
import { listSessions, deleteSession, duplicateSession } from '../api/client'

export const useSessionsStore = defineStore('sessions', () => {
  const items = ref<Session[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function load() {
    loading.value = true
    error.value = null
    try {
      items.value = await listSessions()
    } catch (e) {
      error.value = (e as Error).message
    } finally {
      loading.value = false
    }
  }

  async function remove(id: string) {
    try { await deleteSession(id); items.value = items.value.filter((s) => s.id !== id) }
    catch (e) { error.value = (e as Error).message }
  }

  async function duplicate(id: string) {
    try { await duplicateSession(id); await load() }
    catch (e) { error.value = (e as Error).message }
  }

  return { items, loading, error, load, remove, duplicate }
})
