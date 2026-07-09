<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { Pencil, Download, Upload, Copy, Trash2, Star, X, Check } from 'lucide-vue-next'
import EntityPicker from './EntityPicker.vue'

const { t } = useI18n()

const props = withDefaults(
  defineProps<{
    items: { id: string; name: string; avatar?: string | null }[]
    placeholder?: string
    createLabel?: string
    selectedLabel?: string
    selectedId?: string | null
    entityName?: string
    showDefault?: boolean
    isDefault?: boolean
    showRename?: boolean
    showImport?: boolean
    showExport?: boolean
    showDuplicate?: boolean
    showDelete?: boolean
  }>(),
  {
    showDefault: false,
    isDefault: false,
    showRename: true,
    showImport: true,
    showExport: true,
    showDuplicate: true,
    showDelete: true,
  },
)

const emit = defineEmits<{
  select: [id: string]
  create: [name: string]
  cancelCreate: []
  rename: [name: string]
  import: []
  export: []
  duplicate: []
  delete: []
  'toggle-default': []
}>()

// ── creation mode ──
const creating = ref(false)
const createName = ref('')
const createInput = ref<HTMLInputElement | null>(null)

function onIntentCreate(draft: string) {
  createName.value = draft
  creating.value = true
  nextTick(() => createInput.value?.focus())
}

function confirmCreate() {
  const name = createName.value.trim()
  if (!name) return
  emit('create', name)
  creating.value = false
  createName.value = ''
}

function cancelCreate() {
  creating.value = false
  createName.value = ''
}

// ── rename mode ──
const renaming = ref(false)
const renameName = ref('')

function startRename() {
  renameName.value = props.entityName ?? ''
  renaming.value = true
  nextTick(() => {
    const el = document.querySelector<HTMLInputElement>('[data-test="rename-input"]')
    el?.focus()
  })
}

function confirmRename() {
  const name = renameName.value.trim()
  if (!name) { renaming.value = false; return }
  emit('rename', name)
  renaming.value = false
}

function cancelRename() {
  renaming.value = false
}
</script>

<template>
  <div data-test="entity-toolbar">
    <!-- Rename mode -->
    <div v-if="renaming" class="flex items-center gap-2">
      <input
        v-model="renameName"
        type="text"
        data-test="rename-input"
        class="flex-1 bg-transparent border border-line rounded-lg px-3 py-2 text-[14px] text-ink placeholder:text-muted/60 outline-none focus:border-primary/50"
        @keydown.enter="confirmRename"
        @keydown.escape="cancelRename"
      />
      <button
        class="px-3 py-1.5 text-[13px] bg-primary text-white rounded-lg hover:bg-primary-strong"
        @click="confirmRename"
      >{{ $t('common.done') }}</button>
      <button class="text-muted hover:text-ink" @click="cancelRename"><X :size="18" /></button>
    </div>

    <!-- Normal mode -->
    <template v-else-if="!creating">
      <div class="flex items-center gap-2 flex-wrap">
        <div class="flex-1 min-w-[180px]">
          <EntityPicker
            :items="items"
            :placeholder="placeholder"
            :create-label="createLabel"
            :selected-label="selectedLabel"
            @select="emit('select', $event)"
            @intent-create="onIntentCreate"
          />
        </div>
        <div class="flex items-center flex-wrap">
          <button
            v-if="showDefault"
            class="w-[33px] h-[33px] grid place-items-center rounded-lg disabled:opacity-40"
            :class="isDefault ? 'text-amber-500' : 'text-muted hover:text-ink'"
            :title="t('book.defaultTemplate')"
            :disabled="!selectedId"
            @click="emit('toggle-default')"
          >
            <Star :size="15" :fill="isDefault ? 'currentColor' : 'none'" />
          </button>
          <button
            v-if="showRename"
            class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-30 disabled:pointer-events-none"
            :title="t('common.rename')"
            :disabled="!selectedId"
            @click="startRename"
          >
            <Pencil :size="15" />
          </button>
          <button
            v-if="showImport"
            class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg"
            :title="t('common.import')"
            @click="emit('import')"
          >
            <Upload :size="16" />
          </button>
          <button
            v-if="showExport"
            class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-30 disabled:pointer-events-none"
            :title="t('common.export')"
            :disabled="!selectedId"
            @click="emit('export')"
          >
            <Download :size="16" />
          </button>
          <button
            v-if="showDuplicate"
            class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-30 disabled:pointer-events-none"
            :title="t('common.duplicate')"
            :disabled="!selectedId"
            @click="emit('duplicate')"
          >
            <Copy :size="16" />
          </button>
          <button
            v-if="showDelete"
            class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-coral rounded-lg disabled:opacity-30 disabled:pointer-events-none"
            :title="t('common.delete')"
            :disabled="!selectedId"
            @click="emit('delete')"
          >
            <Trash2 :size="16" />
          </button>
        </div>
      </div>
    </template>

    <!-- Creation mode: text input + ❌ / ✔ -->
    <div v-else class="flex items-center gap-2">
      <input
        ref="createInput"
        v-model="createName"
        type="text"
        data-test="create-name-input"
        :placeholder="createLabel"
        class="flex-1 bg-transparent border border-line rounded-lg px-3 py-2 text-[14px] text-ink placeholder:text-muted/60 outline-none focus:border-primary/50"
        @keydown.enter="confirmCreate"
        @keydown.escape="cancelCreate"
      />
      <button
        data-test="create-cancel"
        class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg"
        :title="t('common.cancel')"
        @click="cancelCreate"
      >
        <X :size="18" />
      </button>
      <button
        data-test="create-confirm"
        class="w-[33px] h-[33px] grid place-items-center rounded-lg"
        :class="createName.trim() ? 'text-primary hover:text-emerald-400' : 'text-muted opacity-30 pointer-events-none'"
        :title="t('common.save')"
        @click="confirmCreate"
      >
        <Check :size="20" />
      </button>
    </div>
  </div>
</template>
