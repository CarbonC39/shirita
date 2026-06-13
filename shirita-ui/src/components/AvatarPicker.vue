<script setup lang="ts">
import { ref } from 'vue'
import { Camera, Upload } from 'lucide-vue-next'

const emit = defineEmits<{ select: [path: string | null] }>()

const isOpen = ref(false)
const selectedPath = ref<string | null>(null)
const library = ref<string[]>([])

function toggle() { isOpen.value = !isOpen.value }
function selectAvatar(path: string | null) { selectedPath.value = path; emit('select', path); isOpen.value = false }
</script>

<template>
  <div class="relative">
    <button type="button" class="relative w-20 h-20 rounded-full bg-sky/20 overflow-hidden group" @click="toggle">
      <img v-if="selectedPath" :src="`/assets/${selectedPath}`" class="w-full h-full object-cover" alt="" />
      <div class="absolute inset-0 bg-black/0 group-hover:bg-black/20 transition-colors flex items-center justify-center">
        <Camera :size="22" class="text-white/80 opacity-0 group-hover:opacity-100 transition-opacity" />
      </div>
    </button>
    <div v-if="isOpen" class="absolute top-full left-0 mt-2 bg-white border border-line rounded-xl shadow-lg p-3 w-64 z-10">
      <div class="flex flex-wrap gap-2 mb-2">
        <div v-for="(avatar, i) in library" :key="i" class="w-12 h-12 rounded-full bg-sky/10 overflow-hidden cursor-pointer hover:ring-2 ring-primary" @click="selectAvatar(avatar)">
          <img :src="`/assets/${avatar}`" class="w-full h-full object-cover" alt="" />
        </div>
      </div>
      <p v-if="library.length === 0" class="text-muted text-xs text-center py-2">No avatars yet. Upload one below.</p>
      <button class="w-full flex items-center justify-center gap-1.5 text-xs text-muted hover:text-ink py-1.5 border border-dashed border-line rounded-lg" @click="selectAvatar(null)">
        <Upload :size="14" /> Upload new
      </button>
    </div>
  </div>
</template>
