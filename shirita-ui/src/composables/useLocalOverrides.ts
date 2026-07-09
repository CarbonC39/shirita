import { ref, reactive, computed, watch, type Ref } from 'vue'
import { useUiStore } from '../stores/ui'
import { useLibraryStore } from '../stores/library'
import { deepClone } from '../utils/clone'
import {
  getSession, listNodes, setLocalDefinition, clearLocalDefinition,
  setLocalVariables, materializeNodes, materializePackNodes,
} from '../api/client'
import type { PromptNode, Session, Definition, Trigger, VarDecl } from '../api/types'

function blankDef(): Definition {
  return { id: '', type: 'char', name: '', content: '', meta: {} }
}

export function useLocalOverrides(opts: { selectedPackId: Ref<string | null> }) {
  const library = useLibraryStore()
  const ui = useUiStore()

  const customizedLocally = ref(false)
  const localSession = ref<Session | null>(null)
  const localEditDef = reactive<Definition>(blankDef())
  const localDefActive = ref(false)
  const localSavedTick = ref(0)
  const localNodes = ref<PromptNode[]>([])

  const localDefs = computed<Record<string, Record<string, unknown>>>(
    () =>
      (localSession.value?.override_config as Record<string, unknown>)
        ?.local_definitions as Record<string, Record<string, unknown>> ?? {},
  )

  const localVars = computed<VarDecl[]>(
    () => ((localSession.value?.override_config as Record<string, unknown> | undefined)?.local_variables as VarDecl[]) ?? [],
  )

  // Session template nodes: materialized template nodes (tagged _source='template')
  // PLUS newly-created session nodes (no _source tag yet).
  // Pack nodes (_source='pack') stay excluded.
  const templateNodes = computed(() =>
    localNodes.value.filter((n) => {
      const s = (n.meta as Record<string, unknown> | null)?._source
      return s === 'template' || s === undefined
    }),
  )

  async function loadLocal() {
    if (!ui.activeChatId) {
      localSession.value = null
      return
    }
    try { localSession.value = await getSession(ui.activeChatId) }
    catch { /* ignore */ }
  }
  watch(() => ui.activeChatId, loadLocal, { immediate: true })

  function defName(defId: string): string {
    return library.definitions.find((d) => d.id === defId)?.name ?? defId
  }

  function editLocal(defId: string) {
    if (!defId) return
    const base = library.definitions.find((d) => d.id === defId)
    if (!base) return
    const patch = localDefs.value[defId] ?? {}
    const meta = deepClone(base.meta) as Record<string, unknown>
    if (patch.trigger) meta.trigger = patch.trigger
    if (patch.scan) meta.scan = patch.scan
    Object.assign(localEditDef, {
      id: base.id,
      type: base.type,
      name: (patch.name as string) ?? base.name,
      content: (patch.content as string) ?? base.content,
      meta,
    })
    localDefActive.value = true
  }

  async function saveLocal() {
    if (!ui.activeChatId || !localEditDef.id) return
    const base = library.definitions.find((d) => d.id === localEditDef.id)
    const patch: Record<string, unknown> = {}
    if (base && localEditDef.content !== base.content) patch.content = localEditDef.content
    if (base && localEditDef.name !== base.name) patch.name = localEditDef.name
    const t = (localEditDef.meta as Record<string, unknown>).trigger
    if (t) patch.trigger = t
    const s = (localEditDef.meta as Record<string, unknown>).scan
    if (s) patch.scan = s
    try {
      await setLocalDefinition(ui.activeChatId, localEditDef.id, patch)
      await loadLocal()
      localSavedTick.value++
    } catch (e) { throw e }
  }

  async function revertLocal(defId: string) {
    if (!ui.activeChatId) return
    try {
      await clearLocalDefinition(ui.activeChatId, defId)
      if (localEditDef.id === defId) {
        Object.assign(localEditDef, blankDef())
        localDefActive.value = false
      }
      await loadLocal()
    } catch (e) { throw e }
  }

  async function setLocalPatch(defId: string, fields: Record<string, unknown>) {
    if (!ui.activeChatId) return
    try {
      const existing = localDefs.value[defId] ?? {}
      await setLocalDefinition(ui.activeChatId, defId, { ...existing, ...fields })
      await loadLocal()
    } catch (e) { throw e }
  }

  async function saveLocalVars(vars: VarDecl[]) {
    if (!ui.activeChatId) return
    try {
      await setLocalVariables(ui.activeChatId, vars)
      await loadLocal()
    } catch (e) { throw e }
  }

  async function loadLocalNodes() {
    if (!ui.activeChatId) { localNodes.value = []; return }
    try { localNodes.value = await listNodes('session', ui.activeChatId) }
    catch { localNodes.value = [] }
  }
  watch(() => ui.activeChatId, loadLocalNodes, { immediate: true })

  async function ensureMaterialized() {
    if (!ui.activeChatId) return
    if (localNodes.value.length === 0) {
      await materializeNodes(ui.activeChatId)
      await loadLocalNodes()
    }
    customizedLocally.value = true
  }

  async function ensurePackMaterialized(packId: string) {
    if (!ui.activeChatId) return
    if (localNodes.value.length === 0) {
      await materializePackNodes(ui.activeChatId, packId)
      await loadLocalNodes()
    }
    customizedLocally.value = true
  }

  async function materializeAll() {
    if (localSession.value?.template_id) await ensureMaterialized()
    if (opts.selectedPackId.value) await ensurePackMaterialized(opts.selectedPackId.value)
  }

  return {
    customizedLocally, localSession, localDefs, localVars,
    localEditDef, localDefActive, localSavedTick,
    localNodes, templateNodes,
    loadLocal, editLocal, saveLocal, revertLocal, setLocalPatch,
    saveLocalVars, loadLocalNodes,
    ensureMaterialized, ensurePackMaterialized, materializeAll,
    defName,
  }
}
