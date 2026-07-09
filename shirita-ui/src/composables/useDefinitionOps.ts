import { reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useLibraryStore } from '../stores/library'
import {
  createDefinition, updateDefinition, deleteDefinition, exportDefinitionPath, downloadExport,
} from '../api/client'
import type { Definition } from '../api/types'

function blankDef(): Definition {
  return { id: '', type: 'char', name: '', content: '', meta: {} }
}

export function useDefinitionOps() {
  const library = useLibraryStore()
  const { t: tr } = useI18n()

  const editDef = reactive<Definition>(blankDef())
  const defActive = ref(false)
  const defSavedTick = ref(0)

  function loadDef(d: Definition) {
    Object.assign(editDef, {
      id: d.id,
      type: d.type,
      name: d.name,
      content: d.content,
      meta: { ...d.meta },
    })
  }

  function selectDefinition(id: string, seedName?: string) {
    try { localStorage.setItem('book.defId', id || '') } catch { /* ignore */ }
    if (!id) {
      loadDef({ ...blankDef(), name: seedName?.trim() || '' })
      defActive.value = true
      return
    }
    const found = library.definitions.find((d) => d.id === id)
    if (found) {
      loadDef(found)
      defActive.value = true
    }
  }

  async function saveDefinition() {
    try {
      if (editDef.id) {
        await updateDefinition(editDef.id, {
          type: editDef.type,
          name: editDef.name,
          content: editDef.content,
          meta: editDef.meta,
        })
      } else {
        const created = await createDefinition({
          type: editDef.type,
          name: editDef.name || 'Untitled',
          content: editDef.content,
          meta: editDef.meta,
        })
        editDef.id = created.id
      }
      await library.loadDefinitions()
      defSavedTick.value++
    } catch (e) { throw e }
  }

  async function deleteDef() {
    if (!editDef.id) {
      loadDef(blankDef())
      defActive.value = false
      return
    }
    if (!confirm(tr('book.deleteDefConfirm', { name: editDef.name }))) return
    try {
      await deleteDefinition(editDef.id)
      loadDef(blankDef())
      defActive.value = false
      await library.loadDefinitions()
    } catch (e) { throw e }
  }

  async function duplicateDef() {
    try {
      const created = await createDefinition({
        type: editDef.type,
        name: `${editDef.name || 'Untitled'} copy`,
        content: editDef.content,
        meta: editDef.meta,
      })
      await library.loadDefinitions()
      loadDef(created)
    } catch (e) { throw e }
  }

  async function exportDefinition(d: Definition) {
    if (!d.id) return
    await downloadExport(exportDefinitionPath(d.id), `${d.name || 'definition'}.json`)
  }

  return {
    editDef, defActive, defSavedTick,
    blankDef, loadDef, selectDefinition, saveDefinition, deleteDef, duplicateDef, exportDefinition,
  }
}
