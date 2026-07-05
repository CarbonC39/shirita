import type { InjectionKey, ComputedRef, Ref } from 'vue'
import type { PromptNode, Definition, DefType, Trigger } from '../../api/types'

// One entry on the BookNavigator stack. stack[0] is always sessionRoot.
export type Target =
  | { kind: 'sessionRoot' }
  | { kind: 'pack'; packId: string }
  | { kind: 'definition'; definitionId: string }

// PromptTree event handlers, branched on editTarget by the provider (BookView).
export interface TreeHandlers {
  toggleEnabled: (nodeId: string) => void
  addPrompt: (definitionId: string) => void
  addContainer: (typeId: string) => void
  addRefToContainer: (parentId: string, definitionId: string) => void
  createNewPrompt: (name: string) => void
  createNewInContainer: (parentId: string | null, typeId: string) => void
  createType: (name: string) => void
  updateContent: (definitionId: string, content: string) => void
  updateTrigger: (definitionId: string, trigger: Trigger) => void
  updateNodeMeta: (nodeId: string, meta: Record<string, unknown>) => void
  updateDefMeta: (definitionId: string, meta: Record<string, unknown>) => void
  updateDefName: (definitionId: string, name: string) => void
  deleteNode: (nodeId: string) => void
  reorder: (orderedIds: string[]) => void
}

// Everything a book level component needs from the host (BookView), via inject.
export interface LocalBookApi {
  // session template nodes (localNodes filtered by meta._source === 'template')
  templateNodes: ComputedRef<PromptNode[]>
  definitions: Definition[]
  types: DefType[]
  // local definition overrides (session.override_config.local_definitions)
  localDefs: ComputedRef<Record<string, Record<string, unknown>>>
  // local definition editor buffer + state
  localEditDef: Definition
  localDefActive: Ref<boolean>
  localSavedTick: Ref<number>
  // PromptTree wiring (session-owner node ops)
  tree: TreeHandlers
  // local definition override ops
  editLocal: (defId: string) => void
  saveLocal: () => Promise<void>
  revertLocal: (defId: string) => Promise<void>
  defName: (defId: string) => string
}

export const LOCAL_BOOK_KEY: InjectionKey<LocalBookApi> = Symbol('local-book')
