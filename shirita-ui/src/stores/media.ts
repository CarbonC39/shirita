import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listAssets, uploadAsset, renameAsset, deleteAsset, type Asset } from '../api/client'

// The shared media library — uploaded images with editable names, tagged by
// kind (avatar/background). Picker components pass their kind to filter.
export const useMediaStore = defineStore('media', () => {
  const assets = ref<Record<string, Asset[]>>({ avatar: [], background: [] })
  const loaded = ref<Record<string, boolean>>({ avatar: false, background: false })
  const error = ref<string | null>(null)

  function byKind(kind: 'avatar' | 'background') { return assets.value[kind] ?? [] }

  async function load(kind: 'avatar' | 'background', force = false) {
    if (loaded.value[kind] && !force) return
    try { assets.value[kind] = await listAssets(kind); loaded.value[kind] = true }
    catch (e) { error.value = (e as Error).message }
  }

  async function upload(file: File, kind: 'avatar' | 'background' = 'background'): Promise<Asset | null> {
    try { const a = await uploadAsset(file, kind); assets.value[kind] = [a, ...byKind(kind)]; return a }
    catch (e) { error.value = (e as Error).message; return null }
  }

  async function rename(id: string, kind: 'avatar' | 'background', name: string) {
    const a = byKind(kind).find((x) => x.id === id)
    const prev = a?.name
    if (a) a.name = name
    try { await renameAsset(id, name) }
    catch (e) { if (a && prev !== undefined) a.name = prev; error.value = (e as Error).message }
  }

  async function remove(id: string, kind: 'avatar' | 'background') {
    try { await deleteAsset(id); assets.value[kind] = byKind(kind).filter((x) => x.id !== id) }
    catch (e) { error.value = (e as Error).message }
  }

  return { assets, loaded, error, byKind, load, upload, rename, remove }
})
