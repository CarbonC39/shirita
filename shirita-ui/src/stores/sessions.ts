import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { Session } from '../api/types'
import { listSessions } from '../api/client'

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

  return { items, loading, error, load }
})
