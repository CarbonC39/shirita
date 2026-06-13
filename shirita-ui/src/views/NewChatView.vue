<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import AvatarPicker from '../components/AvatarPicker.vue'

const router = useRouter()
const name = ref('')
const avatar = ref<string | null>(null)
const canProceed = computed(() => name.value.trim().length > 0)

function proceed() {
  const params: Record<string, string> = {}
  if (name.value.trim()) params.name = name.value.trim()
  if (avatar.value) params.avatar = avatar.value
  router.push({ path: '/new/prompt', query: params })
}
</script>

<template>
  <div class="max-w-[480px] mx-auto px-5 pt-10">
    <div class="flex items-center gap-1.5 text-[13px] text-muted mb-8">
      <router-link to="/" class="hover:text-ink">Chat</router-link> <span>/</span> <span class="text-ink">New</span>
    </div>
    <div class="flex flex-col items-center gap-6">
      <AvatarPicker @select="avatar = $event" />
      <div class="w-full">
        <input v-model="name" type="text" placeholder="Name" class="w-full text-center text-xl font-semibold bg-transparent border-b-2 border-line focus:border-primary outline-none pb-2 placeholder:text-muted/50" @keydown.enter="proceed()" />
      </div>
      <button :class="['px-8 py-2.5 rounded-full font-medium text-[15px] transition-colors', canProceed ? 'bg-primary text-white hover:bg-primary-strong' : 'bg-line text-muted']" @click="proceed()">{{ canProceed ? 'Next' : 'Skip' }}</button>
    </div>
  </div>
</template>
