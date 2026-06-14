<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { Upload } from 'lucide-vue-next'
import { useSessionsStore } from '../stores/sessions'
import { exportSession, importSession } from '../api/client'
import ChatCard from '../components/ChatCard.vue'

const store = useSessionsStore()
const sort = ref<'name-asc' | 'name-desc'>('name-asc')
const importInput = ref<HTMLInputElement | null>(null)

onMounted(() => store.load())

const sortedItems = computed(() => {
  const items = [...store.items]
  items.sort((a, b) => a.name.localeCompare(b.name))
  if (sort.value === 'name-desc') items.reverse()
  return items
})

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
    <div v-if="store.items.length > 0" class="flex items-center justify-between mb-3.5">
      <select v-model="sort" class="field !py-1.5 text-[13px]" aria-label="Sort conversations">
        <option value="name-asc">Name A–Z</option>
        <option value="name-desc">Name Z–A</option>
      </select>
      <button class="btn btn-ghost !py-1.5" title="Import a conversation" @click="importInput?.click()">
        <Upload :size="14" /> Import
      </button>
      <input ref="importInput" type="file" accept="application/json,.json" class="hidden" @change="onImportFile" />
    </div>

    <div class="flex-1 overflow-y-auto">
      <p v-if="store.loading" class="text-muted text-sm">Loading…</p>
      <p v-else-if="store.error" class="text-coral text-sm">{{ store.error }}</p>
      <p v-else-if="store.items.length === 0" class="text-muted text-sm">
        No conversations yet.
      </p>
      <ChatCard
        v-for="s in sortedItems"
        :key="s.id"
        :session="s"
        @duplicate="onDuplicate"
        @export="onExport"
        @delete="onDelete"
      />
    </div>

    <router-link
      to="/new"
      aria-label="New chat"
      class="absolute right-5 bottom-6 block z-20"
    >
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
</template>
