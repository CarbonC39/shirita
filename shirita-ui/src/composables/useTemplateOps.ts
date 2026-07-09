import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useLibraryStore } from '../stores/library'
import { estimateTokens, formatTokens } from '../utils/tokens'
import {
  listNodes, createTemplate, updateTemplate, duplicateTemplate, deleteTemplate, getOrphanDefinitions,
} from '../api/client'
import type { PromptNode } from '../api/types'

export function useTemplateOps() {
  const library = useLibraryStore()
  const { t: tr } = useI18n()

  const selectedTemplateId = ref<string | null>(null)
  const nodes = ref<PromptNode[]>([])
  const templateName = ref('')
  const renamingTemplate = ref(false)
  const templateNameInput = ref<HTMLInputElement | null>(null)

  const templateTokens = computed(() => {
    const byId = new Map(library.definitions.map((d) => [d.id, d]))
    return nodes.value.reduce((sum, n) => {
      if (n.kind !== 'ref' || !n.enabled || !n.definition_id) return sum
      return sum + estimateTokens(byId.get(n.definition_id)?.content ?? '')
    }, 0)
  })

  const isDefaultTemplate = computed(() => {
    const t = library.templates.find((x) => x.id === selectedTemplateId.value)
    return (t?.meta as Record<string, unknown> | undefined)?.default === true
  })

  async function selectTemplate(id: string) {
    selectedTemplateId.value = id || null
    try { localStorage.setItem('book.templateId', id || '') } catch { /* ignore */ }
    templateName.value = library.templates.find((t) => t.id === id)?.name ?? ''
    if (id) {
      try { nodes.value = await listNodes('template', id) }
      catch { nodes.value = [] }
    } else {
      nodes.value = []
    }
  }

  async function reload() {
    if (selectedTemplateId.value)
      nodes.value = await listNodes('template', selectedTemplateId.value)
  }

  async function createTemplateNamed(name: string) {
    try {
      const t = await createTemplate(name.trim() || 'New template')
      await library.loadTemplates()
      await selectTemplate(t.id)
    } catch (e) { throw e }
  }

  async function renameTemplate() {
    if (!selectedTemplateId.value) return
    const name = templateName.value.trim()
    const current = library.templates.find((t) => t.id === selectedTemplateId.value)
    if (!name || !current || name === current.name) {
      templateName.value = current?.name ?? name
      return
    }
    try {
      await updateTemplate(selectedTemplateId.value, name)
      await library.loadTemplates()
    } catch (e) { throw e }
  }

  async function dupTemplate() {
    if (!selectedTemplateId.value) return
    try {
      const t = await duplicateTemplate(selectedTemplateId.value)
      await library.loadTemplates()
      await selectTemplate(t.id)
    } catch (e) { throw e }
  }

  async function delTemplate() {
    if (!selectedTemplateId.value) return
    if (!confirm(tr('book.deleteTemplateConfirm'))) return
    try {
      const orphans = await getOrphanDefinitions(selectedTemplateId.value)
      const deleteOrphans = orphans.length > 0 && confirm(tr('book.deleteTemplateOrphans', orphans.length))
      await deleteTemplate(selectedTemplateId.value, deleteOrphans)
      selectedTemplateId.value = null
      templateName.value = ''
      nodes.value = []
      await library.loadTemplates()
    } catch (e) { throw e }
  }

  async function toggleDefaultTemplate() {
    const id = selectedTemplateId.value
    if (!id) return
    const turningOn = !isDefaultTemplate.value
    try {
      for (const t of library.templates) {
        if ((t.meta as Record<string, unknown>)?.default && t.id !== id) {
          await updateTemplate(t.id, t.name, { ...t.meta, default: false })
        }
      }
      const cur = library.templates.find((t) => t.id === id)
      await updateTemplate(id, cur?.name ?? templateName.value, { ...(cur?.meta ?? {}), default: turningOn })
      await library.loadTemplates()
    } catch (e) { throw e }
  }

  return {
    selectedTemplateId, nodes, templateName, renamingTemplate, templateNameInput,
    templateTokens, isDefaultTemplate,
    selectTemplate, reload, createTemplateNamed, renameTemplate, dupTemplate, delTemplate, toggleDefaultTemplate,
  }
}
