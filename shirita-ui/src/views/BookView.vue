<script setup lang="ts">
import { ref, computed, watch, onMounted, provide, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronDown } from 'lucide-vue-next'
import { useLibraryStore } from '../stores/library'
import { useUiStore } from '../stores/ui'
import { useMediaStore } from '../stores/media'
import { createDefinition } from '../api/client'
import { deepClone } from '../utils/clone'
import { useTemplateOps } from '../composables/useTemplateOps'
import { usePackOps } from '../composables/usePackOps'
import { useDefinitionOps } from '../composables/useDefinitionOps'
import { useLocalOverrides } from '../composables/useLocalOverrides'
import { useImportExport } from '../composables/useImportExport'
import { useTreeEditor } from '../composables/useTreeEditor'
import type { Definition } from '../api/types'
import BookNavigator from '../components/book/BookNavigator.vue'
import { LOCAL_BOOK_KEY } from '../components/book/types'
import VariablesEditor from '../components/VariablesEditor.vue'
import TemplateSection from '../components/book/TemplateSection.vue'
import PackSection from '../components/book/PackSection.vue'
import DefinitionSection from '../components/book/DefinitionSection.vue'

const { t } = useI18n()
const library = useLibraryStore()
const ui = useUiStore()
const media = useMediaStore()

// ── composables ──
const template = useTemplateOps()
const pack = usePackOps()
const definition = useDefinitionOps()
const local = useLocalOverrides({ selectedPackId: pack.selectedPackId })
const impex = useImportExport({
  selectedTemplateId: template.selectedTemplateId,
  templateName: template.templateName,
  selectedPackId: pack.selectedPackId,
  selectedPack: pack.selectedPack,
  selectTemplate: template.selectTemplate,
  selectPack: pack.selectPack,
})

const templateTree = useTreeEditor({
  scope: 'template',
  ownerId: template.selectedTemplateId,
  nodes: template.nodes,
  reload: template.reload,
})

const sessionTree = useTreeEditor({
  scope: 'session',
  ownerId: computed(() => ui.activeChatId),
  nodes: local.localNodes,
  reload: local.loadLocalNodes,
  ensureMaterialized: local.ensureMaterialized,
  setLocalPatch: local.setLocalPatch,
})

const packTree = useTreeEditor({
  scope: 'pack',
  ownerId: pack.selectedPackId,
  nodes: local.localNodes,
  reload: local.loadLocalNodes,
  ensureMaterialized: () => local.ensurePackMaterialized(pack.selectedPackId.value!),
  setLocalPatch: local.setLocalPatch,
})

// ── provide for BookNavigator ──
provide(LOCAL_BOOK_KEY, {
  templateNodes: local.templateNodes,
  definitions: library.definitions,
  types: library.containerTypes,
  localDefs: local.localDefs,
  localEditDef: local.localEditDef,
  localDefActive: local.localDefActive,
  localSavedTick: local.localSavedTick,
  tree: sessionTree,
  editLocal: local.editLocal,
  saveLocal: local.saveLocal,
  revertLocal: local.revertLocal,
  defName: local.defName,
})

// ── init ──
const loading = ref(true)
const error = ref<string | null>(null)
const showGlobal = ref(true)

onMounted(async () => {
  try {
    await library.loadAll()

    const lastTpl = localStorage.getItem('book.templateId')
    const lastPack = localStorage.getItem('book.packId')
    const lastDef = localStorage.getItem('book.defId')

    if (lastTpl && library.templates.some((x) => x.id === lastTpl)) {
      await template.selectTemplate(lastTpl)
    } else {
      const defTpl = library.templates.find((x) => (x.meta as Record<string, unknown>)?.default)
      const first = defTpl ?? library.templates[0]
      if (first) await template.selectTemplate(first.id)
    }

    if (lastPack && library.packs.some((x) => x.id === lastPack)) {
      pack.selectPack(lastPack)
    }

    // Handle ?create=name
    const createName = new URLSearchParams(window.location.search).get('create')
    if (createName) {
      await template.createTemplateNamed(createName)
      window.history.replaceState({}, '', '/book')
    }

    if (lastDef && library.definitions.some((d) => d.id === lastDef)) {
      definition.selectDefinition(lastDef)
    }
  } catch (e) {
    error.value = (e as Error).message
  } finally {
    loading.value = false
  }
})

// ── entity lists for toolbars ──
const templateItems = computed(() =>
  library.templates.map((x) => ({ id: x.id, name: x.name })),
)
const packItems = computed(() =>
  library.packs.map((x) => ({ id: x.id, name: x.name })),
)
const defItems = computed(() =>
  library.definitions.map((d) => ({ id: d.id, name: d.name })),
)

// ── toolbar handlers ──
async function onTemplateRename(name: string) {
  template.templateName.value = name
  await template.renameTemplate()
}
async function onPackRename(name: string) {
  pack.packNameDraft.value = name
  pack.renamingPack.value = true
  await pack.renamePack()
}
async function onDefRename(name: string) {
  definition.editDef.name = name
  await definition.saveDefinition()
}

async function onDefCreate(name: string) {
  // Create a NEW definition from the current editor state
  const created = await createDefinition({
    type: definition.editDef.type || 'char',
    name: name || 'Untitled',
    content: definition.editDef.content,
    meta: deepClone(definition.editDef.meta),
  })
  await library.loadDefinitions()
  definition.selectDefinition(created.id)
}

async function onImport() {
  nextTick(() => impex.importInput.value?.click())
}
</script>

<template>
  <div class="pt-6 pb-12">
    <p v-if="loading" class="text-muted text-sm text-center pt-12">{{ $t('common.loading') }}</p>
    <template v-else>
      <p v-if="error" data-test="error" class="text-coral text-sm mb-4">{{ error }}</p>

      <!-- Import conflict modal -->
      <div
        v-if="impex.importSummary.value"
        data-test="import-conflict"
        class="rounded-2xl border border-primary/40 bg-card p-5 mb-6 space-y-3"
      >
        <p data-test="import-summary" class="text-[12px] text-muted mt-2">
          {{
            $t("book.importSummary", {
              created: impex.importSummary.value.created.length,
              skipped: impex.importSummary.value.skipped.length,
              overwritten: impex.importSummary.value.overwritten.length,
            })
          }}
        </p>
        <p v-if="impex.importSummary.value && impex.importSummary.value.created.some((c: any) => c.kind === 'panel')" data-test="import-panel-hint" class="text-[12px] text-muted/70">
          {{ $t("book.importPanelDetected") }}
        </p>
        <div v-if="impex.importSummary.value && impex.importSummary.value.skipped.length > 0" class="flex items-center gap-3 flex-wrap">
          <span>{{ $t("book.importConflictFound", { skipped: impex.importSummary.value.skipped.length }) }}</span>
          <button class="btn btn-primary" :disabled="impex.importBusy.value" @click="impex.resolveImportConflicts('overwrite')">{{ $t("book.conflictOverwrite") }}</button>
          <button class="btn" :disabled="impex.importBusy.value" @click="impex.resolveImportConflicts('duplicate')">{{ $t("book.conflictDuplicate") }}</button>
        </div>
      </div>

      <!-- Import file input (shared) -->
      <input :ref="(el) => { impex.importInput.value = el as HTMLInputElement }" type="file" hidden accept=".json,application/json,.png,image/png,.zip,application/zip" @change="impex.onImportPicked" />

      <!-- ── LOCAL SECTION (session-context) ── -->
      <section v-if="ui.activeChatId" data-test="book-local" class="rounded-2xl bg-primary/5 border border-line/60 p-4 mb-6">
        <h2 class="flex items-center text-[12px] font-semibold uppercase tracking-wide text-primary border-l-2 border-primary pl-2 mb-3">
          {{ $t('book.localHeading') }}
        </h2>

        <template v-if="!local.customizedLocally.value">
          <button
            data-test="customize-locally"
            class="btn btn-primary"
            @click="local.materializeAll()"
          >{{ $t('book.customizeLocally') }}</button>
        </template>

        <template v-else>
          <BookNavigator :root-target="{ kind: 'sessionRoot' }" />
          <div class="mt-3 flex justify-end">
            <button
              data-test="local-revert"
              class="text-[12px] text-muted hover:text-coral"
              @click="local.customizedLocally.value = false"
            >{{ $t('book.revertToGlobal') }}</button>
          </div>

          <div class="mt-3" data-test="local-variables">
            <span class="text-[12px] text-muted block mb-1.5">{{ $t('book.variablesThisChat') }}</span>
            <VariablesEditor
              :model-value="local.localVars.value"
              @update:model-value="local.saveLocalVars"
            />
          </div>
        </template>
      </section>

      <!-- ── GLOBAL TOGGLE ── -->
      <button
        v-if="local.customizedLocally.value"
        data-test="toggle-global"
        class="flex items-center gap-2 text-[13px] text-muted hover:text-ink mb-4"
        @click="showGlobal = !showGlobal"
      >
        <ChevronDown :size="15" :class="showGlobal ? '' : '-rotate-90'" class="transition-transform" />
        {{ showGlobal ? $t('book.hideGlobalLibrary') : $t('book.showGlobalLibrary') }}
      </button>

      <!-- ── GLOBAL SECTIONS ── -->
      <div v-if="showGlobal" data-test="book-global">
        <TemplateSection
          :items="templateItems"
          :selected-id="template.selectedTemplateId.value"
          :selected-label="template.templateName.value"
          :placeholder="$t('book.templateNamePlaceholder')"
          :create-label="$t('book.newTemplate')"
          :is-default="template.isDefaultTemplate.value"
          :nodes="template.nodes.value"
          :definitions="library.definitions"
          :types="library.containerTypes"
          @select="template.selectTemplate"
          @create="template.createTemplateNamed"
          @rename="(name: string) => onTemplateRename(name)"
          @import="onImport"
          @export="impex.exportSelectedTemplate"
          @duplicate="template.dupTemplate"
          @delete="template.delTemplate"
          @toggle-default="template.toggleDefaultTemplate"
          @toggle-enabled="templateTree.toggleEnabled"
          @add-prompt="templateTree.addPrompt"
          @add-container="templateTree.addContainer"
          @add-ref-to-container="templateTree.addRefToContainer"
          @create-new-prompt="templateTree.createNewPrompt"
          @create-new-in-container="templateTree.createNewInContainer"
          @create-type="templateTree.createType"
          @update-content="templateTree.updateContent"
          @update-trigger="templateTree.updateTrigger"
          @update-node-meta="templateTree.updateNodeMeta"
          @update-def-meta="templateTree.updateDefMeta"
          @update-def-name="templateTree.updateDefName"
          @delete-node="templateTree.deleteNode"
          @reorder="templateTree.reorder"
          @open-definition="definition.selectDefinition"
        />

        <PackSection
          :items="packItems"
          :selected-id="pack.selectedPackId.value"
          :selected-label="pack.selectedPack?.value?.name ?? ''"
          :placeholder="$t('book.packNamePlaceholder')"
          :create-label="$t('book.createPack')"
          :selected-pack="pack.selectedPack.value"
          @select="pack.selectPack"
          @create="pack.createPackNamed"
          @rename="(name: string) => onPackRename(name)"
          @import="onImport"
          @export="pack.exportSelectedPack"
          @duplicate="pack.dupPack"
          @delete="pack.delPack"
          @pack-changed="library.loadPacks()"
        />

        <DefinitionSection
          :items="defItems"
          :selected-id="definition.editDef.id || null"
          :selected-label="definition.editDef.name"
          :placeholder="$t('definition.searchPlaceholder')"
          :create-label="$t('definition.newDefinition')"
          :edit-def="definition.editDef"
          :edit-def-active="definition.defActive.value"
          :edit-def-saved-tick="definition.defSavedTick.value"
          :types="library.containerTypes"
          @select="definition.selectDefinition"
          @create="onDefCreate"
          @rename="(name: string) => onDefRename(name)"
          @import="onImport"
          @export="definition.exportDefinition(definition.editDef)"
          @duplicate="definition.duplicateDef"
          @delete="definition.deleteDef"
          @update:def-name="definition.editDef.name = $event"
          @update:def-type="definition.editDef.type = $event"
          @update:def-content="definition.editDef.content = $event"
          @update:def-meta="definition.editDef.meta = $event"
          @save="definition.saveDefinition"
          @create-type="(n: string) => library.addType(n.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, ''), n)"
          @delete-type="library.removeType"
        />
      </div>
    </template>
  </div>
</template>
