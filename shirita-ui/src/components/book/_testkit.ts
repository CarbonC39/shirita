import { reactive, ref } from 'vue'
import type { LocalBookApi } from './types'

// Minimal LocalBookApi stub for DefinitionView tests.
export function blankDefHolder(overrides: Partial<LocalBookApi> = {}): LocalBookApi {
  return {
    templateNodes: ref([]) as any,
    definitions: [{ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: {} }] as any,
    types: [],
    localDefs: ref({}) as any,
    localEditDef: reactive({ id: 'd1', type: 'prompt', name: 'N', content: 'c', meta: {} }) as any,
    localDefActive: ref(true),
    localSavedTick: ref(0),
    tree: {} as any,
    editLocal: () => {},
    saveLocal: async () => {},
    revertLocal: async () => {},
    defName: () => 'N',
    ...overrides,
  } as any
}
