<script setup lang="ts">
import type { Pack } from '../../api/types'
import EntityToolbar from '../EntityToolbar.vue'
import PackEditor from '../PackEditor.vue'

defineProps<{
  items: { id: string; name: string }[]
  selectedId: string | null
  selectedLabel: string
  placeholder: string
  createLabel: string
  selectedPack: Pack | null
}>()

const emit = defineEmits<{
  select: [id: string]
  create: [name: string]
  rename: [name: string]
  import: []
  export: []
  duplicate: []
  delete: []
  packChanged: []
}>()
</script>

<template>
  <div class="rounded-2xl bg-teal/5 border border-line/60 p-4 mb-4">
    <h2 data-test="section-pack" class="flex items-center text-[12px] font-semibold uppercase tracking-wide text-teal border-l-2 border-teal pl-2 mb-3">
      {{ $t('book.packHeading') }}
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
    <div class="mb-3" />
    <PackEditor
      v-if="selectedPack"
      :pack="selectedPack"
      @changed="emit('packChanged')"
    />
  </div>
</template>
