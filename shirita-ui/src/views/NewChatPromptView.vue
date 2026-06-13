<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useLibraryStore } from '../stores/library'
import { createSession, createNode, updateNode, listNodes } from '../api/client'
import PromptTree from '../components/PromptTree.vue'
import type { PromptNode } from '../api/types'

const route = useRoute()
const router = useRouter()
const library = useLibraryStore()

const sessionName = (route.query.name as string) || 'Untitled'
const selectedTemplateId = ref<string | null>(null)
const nodes = ref<PromptNode[]>([])
const creating = ref(false)
const error = ref<string | null>(null)

onMounted(async () => { await library.loadAll() })

async function selectTemplate(templateId: string) {
  selectedTemplateId.value = templateId
  try { nodes.value = await listNodes('template', templateId) } catch { nodes.value = [] }
}

async function handleAddNode(parentId: string | null, definitionId: string) {
  if (!selectedTemplateId.value) return
  try {
    const node = await createNode('template', selectedTemplateId.value, { parent_id: parentId, kind: 'ref', definition_id: definitionId })
    nodes.value = [...nodes.value, node]
  } catch (e) { error.value = (e as Error).message }
}

async function handleToggleEnabled(nodeId: string) {
  const node = nodes.value.find(n => n.id === nodeId)
  if (!node) return
  try {
    const updated = await updateNode(nodeId, { enabled: !node.enabled })
    const idx = nodes.value.findIndex(n => n.id === nodeId)
    if (idx !== -1) nodes.value = [...nodes.value.slice(0, idx), updated, ...nodes.value.slice(idx + 1)]
  } catch (e) { error.value = (e as Error).message }
}

async function createChat() {
  creating.value = true; error.value = null
  try {
    const session = await createSession(sessionName, selectedTemplateId.value)
    router.push(`/chat/${session.id}`)
  } catch (e) { error.value = (e as Error).message }
  finally { creating.value = false }
}
</script>

<template>
  <div class="max-w-[560px] mx-auto px-5 pt-8 pb-12">
    <div class="flex items-center gap-1.5 text-[13px] text-muted mb-6">
      <router-link to="/" class="hover:text-ink">Chat</router-link> <span>/</span>
      <router-link to="/new" class="hover:text-ink">New</router-link> <span>/</span>
      <span class="text-ink">Prompt</span>
    </div>
    <h2 class="text-lg font-semibold mb-1">{{ sessionName }}</h2>
    <p class="text-[13px] text-muted mb-6">Choose a prompt template and configure the tree.</p>
    <div class="mb-4">
      <label class="text-[13px] text-muted mb-1.5 block">Template</label>
      <select :value="selectedTemplateId" class="w-full border border-line rounded-lg px-3 py-2 text-[14px] bg-white outline-none focus:border-primary/50" @change="selectTemplate(($event.target as HTMLSelectElement).value)">
        <option :value="null">None (start empty)</option>
        <option v-for="t in library.templates" :key="t.id" :value="t.id">{{ t.name }}</option>
      </select>
    </div>
    <PromptTree v-if="selectedTemplateId" :nodes="nodes" :definitions="library.definitions" @add-node="handleAddNode" @toggle-enabled="handleToggleEnabled" />
    <p v-if="error" class="text-coral text-sm mt-3">{{ error }}</p>
    <div class="mt-8">
      <button :disabled="creating" class="w-full py-2.5 rounded-full font-medium bg-primary text-white hover:bg-primary-strong transition-colors disabled:opacity-50" @click="createChat">{{ creating ? 'Creating…' : 'Create conversation' }}</button>
    </div>
  </div>
</template>
