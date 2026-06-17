<script setup lang="ts">
import { ref } from 'vue'
import { Camera, User } from 'lucide-vue-next'
import AssetPicker from './AssetPicker.vue'

const emit = defineEmits<{ select: [path: string | null] }>()

const isOpen = ref(false)
const selectedPath = ref<string>('')

function toggle() { isOpen.value = !isOpen.value }
function onSelect(path: string) {
  selectedPath.value = path
  emit('select', path || null)
}
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

    <!-- inline shared media library, expands below -->
    <div v-if="isOpen" class="w-full mt-4 bg-card border border-line rounded-xl p-3.5">
      <p class="text-[12px] text-muted mb-3">{{ $t('common.avatarPickHint') }}</p>
      <AssetPicker :model-value="selectedPath" shape="circle" @update:model-value="onSelect" />
    </div>
  </div>
</template>
