<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { Upload, Pencil, Check } from 'lucide-vue-next'
import { useSessionsStore } from '../stores/sessions'
import { exportSession, importSession } from '../api/client'
import type { Session } from '../api/types'
import ChatCard from '../components/ChatCard.vue'

const store = useSessionsStore()
const importInput = ref<HTMLInputElement | null>(null)
const editMode = ref(false)

// Local working copy so a drag can reorder fluidly; the server order (recency
// by default) is the source of truth and resyncs whenever the store changes.
const items = ref<Session[]>([])
watch(() => store.items, (v) => { items.value = [...v] }, { immediate: true })

onMounted(() => store.load())

let dragFrom = -1
function onDragStart(i: number, e: DragEvent) {
  dragFrom = i
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move'
}
function onDragOver(i: number, e: DragEvent) {
  e.preventDefault()
  if (dragFrom === -1 || dragFrom === i) return
  const arr = [...items.value]
  const [moved] = arr.splice(dragFrom, 1)
  arr.splice(i, 0, moved)
  items.value = arr
  dragFrom = i
}
function onDrop() {
  if (dragFrom === -1) return
  dragFrom = -1
  store.reorder(items.value.map((s) => s.id))
}

async function onDuplicate(id: string) { await store.duplicate(id) }

async function onDelete(id: string) {
  if (!confirm('Delete this conversation and all its messages?')) return
  await store.remove(id)
}

async function onExport(id: string) {
  try {
    const data = await exportSession(id)
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `shirita-chat-${id}.json`
    a.click()
    URL.revokeObjectURL(url)
  } catch (e) { store.error = (e as Error).message }
}

async function onImportFile(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  try {
    const body = JSON.parse(await file.text())
    await importSession(body)
    await store.load()
  } catch (err) { store.error = (err as Error).message }
  finally { if (importInput.value) importInput.value.value = '' }
}
</script>

<template>
  <div class="relative max-w-[560px] mx-auto px-5 pt-7 pb-8 h-full flex flex-col">
    <div class="flex-1 overflow-y-auto">
      <p v-if="store.loading" class="text-muted text-sm">Loading…</p>
      <p v-else-if="store.error" class="text-coral text-sm">{{ store.error }}</p>
      <p v-else-if="store.items.length === 0" class="text-muted text-sm">
        No conversations yet.
      </p>
      <ChatCard
        v-for="(s, i) in items"
        :key="s.id"
        :session="s"
        :edit-mode="editMode"
        :draggable="editMode"
        class="reveal"
        :style="{ animationDelay: `${Math.min(i, 8) * 45}ms` }"
        @dragstart="onDragStart(i, $event)"
        @dragover="onDragOver(i, $event)"
        @drop="onDrop"
        @duplicate="onDuplicate"
        @export="onExport"
        @delete="onDelete"
      />
    </div>

    <!-- Edit + Import are flat secondary actions beside the new-chat button -->
    <div class="absolute right-5 bottom-6 flex items-center gap-2 z-20">
      <button
        v-if="store.items.length > 0"
        data-test="edit-toggle"
        :class="['flex items-center gap-1.5 px-3 py-1.5 rounded-full text-[13px] font-medium backdrop-blur-sm transition-colors',
                 editMode ? 'bg-primary text-white' : 'bg-card/80 text-muted hover:text-ink']"
        :title="editMode ? 'Done' : 'Reorder & delete'"
        @click="editMode = !editMode"
      >
        <component :is="editMode ? Check : Pencil" :size="15" /> {{ editMode ? 'Done' : 'Edit' }}
      </button>
      <button
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-full text-[13px] font-medium bg-card/80 text-muted hover:text-ink backdrop-blur-sm transition-colors"
        title="Import a conversation"
        @click="importInput?.click()"
      >
        <Upload :size="15" /> Import
      </button>
      <input ref="importInput" type="file" accept="application/json,.json" class="hidden" @change="onImportFile" />

      <router-link to="/new" aria-label="New chat" class="block ml-1">
        <svg
          width="54"
          height="54"
          viewBox="0 0 24 24"
          style="transform: scaleX(-1); filter: drop-shadow(0 7px 16px rgba(0, 0, 0, 0.18))"
        >
          <path fill="var(--color-primary)" d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" />
          <line x1="8" y1="12" x2="16" y2="12" stroke="#fff" stroke-width="2.2" stroke-linecap="round" />
          <line x1="12" y1="8" x2="12" y2="16" stroke="#fff" stroke-width="2.2" stroke-linecap="round" />
        </svg>
      </router-link>
    </div>
  </div>
</template>
