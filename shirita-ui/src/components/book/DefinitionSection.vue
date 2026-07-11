<script setup lang="ts">
import type { Definition, DefType } from '../../api/types'
import EntityToolbar from '../EntityToolbar.vue'
import DefinitionEditor from '../DefinitionEditor.vue'

defineProps<{
  items: { id: string; name: string }[]
  selectedId: string | null
  selectedLabel: string
  placeholder: string
  createLabel: string
  // Editor
  editDef: Definition
  editDefActive: boolean
  editDefSavedTick: number
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
  // definition editor
  'update:def-name': [name: string]
  'update:def-type': [type: string]
  'update:def-content': [content: string]
  'update:def-meta': [meta: Record<string, unknown>]
  save: []
  'create-type': [name: string]
  'delete-type': [id: string]
}>()
</script>

<template>
  <div class="rounded-2xl bg-sky/20 border border-line/60 p-4 mb-4">
    <h2 data-test="section-definition" class="flex items-center text-[12px] font-semibold uppercase tracking-wide text-ink/80 border-l-[3px] border-sky pl-2 mb-3">
      {{ $t('definition.heading') }}
    </h2>
    <EntityToolbar
      :items="items"
      :selected-id="selectedId"
      :selected-label="selectedLabel"
      :placeholder="placeholder"
      :create-label="createLabel"
      :entity-name="selectedLabel"
      show-rename
      show-import
      show-export
      show-duplicate
      show-delete
      @select="emit('select', $event)"
      @create="emit('create', $event)"
      @rename="emit('rename', $event)"
      @import="emit('import')"
      @export="emit('export')"
      @duplicate="emit('duplicate')"
      @delete="emit('delete')"
    />
    <div class="mb-2" />
    <DefinitionEditor
      :definition="editDef"
      hide-heading
      :types="types"
      :active="editDefActive"
      :saved-tick="editDefSavedTick"
      @update:name="emit('update:def-name', $event)"
      @update:type="emit('update:def-type', $event)"
      @update:content="emit('update:def-content', $event)"
      @update:meta="emit('update:def-meta', $event)"
      @save="emit('save')"
      @create-type="emit('create-type', $event)"
      @delete-type="emit('delete-type', $event)"
    />
  </div>
</template>
