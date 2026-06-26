<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { GripVertical, X } from 'lucide-vue-next'
import { useLibraryStore } from '../stores/library'
import { createSession } from '../api/client'
import EntityPicker from '../components/EntityPicker.vue'
import AvatarPicker from '../components/AvatarPicker.vue'

const router = useRouter()
const library = useLibraryStore()

const name = ref('')
const avatar = ref<string | null>(null)
const selectedTemplateId = ref<string | null>(null)
const mountedPackIds = ref<string[]>([])
const creating = ref(false)
const error = ref<string | null>(null)

onMounted(async () => {
  await library.loadAll()
  if (!selectedTemplateId.value) {
    const def = library.templates.find((t) => (t.meta as Record<string, unknown>)?.default)
    selectedTemplateId.value = def?.id ?? library.templates[0]?.id ?? null
  }
})

const selectedTemplateName = computed(
  () => library.templates.find((t) => t.id === selectedTemplateId.value)?.name ?? '',
)
const mountedPacks = computed(() =>
  mountedPackIds.value
    .map((id) => library.packs.find((p) => p.id === id))
    .filter((p): p is NonNullable<typeof p> => !!p),
)

function selectTemplate(id: string) { selectedTemplateId.value = id || null }
function goAuthor() { router.push('/book') }

function addPack(id: string) {
  if (id && !mountedPackIds.value.includes(id)) mountedPackIds.value = [...mountedPackIds.value, id]
}
function removePack(id: string) {
  mountedPackIds.value = mountedPackIds.value.filter((p) => p !== id)
}

// native HTML5 drag-reorder of the chip list, same pattern as PromptTree:
// a drag only counts if it began on a [data-test="drag-handle"] grip.
const dragId = ref<string | null>(null)
const grabbedHandle = ref(false)
function onMouseDown(e: MouseEvent) {
  grabbedHandle.value = !!(e.target as HTMLElement).closest('[data-test="drag-handle"]')
}
function onDragStart(id: string, e: DragEvent) {
  if (!grabbedHandle.value) { e.preventDefault(); return }
  dragId.value = id
}
function onDrop(targetId: string) {
  const src = dragId.value
  dragId.value = null
  grabbedHandle.value = false
  if (!src || src === targetId) return
  const ids = [...mountedPackIds.value]
  const from = ids.indexOf(src)
  const to = ids.indexOf(targetId)
  if (from === -1 || to === -1) return
  ids.splice(to, 0, ids.splice(from, 1)[0])
  mountedPackIds.value = ids
}

async function createChat() {
  creating.value = true
  error.value = null
  // name falls back to the first mounted pack's name, then "Untitled".
  const finalName = name.value.trim() || mountedPacks.value[0]?.name || ''
  try {
    const session = await createSession(
      finalName || 'Untitled',
      selectedTemplateId.value,
      avatar.value,
      mountedPackIds.value,
    )
    router.push(`/chat/${session.id}`)
  } catch (e) {
    error.value = (e as Error).message
  } finally {
    creating.value = false
  }
}
</script>

<template>
  <div data-test="new-chat" class="pt-6 pb-12 flex flex-col gap-5">
    <h2 class="text-lg font-semibold">{{ $t('newChat.title') }}</h2>

    <!-- name -->
    <input
      v-model="name"
      data-test="chat-name"
      type="text"
      :placeholder="$t('newChat.namePlaceholder')"
      class="field w-full"
    />

    <!-- template -->
    <div>
      <label class="text-[13px] text-muted mb-1.5 block">{{ $t('newChat.template') }}</label>
      <EntityPicker
        data-test="template-picker"
        :items="library.templates.map((t) => ({ id: t.id, name: t.name }))"
        :placeholder="$t('newChat.templatePlaceholder')"
        :create-label="$t('newChat.newTemplate')"
        @select="selectTemplate"
        @create="goAuthor"
      />
      <p v-if="selectedTemplateName" class="text-[12.5px] text-muted mt-1.5">{{ selectedTemplateName }}</p>
    </div>

    <!-- mount packs -->
    <div>
      <label class="text-[13px] text-muted mb-1.5 block">{{ $t('newChat.mountPacks') }}</label>
      <EntityPicker
        data-test="pack-picker"
        :items="library.packs.map((p) => ({ id: p.id, name: p.name }))"
        :placeholder="$t('newChat.mountPlaceholder')"
        :create-label="$t('newChat.newPack')"
        @select="addPack"
        @create="goAuthor"
      />
      <div
        v-if="mountedPacks.length"
        data-test="pack-chips"
        class="flex flex-wrap gap-2 mt-2.5"
        @mousedown="onMouseDown"
      >
        <span
          v-for="p in mountedPacks"
          :key="p.id"
          data-test="pack-chip"
          draggable="true"
          class="inline-flex items-center gap-1.5 pl-1.5 pr-2 py-1 rounded-full bg-card border border-line text-[13px]"
          @dragstart="onDragStart(p.id, $event)"
          @dragover.prevent
          @drop="onDrop(p.id)"
        >
          <span
            data-test="drag-handle"
            class="cursor-grab active:cursor-grabbing text-muted/40 hover:text-muted/70"
            :title="$t('newChat.reorderPack')"
          ><GripVertical :size="13" /></span>
          <span class="text-ink">{{ p.name }}</span>
          <button
            data-test="pack-chip-remove"
            class="text-muted hover:text-coral"
            :title="$t('newChat.removePack')"
            @click="removePack(p.id)"
          ><X :size="13" /></button>
        </span>
      </div>
    </div>

    <!-- avatar override -->
    <div>
      <label class="text-[13px] text-muted mb-2 block">{{ $t('newChat.avatar') }}</label>
      <AvatarPicker @select="avatar = $event" />
    </div>

    <p v-if="error" class="text-coral text-sm">{{ error }}</p>

    <button
      data-test="create-chat"
      :disabled="creating"
      class="w-full py-2.5 rounded-full font-medium bg-primary text-white hover:bg-primary-strong transition-colors disabled:opacity-50"
      @click="createChat"
    >{{ creating ? $t('newChat.creating') : $t('newChat.create') }}</button>
  </div>
</template>
