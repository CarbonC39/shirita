<script setup lang="ts">
import { ref, computed } from 'vue'
import { ArrowUp, Plus, X } from 'lucide-vue-next'
import { estimateTokens, formatTokens } from '../utils/tokens'
import { uploadAsset, type Asset } from '../api/client'

const props = defineProps<{ disabled: boolean }>()

const emit = defineEmits<{
  send: [text: string, attachments: string[]]
}>()

const text = ref('')
const pending = ref<Asset[]>([])
const uploading = ref(false)
const fileInput = ref<HTMLInputElement | null>(null)
const hasText = computed(() => text.value.trim().length > 0)
const canSend = computed(() => hasText.value || pending.value.length > 0)
const draftTokens = computed(() => estimateTokens(text.value))

function pickFile() {
  fileInput.value?.click()
}

async function onFile(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  uploading.value = true
  try {
    const asset = await uploadAsset(file)
    pending.value.push(asset)
  } finally {
    uploading.value = false
    if (fileInput.value) fileInput.value.value = ''
  }
}

function removePending(id: string) {
  pending.value = pending.value.filter((a) => a.id !== id)
}

function submit() {
  const trimmed = text.value.trim()
  if (!canSend.value || props.disabled) return
  emit('send', trimmed, pending.value.map((a) => a.id))
  text.value = ''
  pending.value = []
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    submit()
  }
}
</script>

<template>
  <div class="app-composer border-t border-line bg-card px-4 py-3">
    <div v-if="pending.length" class="max-w-[600px] mx-auto pl-[46px] pr-[50px] pb-2 flex flex-wrap gap-2">
      <div v-for="a in pending" :key="a.id" class="relative w-14 h-14 rounded-lg overflow-hidden border border-line">
        <img :src="a.url" class="w-full h-full object-cover" alt="" />
        <button
          type="button"
          class="absolute -top-1 -right-1 w-4 h-4 rounded-full bg-card border border-line text-muted hover:text-coral grid place-items-center"
          :title="$t('composer.removeAttachment')"
          @click="removePending(a.id)"
        >
          <X :size="10" />
        </button>
      </div>
    </div>
    <div class="max-w-[600px] mx-auto flex items-end gap-2.5">
      <button
        type="button"
        class="text-muted hover:text-ink p-1.5 shrink-0 mb-0.5 disabled:opacity-50"
        :disabled="uploading"
        :title="$t('composer.attach')"
        @click="pickFile"
      >
        <Plus :size="20" />
      </button>
      <input ref="fileInput" type="file" accept="image/*" class="hidden" @change="onFile" />
      <textarea
        v-model="text"
        :disabled="disabled"
        rows="1"
        :placeholder="$t('composer.placeholder')"
        class="flex-1 resize-none rounded-xl border border-line px-3.5 py-2.5 text-[15px] leading-relaxed
               focus:outline-none focus:border-primary/50 placeholder:text-muted/60
               disabled:bg-surface disabled:text-muted/50"
        @keydown="onKeydown"
      />
      <button
        data-test="send-btn"
        :disabled="disabled || !canSend"
        :class="[
          'w-10 h-10 rounded-full flex items-center justify-center shrink-0 transition-colors',
          canSend && !disabled ? 'bg-primary text-white' : 'bg-line text-muted',
        ]"
        @click="submit"
      >
        <ArrowUp :size="18" />
      </button>
    </div>
    <div v-if="hasText" class="max-w-[600px] mx-auto pl-[46px] pr-[50px] pt-1">
      <span class="text-[11px] text-muted tabular-nums">{{ $t('common.tokensEstimate', { tokens: formatTokens(draftTokens) }, draftTokens) }}</span>
    </div>
  </div>
</template>
