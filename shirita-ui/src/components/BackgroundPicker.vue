<script setup lang="ts">
import { ref } from 'vue'
import { ImagePlus, X } from 'lucide-vue-next'
import { uploadAsset } from '../api/client'

defineProps<{ modelValue: string }>()
const emit = defineEmits<{ 'update:modelValue': [path: string] }>()

const fileInput = ref<HTMLInputElement | null>(null)
const uploading = ref(false)
const error = ref('')

function pick() { fileInput.value?.click() }
async function onFile(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  uploading.value = true; error.value = ''
  try { const { path } = await uploadAsset(file); emit('update:modelValue', path) }
  catch (err) { error.value = (err as Error).message }
  finally { uploading.value = false; if (fileInput.value) fileInput.value.value = '' }
}
</script>

<template>
  <div class="flex items-center gap-3">
    <button
      type="button"
      class="w-[88px] h-[52px] rounded-lg border border-line overflow-hidden bg-surface grid place-items-center shrink-0 hover:border-primary/50 transition-colors"
      title="Upload background"
      @click="pick"
    >
      <img v-if="modelValue" :src="`/assets/${modelValue}`" class="w-full h-full object-cover" alt="" />
      <ImagePlus v-else :size="18" class="text-muted" />
    </button>
    <div class="flex flex-col gap-1">
      <div class="flex items-center gap-2">
        <button class="btn btn-ghost !py-1" :disabled="uploading" @click="pick">
          {{ uploading ? 'Uploading…' : (modelValue ? 'Change' : 'Upload') }}
        </button>
        <button v-if="modelValue" class="inline-flex items-center gap-1 text-[12px] text-muted hover:text-coral transition-colors" @click="emit('update:modelValue', '')">
          <X :size="13" /> Remove
        </button>
      </div>
      <p v-if="error" class="text-[12px] text-coral">{{ error }}</p>
    </div>
    <input ref="fileInput" type="file" accept="image/*" class="hidden" @change="onFile" />
  </div>
</template>
