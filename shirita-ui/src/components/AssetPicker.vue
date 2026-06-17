<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Upload, X, ImageOff } from 'lucide-vue-next'
import { useMediaStore } from '../stores/media'

const { t } = useI18n()

const props = withDefaults(
  defineProps<{ modelValue: string; shape?: 'rect' | 'circle'; clearable?: boolean }>(),
  { shape: 'rect', clearable: true },
)
const emit = defineEmits<{ 'update:modelValue': [path: string] }>()

const media = useMediaStore()
const fileInput = ref<HTMLInputElement | null>(null)
const uploading = ref(false)

onMounted(() => media.load())

function pick() { fileInput.value?.click() }
async function onFile(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  uploading.value = true
  try {
    const a = await media.upload(file)
    if (a) emit('update:modelValue', a.path) // newly uploaded becomes the selection
  } finally {
    uploading.value = false
    if (fileInput.value) fileInput.value.value = ''
  }
}

async function onDelete(id: string, path: string) {
  if (!confirm(t('common.imageDeleteConfirm'))) return
  await media.remove(id)
  if (props.modelValue === path) emit('update:modelValue', '') // clear if it was selected
}
</script>

<template>
  <div>
    <div class="flex flex-wrap gap-3">
      <!-- none / clear -->
      <button
        v-if="clearable"
        type="button"
        :class="[shape === 'circle' ? 'w-[60px] h-[60px] rounded-full' : 'w-[92px] h-[58px] rounded-lg',
                 'border grid place-items-center text-muted shrink-0 transition-colors',
                 modelValue === '' ? 'border-primary ring-2 ring-primary/30' : 'border-line hover:border-primary/40']"
        :title="$t('common.imageNone')"
        @click="emit('update:modelValue', '')"
      >
        <ImageOff :size="16" />
      </button>

      <!-- library entries -->
      <div v-for="a in media.assets" :key="a.id" :class="shape === 'circle' ? 'w-[60px]' : 'w-[92px]'">
        <div class="relative group">
          <button
            type="button"
            :class="[shape === 'circle' ? 'w-[60px] h-[60px] rounded-full' : 'w-[92px] h-[58px] rounded-lg',
                     'overflow-hidden border shrink-0 transition-colors',
                     modelValue === a.path ? 'border-primary ring-2 ring-primary/40' : 'border-line hover:border-primary/40']"
            :title="a.name"
            @click="emit('update:modelValue', a.path)"
          >
            <img :src="a.url" class="w-full h-full object-cover" alt="" />
          </button>
          <button
            type="button"
            class="absolute -top-1.5 -right-1.5 w-5 h-5 rounded-full bg-card border border-line text-muted hover:text-coral grid place-items-center opacity-0 group-hover:opacity-100 transition-opacity"
            :title="$t('common.imageDelete')"
            @click.stop="onDelete(a.id, a.path)"
          >
            <X :size="12" />
          </button>
        </div>
        <input
          :value="a.name"
          class="mt-1 w-full bg-transparent text-[11px] text-muted text-center outline-none focus:text-ink truncate"
          :aria-label="$t('common.imageName')"
          @change="media.rename(a.id, ($event.target as HTMLInputElement).value.trim() || a.name)"
        />
      </div>

      <!-- upload -->
      <button
        type="button"
        :class="[shape === 'circle' ? 'w-[60px] h-[60px] rounded-full' : 'w-[92px] h-[58px] rounded-lg',
                 'border-[1.5px] border-dashed border-line grid place-items-center text-muted hover:text-ink hover:border-muted shrink-0 transition-colors']"
        :title="uploading ? $t('common.uploading') : $t('common.imageUpload')"
        @click="pick"
      >
        <Upload :size="16" :class="uploading ? 'animate-pulse' : ''" />
      </button>
      <input ref="fileInput" type="file" accept="image/*" class="hidden" @change="onFile" />
    </div>
    <p v-if="media.error" class="text-[12px] text-coral mt-2">{{ media.error }}</p>
  </div>
</template>
