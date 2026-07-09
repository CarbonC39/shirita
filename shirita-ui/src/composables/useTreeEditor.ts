import { type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  listNodes, createNode, updateNode, deleteNode as apiDeleteNode, reorderNodes,
  createDefinition, updateDefinition,
} from '../api/client'
import { useLibraryStore } from '../stores/library'
import { selectOneSiblingsToDisable } from '../utils/tree'
import type { PromptNode, Trigger } from '../api/types'

export type TreeScope = 'template' | 'session' | 'pack'

export interface UseTreeEditorOptions {
  scope: TreeScope
  ownerId: Ref<string | null>
  nodes: Ref<PromptNode[]>
  reload: () => Promise<void>
  /** Only required for session scope — materializes template/pack nodes on first edit. */
  ensureMaterialized?: () => Promise<void>
  /** For session scope, inline content/trigger edits write to local override instead of global def. */
  setLocalPatch?: (defId: string, fields: Record<string, unknown>) => Promise<void>
}

function slugifyType(name: string) {
  const slug = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '')
  return slug || `type-${Date.now().toString(36)}`
}

export function useTreeEditor(opts: UseTreeEditorOptions) {
  const { scope, ownerId, nodes, reload, ensureMaterialized, setLocalPatch } = opts
  const { t: tr } = useI18n()
  const library = useLibraryStore()

  const ownerKind = scope === 'session' ? 'session' : scope === 'pack' ? 'pack' : 'template'

  async function guard() {
    if (!ownerId.value) throw new Error('No owner selected')
    if (ensureMaterialized) await ensureMaterialized()
  }

  async function addPrompt(definitionId: string) {
    if (!ownerId.value) return
    try {
      await guard()
      await createNode(ownerKind, ownerId.value, { parent_id: null, kind: 'ref', definition_id: definitionId })
      await reload()
    } catch { /* error handled by caller */ }
  }

  async function addContainer(typeId: string) {
    if (!ownerId.value) return
    try {
      await guard()
      await createNode(ownerKind, ownerId.value, { parent_id: null, kind: 'folder', tag: typeId })
      await reload()
    } catch { /* error handled by caller */ }
  }

  async function addRefToContainer(parentId: string, definitionId: string) {
    if (!ownerId.value) return
    try {
      await guard()
      await createNode(ownerKind, ownerId.value, { parent_id: parentId, kind: 'ref', definition_id: definitionId })
      await reload()
    } catch { /* error handled by caller */ }
  }

  async function createNewPrompt(name: string) {
    if (!ownerId.value) return
    try {
      await guard()
      const def = await createDefinition({ type: 'prompt', name: name?.trim() || 'New prompt', content: '', meta: {} })
      await library.loadDefinitions()
      await createNode(ownerKind, ownerId.value, { parent_id: null, kind: 'ref', definition_id: def.id })
      await reload()
    } catch { /* error handled by caller */ }
  }

  async function createNewInContainer(parentId: string | null, typeId: string) {
    if (!ownerId.value) return
    try {
      await guard()
      const isRx = typeId === 'regex_rule'
      const def = await createDefinition({
        type: typeId,
        name: isRx ? 'New rule' : `New ${typeId}`,
        content: '',
        meta: isRx ? { pattern: '', replacement: '', disabled: false, scope: 'display', targets: ['ai_output'] } : {},
      })
      await library.loadDefinitions()
      await createNode(ownerKind, ownerId.value, { parent_id: parentId, kind: 'ref', definition_id: def.id })
      await reload()
    } catch { /* error handled by caller */ }
  }

  async function createType(name: string) {
    if (!ownerId.value || !name.trim()) return
    try {
      await guard()
      const created = await library.addType(slugifyType(name), name.trim())
      await addContainer(created.id)
    } catch { /* error handled by caller */ }
  }

  async function toggleEnabled(nodeId: string) {
    if (!ownerId.value) return
    const node = nodes.value.find((n) => n.id === nodeId)
    if (!node) return
    const enabling = !node.enabled
    try {
      await guard()
      await updateNode(nodeId, { enabled: enabling })
      if (enabling) {
        for (const sib of selectOneSiblingsToDisable(nodes.value, nodeId)) {
          await updateNode(sib, { enabled: false })
        }
      }
      await reload()
    } catch { /* error handled by caller */ }
  }

  async function updateNodeMeta(nodeId: string, meta: Record<string, unknown>) {
    if (!ownerId.value) return
    try {
      await guard()
      await updateNode(nodeId, { meta })
      await reload()
    } catch { /* error handled by caller */ }
  }

  async function updateContent(definitionId: string, content: string) {
    try {
      if (setLocalPatch) {
        await setLocalPatch(definitionId, { content })
      } else {
        await updateDefinition(definitionId, { content })
        await library.loadDefinitions()
      }
    } catch { /* error handled by caller */ }
  }

  async function updateTrigger(definitionId: string, trigger: Trigger) {
    try {
      if (setLocalPatch) {
        await setLocalPatch(definitionId, { trigger })
      } else {
        const def = library.definitions.find((d) => d.id === definitionId)
        if (!def) return
        await updateDefinition(definitionId, { meta: { ...def.meta, trigger } })
        await library.loadDefinitions()
      }
    } catch { /* error handled by caller */ }
  }

  async function updateDefMeta(definitionId: string, meta: Record<string, unknown>) {
    try {
      await updateDefinition(definitionId, { meta })
      await library.loadDefinitions()
    } catch { /* error handled by caller */ }
  }

  async function updateDefName(definitionId: string, name: string) {
    try {
      await updateDefinition(definitionId, { name })
      await library.loadDefinitions()
    } catch { /* error handled by caller */ }
  }

  async function deleteNode(nodeId: string) {
    if (!ownerId.value) return
    const node = nodes.value.find((n) => n.id === nodeId)
    if (!node) return
    const childCount = nodes.value.filter((n) => n.parent_id === nodeId).length
    if (node.kind === 'folder' && childCount > 0 && !confirm(tr('prompt.deleteContainerConfirm', childCount))) return
    if (!confirm(tr('prompt.deleteNodeConfirm', { name: node.tag || '(untitled)' }))) return
    try {
      await guard()
      await apiDeleteNode(nodeId)
      await reload()
    } catch { /* error handled by caller */ }
  }

  async function reorder(orderedIds: string[]) {
    if (!ownerId.value) return
    try {
      await guard()
      await reorderNodes(ownerKind, ownerId.value, orderedIds)
      await reload()
    } catch { /* error handled by caller */ }
  }

  async function addPanel() {
    if (!ownerId.value) return
    try {
      await guard()
      const html = await createDefinition({ type: 'html', name: 'Panel HTML', content: '', meta: {} })
      const css = await createDefinition({ type: 'css', name: 'Panel CSS', content: '', meta: {} })
      await library.loadDefinitions()
      const folder = await createNode(ownerKind, ownerId.value, { parent_id: null, kind: 'folder', tag: 'panel' })
      await updateNode(folder.id, { meta: { name: 'Panel', caps: {} } })
      await createNode(ownerKind, ownerId.value, { parent_id: folder.id, kind: 'ref', definition_id: html.id })
      await createNode(ownerKind, ownerId.value, { parent_id: folder.id, kind: 'ref', definition_id: css.id })
      await reload()
    } catch { /* error handled by caller */ }
  }

  return {
    addPrompt, addContainer, addRefToContainer,
    createNewPrompt, createNewInContainer, createType,
    toggleEnabled, updateNodeMeta,
    updateContent, updateTrigger, updateDefMeta, updateDefName,
    deleteNode, reorder,
    addPanel,
  }
}
