import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useLibraryStore } from '../stores/library'
import {
  createPack, updatePack, deletePack, duplicatePack, getOrphanDefinitionsForPack,
  downloadPackExport,
} from '../api/client'
import type { Pack } from '../api/types'

export function usePackOps() {
  const library = useLibraryStore()
  const { t: tr } = useI18n()

  const selectedPackId = ref<string | null>(null)
  const selectedPack = computed<Pack | null>(() =>
    library.packs.find((p) => p.id === selectedPackId.value) ?? null,
  )
  const renamingPack = ref(false)
  const packNameDraft = ref('')

  function selectPack(id: string) {
    selectedPackId.value = id || null
    try { localStorage.setItem('book.packId', id || '') } catch { /* ignore */ }
  }

  async function createPackNamed(name: string) {
    try {
      const p = await createPack({ name: name?.trim() || 'New pack' })
      await library.loadPacks()
      selectedPackId.value = p.id
    } catch (e) { throw e }
  }

  function startRenamePack() {
    if (!selectedPack.value) return
    packNameDraft.value = selectedPack.value.name
    renamingPack.value = true
  }

  async function renamePack() {
    const p = selectedPack.value
    const n = packNameDraft.value.trim()
    renamingPack.value = false
    if (!p || !n || n === p.name) return
    try {
      await updatePack(p.id, { name: n, identity: p.identity, meta: p.meta as Record<string, unknown> })
      await library.loadPacks()
    } catch (e) { throw e }
  }

  async function dupPack() {
    if (!selectedPackId.value) return
    try {
      const p = await duplicatePack(selectedPackId.value)
      await library.loadPacks()
      selectedPackId.value = p.id
    } catch (e) { throw e }
  }

  async function delPack() {
    if (!selectedPackId.value) return
    if (!confirm(tr('book.deletePackConfirm'))) return
    try {
      const orphans = await getOrphanDefinitionsForPack(selectedPackId.value)
      const deleteOrphans = orphans.length > 0 && confirm(tr('book.deleteTemplateOrphans', orphans.length))
      await deletePack(selectedPackId.value, deleteOrphans)
      selectedPackId.value = null
      await library.loadPacks()
    } catch (e) { throw e }
  }

  async function exportSelectedPack() {
    if (!selectedPack.value) return
    await downloadPackExport(selectedPack.value.id, selectedPack.value.name || 'pack')
  }

  return {
    selectedPackId, selectedPack, renamingPack, packNameDraft,
    selectPack, createPackNamed, startRenamePack, renamePack, dupPack, delPack, exportSelectedPack,
  }
}
