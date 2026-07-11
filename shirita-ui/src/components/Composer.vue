<script setup lang="ts">
import { ref, computed, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowUp, Plus, X, Square } from 'lucide-vue-next'
import { estimateTokens, formatTokens } from '../utils/tokens'
import { uploadAsset, type Asset } from '../api/client'
import { useToast } from '../composables/useToast'

const { t } = useI18n()
const { show: showToast } = useToast()

const props = defineProps<{ disabled: boolean; streaming?: boolean }>()

const emit = defineEmits<{
  send: [text: string, attachments: string[]]
  stop: []
}>()

const text = ref('')
const textarea = ref<HTMLTextAreaElement | null>(null)
const pending = ref<Asset[]>([])
const uploading = ref(false)
const fileInput = ref<HTMLInputElement | null>(null)
const hasText = computed(() => text.value.trim().length > 0)
const canSend = computed(() => hasText.value || pending.value.length > 0)
const draftTokens = computed(() => estimateTokens(text.value))

// Auto-grow the textarea to fit its content up to a max height (~7 rows).
function autosize() {
  const el = textarea.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = `${Math.min(el.scrollHeight, 200)}px`
}

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
  } catch {
    // Was swallowed (try/finally with no catch): a failed upload left the user
    // with no thumbnail and no idea why. Surface it.
    showToast(t('composer.uploadFailed'), 'error')
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
  nextTick(autosize)
}

// Called by ChatView when an HTML card posts content via postMessage.
function setText(val: string) {
  text.value = val
  nextTick(autosize)
}

defineExpose({ setText })

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    submit()
  }
}
</script>

<template>
  <div class="app-composer border-t border-line bg-card px-1.5 sm:px-4 pt-2.5 pb-[calc(env(safe-area-inset-bottom)+0.625rem)]">
    <div v-if="pending.length" class="mx-auto w-full max-w-[820px] pl-[42px] pr-[46px] pb-2 flex flex-wrap gap-2">
      <div v-for="a in pending" :key="a.id" class="relative w-14 h-14 rounded-lg overflow-hidden border border-line">
        <img :src="a.url" class="w-full h-full object-cover" alt="" />
        <button
          type="button"
          class="absolute -top-1.5 -right-1.5 w-6 h-6 rounded-full bg-card border border-line text-muted hover:text-coral grid place-items-center"
          :aria-label="$t('composer.removeAttachment')"
          :title="$t('composer.removeAttachment')"
          @click="removePending(a.id)"
        >
          <X :size="12" />
        </button>
      </div>
    </div>
    <div class="mx-auto w-full max-w-[820px] flex items-end gap-2">
      <button
        type="button"
        class="text-muted hover:text-ink p-1.5 shrink-0 mb-0.5 disabled:opacity-50"
        :disabled="uploading"
        :aria-label="$t('composer.attach')"
        :title="$t('composer.attach')"
        @click="pickFile"
      >
        <Plus :size="20" />
      </button>
      <input ref="fileInput" type="file" accept="image/*" class="hidden" @change="onFile" />
      <textarea
        ref="textarea"
        v-model="text"
        :disabled="disabled"
        rows="1"
        :placeholder="$t('composer.placeholder')"
        class="flex-1 resize-none rounded-xl border border-line px-3.5 py-2.5 text-[15px] leading-relaxed
               focus:outline-none focus:border-primary/50 placeholder:text-muted/60
               disabled:bg-surface disabled:text-muted/50 max-h-[200px]"
        @keydown="onKeydown"
        @input="autosize"
      />
      <button
        v-if="streaming"
        data-test="stop-btn"
        :aria-label="$t('composer.stop')"
        :title="$t('composer.stop')"
        class="w-10 h-10 rounded-full flex items-center justify-center shrink-0 bg-coral text-white hover:brightness-110 transition"
        @click="emit('stop')"
      >
        <Square :size="14" fill="currentColor" />
      </button>
      <button
        v-else
        data-test="send-btn"
        :disabled="disabled || !canSend"
        :aria-label="$t('composer.send')"
        :class="[
          'w-10 h-10 rounded-full flex items-center justify-center shrink-0 transition-colors',
          canSend && !disabled ? 'bg-primary text-white' : 'bg-line text-muted',
        ]"
        @click="submit"
      >
        <ArrowUp :size="18" />
      </button>
    </div>
    <div class="mx-auto w-full max-w-[820px] pl-[42px] pr-[46px] pt-1 h-[18px]">
      <span v-if="hasText" class="text-[11px] text-muted tabular-nums">{{ $t('common.tokensEstimate', { tokens: formatTokens(draftTokens) }, draftTokens) }}</span>
    </div>
  </div>
</template>
