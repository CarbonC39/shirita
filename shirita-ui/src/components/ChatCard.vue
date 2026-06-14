<script setup lang="ts">
import { ref, computed } from 'vue'
import { MoreVertical, Copy, Download, Trash2 } from 'lucide-vue-next'
import type { Session } from '../api/types'

const props = defineProps<{ session: Session }>()
const emit = defineEmits<{ duplicate: [id: string]; export: [id: string]; delete: [id: string] }>()

const menuOpen = ref(false)

// Sparing palette accent: derive a tint from the id so the list reads as
// colourful rather than monochrome.
const tints = ['bg-sky/30', 'bg-coral/30', 'bg-mauve/25', 'bg-primary/15']
const tint = computed(() => {
  let h = 0
  for (const c of props.session.id) h = (h * 31 + c.charCodeAt(0)) >>> 0
  return tints[h % tints.length]
})

function act(e: Event, fn: () => void) { e.stopPropagation(); e.preventDefault(); menuOpen.value = false; fn() }
</script>

<template>
  <router-link
    :to="`/chat/${session.id}`"
    class="relative flex items-center gap-3.5 bg-white border border-line rounded-2xl px-4 py-3.5 mb-3 hover:border-primary/30 transition-colors"
  >
    <div :class="['w-11 h-11 rounded-full shrink-0 overflow-hidden grid place-items-center', tint]">
      <img v-if="session.avatar" :src="`/assets/${session.avatar}`" class="w-full h-full object-cover" alt="" />
    </div>
    <div class="flex-1 min-w-0">
      <div class="font-semibold text-ink truncate">{{ session.name }}</div>
      <div class="text-[13px] text-muted truncate">Tap to open</div>
    </div>

    <button data-test="chat-menu" class="text-muted/50 hover:text-ink p-1 -mr-1 shrink-0 transition-colors" title="Options" @click.stop.prevent="menuOpen = !menuOpen">
      <MoreVertical :size="18" />
    </button>

    <!-- click-away + dropdown -->
    <div v-if="menuOpen" class="fixed inset-0 z-20" @click.stop.prevent="menuOpen = false" />
    <div v-if="menuOpen" class="absolute right-3 top-12 z-30 bg-white border border-line rounded-xl shadow-lg overflow-hidden min-w-[150px]">
      <button class="w-full flex items-center gap-2 px-3 py-2 text-[13px] text-ink hover:bg-surface text-left transition-colors" @click="act($event, () => emit('duplicate', session.id))"><Copy :size="14" /> Duplicate</button>
      <button class="w-full flex items-center gap-2 px-3 py-2 text-[13px] text-ink hover:bg-surface text-left transition-colors" @click="act($event, () => emit('export', session.id))"><Download :size="14" /> Export</button>
      <button data-test="chat-delete" class="w-full flex items-center gap-2 px-3 py-2 text-[13px] text-coral hover:bg-surface text-left transition-colors" @click="act($event, () => emit('delete', session.id))"><Trash2 :size="14" /> Delete</button>
    </div>
  </router-link>
</template>
