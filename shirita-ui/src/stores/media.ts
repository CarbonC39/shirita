import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listAssets, uploadAsset, renameAsset, deleteAsset, type Asset } from '../api/client'

// The shared media library — uploaded images with editable names, used by both
// avatars and backgrounds. One store keeps every picker in sync.
export const useMediaStore = defineStore('media', () => {
  const assets = ref<Asset[]>([])
  const loaded = ref(false)
  const error = ref<string | null>(null)

  async function load(force = false) {
    if (loaded.value && !force) return
    try { assets.value = await listAssets(); loaded.value = true }
    catch (e) { error.value = (e as Error).message }
  }

  async function upload(file: File): Promise<Asset | null> {
    try { const a = await uploadAsset(file); assets.value = [a, ...assets.value]; return a }
    catch (e) { error.value = (e as Error).message; return null }
  }

  async function rename(id: string, name: string) {
    const a = assets.value.find((x) => x.id === id)
    const prev = a?.name
    if (a) a.name = name
    try { await renameAsset(id, name) }
    catch (e) { if (a && prev !== undefined) a.name = prev; error.value = (e as Error).message }
  }

  async function remove(id: string) {
    try { await deleteAsset(id); assets.value = assets.value.filter((x) => x.id !== id) }
    catch (e) { error.value = (e as Error).message }
  }

  return { assets, loaded, error, load, upload, rename, remove }
})
