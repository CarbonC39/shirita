<script setup lang="ts">
import type { Definition, DefType, PromptNode, Trigger } from '../../api/types'
import EntityToolbar from '../EntityToolbar.vue'
import PromptTree from '../PromptTree.vue'

defineProps<{
  items: { id: string; name: string }[]
  selectedId: string | null
  selectedLabel: string
  placeholder: string
  createLabel: string
  isDefault?: boolean
  nodes: PromptNode[]
  definitions: Definition[]
  types: DefType[]
}>()

const emit = defineEmits<{
  select: [id: string]
  create: [name: string]
  rename: [name: string]
  import: []
  export: []
  duplicate: []
  delete: []
  'toggle-default': []
  toggleEnabled: [nodeId: string]
  addPrompt: [definitionId: string]
  addContainer: [typeId: string]
  addRefToContainer: [parentId: string, definitionId: string]
  createNewPrompt: [name: string]
  createNewInContainer: [parentId: string | null, typeId: string]
  createType: [name: string]
  updateContent: [definitionId: string, content: string]
  updateTrigger: [definitionId: string, trigger: Trigger]
  updateNodeMeta: [nodeId: string, meta: Record<string, unknown>]
  updateDefMeta: [definitionId: string, meta: Record<string, unknown>]
  updateDefName: [definitionId: string, name: string]
  deleteNode: [nodeId: string]
  reorder: [orderedIds: string[]]
  openDefinition: [definitionId: string]
}>()
</script>

<template>
  <div class="rounded-2xl bg-mauve/5 border border-line/60 p-4 mb-4">
    <h2 data-test="section-template" class="flex items-center text-[12px] font-semibold uppercase tracking-wide text-mauve border-l-2 border-mauve pl-2 mb-3">
      {{ $t('book.templateHeading') }}
    </h2>
    <EntityToolbar
      :items="items"
      :selected-id="selectedId"
      :selected-label="selectedLabel"
      :placeholder="placeholder"
      :create-label="createLabel"
      :show-default="true"
      :is-default="isDefault"
      :entity-name="selectedLabel"
      @select="emit('select', $event)"
      @create="emit('create', $event)"
      @rename="emit('rename', $event)"
      @import="emit('import')"
      @export="emit('export')"
      @duplicate="emit('duplicate')"
      @delete="emit('delete')"
      @toggle-default="emit('toggle-default')"
    />
    <div class="mb-3" />
    <PromptTree
      v-if="selectedId"
      :nodes="nodes"
      :definitions="definitions"
      :types="types"
      @toggle-enabled="emit('toggleEnabled', $event)"
      @add-prompt="emit('addPrompt', $event)"
      @add-container="emit('addContainer', $event)"
      @add-ref-to-container="(parentId, defId) => emit('addRefToContainer', parentId, defId)"
      @create-new-prompt="emit('createNewPrompt', $event)"
      @create-new-in-container="(parentId, typeId) => emit('createNewInContainer', parentId, typeId)"
      @create-type="emit('createType', $event)"
      @update-content="(defId, content) => emit('updateContent', defId, content)"
      @update-trigger="(defId, trigger) => emit('updateTrigger', defId, trigger)"
      @update-node-meta="(nodeId, meta) => emit('updateNodeMeta', nodeId, meta)"
      @update-def-meta="(defId, meta) => emit('updateDefMeta', defId, meta)"
      @update-def-name="(defId, name) => emit('updateDefName', defId, name)"
      @delete-node="emit('deleteNode', $event)"
      @reorder="emit('reorder', $event)"
      @open-definition="emit('openDefinition', $event)"
    />
  </div>
</template>
