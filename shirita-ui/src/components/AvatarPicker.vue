<script setup lang="ts">
import { ref } from 'vue'
import { Camera, Upload, User } from 'lucide-vue-next'

const emit = defineEmits<{ select: [path: string | null] }>()

const isOpen = ref(false)
const selectedPath = ref<string | null>(null)
const library = ref<string[]>([])

function toggle() { isOpen.value = !isOpen.value }
function selectAvatar(path: string | null) { selectedPath.value = path; emit('select', path); isOpen.value = false }
</script>

<template>
  <div class="w-full flex flex-col items-center">
    <!-- avatar with persistent camera badge -->
    <button type="button" class="relative w-[92px] h-[92px] cursor-pointer" @click="toggle">
      <span class="block w-[92px] h-[92px] rounded-full bg-line/40 border border-line overflow-hidden grid place-items-center">
        <img v-if="selectedPath" :src="`/assets/${selectedPath}`" class="w-full h-full object-cover" alt="" />
        <User v-else :size="36" class="text-muted" :stroke-width="1.7" />
      </span>
      <span class="absolute right-0 bottom-0 w-7 h-7 rounded-full bg-primary border-[2.5px] border-surface grid place-items-center">
        <Camera :size="14" class="text-white" :stroke-width="2" />
      </span>
    </button>

    <!-- inline avatar library, expands below -->
    <div v-if="isOpen" class="w-full mt-4 bg-white border border-line rounded-xl p-3.5">
      <p class="text-[12px] text-muted mb-3">Avatar library · pick an existing one, or upload new</p>
      <div class="flex flex-wrap gap-3.5">
        <button
          v-for="(avatar, i) in library"
          :key="i"
          type="button"
          :class="['w-[46px] h-[46px] rounded-full overflow-hidden', selectedPath === avatar ? 'ring-2 ring-primary ring-offset-2 ring-offset-white' : '']"
          @click="selectAvatar(avatar)"
        >
          <img :src="`/assets/${avatar}`" class="w-full h-full object-cover" alt="" />
        </button>
        <button
          type="button"
          class="w-[46px] h-[46px] rounded-full border-[1.5px] border-dashed border-line grid place-items-center text-muted hover:text-ink hover:border-muted transition-colors"
          title="Upload new"
          @click="selectAvatar(null)"
        >
          <Upload :size="18" />
        </button>
      </div>
      <p v-if="library.length === 0" class="text-muted/70 text-[11.5px] mt-2.5">No avatars yet — upload your first one.</p>
    </div>
  </div>
</template>
