import { ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useLibraryStore } from '../stores/library'
import { useMediaStore } from '../stores/media'
import {
  importFile, downloadExport,
  exportDefinitionPath, exportTemplatePath, downloadPackExport,
} from '../api/client'
import type { OnConflict, ImportSummary, Definition, Pack } from '../api/types'
import { useToast } from './useToast'

export function useImportExport(deps: {
  selectedTemplateId: Ref<string | null>
  templateName: Ref<string>
  selectedPackId: Ref<string | null>
  selectedPack: Ref<Pack | null>
  selectTemplate: (id: string) => Promise<void>
  selectPack: (id: string) => void
}) {
  const library = useLibraryStore()
  const media = useMediaStore()
  const { t: tr } = useI18n()
  const { show: showToast } = useToast()

  const importSummary = ref<ImportSummary | null>(null)
  const importBusy = ref(false)
  const importInput = ref<HTMLInputElement | null>(null)
  const pendingImportFile = ref<File | null>(null)

  async function runImport(file: File, onConflict: OnConflict) {
    importBusy.value = true
    try {
      importSummary.value = await importFile(file, onConflict)
      pendingImportFile.value = importSummary.value.skipped.length > 0 ? file : null
      await library.loadAll()
      media.invalidate('avatar')
      const newPack = importSummary.value.created.find((c) => c.kind === 'pack')
      const newTemplate = importSummary.value.created.find((c) => c.kind === 'template')
      if (newPack) {
        deps.selectedPackId.value = newPack.id
      } else if (newTemplate) {
        await deps.selectTemplate(newTemplate.id)
      }
    } catch (e) {
      importSummary.value = null
      pendingImportFile.value = null
      throw e
    } finally {
      importBusy.value = false
    }
  }

  async function onImportPicked(e: Event) {
    const input = e.target as HTMLInputElement
    const file = input.files?.[0]
    if (!file) return
    // runImport re-throws on failure; without a catch the rejection was
    // unhandled and the user saw nothing (the summary panel only renders on
    // success). Surface the failure.
    try {
      await runImport(file, 'skip')
    } catch {
      showToast(tr('book.importFailed'), 'error')
    }
    input.value = ''
  }

  async function resolveImportConflicts(onConflict: OnConflict) {
    if (!pendingImportFile.value) return
    await runImport(pendingImportFile.value, onConflict)
  }

  async function exportDefinition(d: Definition) {
    if (!d.id) return
    await downloadExport(exportDefinitionPath(d.id), `${d.name || 'definition'}.json`)
  }

  async function exportSelectedTemplate() {
    if (!deps.selectedTemplateId.value) return
    await downloadExport(
      exportTemplatePath(deps.selectedTemplateId.value),
      `${deps.templateName.value || 'template'}.json`,
    )
  }

  async function exportSelectedPack() {
    if (!deps.selectedPack.value) return
    await downloadPackExport(deps.selectedPack.value.id, deps.selectedPack.value.name || 'pack')
  }

  return {
    importSummary, importBusy, importInput, pendingImportFile,
    runImport, onImportPicked, resolveImportConflicts,
    exportDefinition, exportSelectedTemplate, exportSelectedPack,
  }
}
