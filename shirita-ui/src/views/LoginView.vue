<script setup lang="ts">
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Eye, EyeOff, LogIn } from 'lucide-vue-next'
import { useAuthStore } from '../stores/auth'
import logoUrl from '../assets/favicon.svg'

const auth = useAuthStore()
const route = useRoute()
const router = useRouter()

const username = ref('')
const password = ref('')
const showPassword = ref(false)
const failed = ref(false)
const submitting = ref(false)

async function submit() {
  if (!username.value || !password.value) return
  failed.value = false
  submitting.value = true
  try {
    await auth.login(username.value.trim(), password.value)
    const redirect = typeof route.query.redirect === 'string' ? route.query.redirect : '/'
    await router.push(redirect)
  } catch {
    failed.value = true
  } finally {
    submitting.value = false
  }
}
</script>

<template>
  <div class="h-dvh flex items-center justify-center px-4 bg-surface">
    <div class="w-full max-w-sm bg-card rounded-2xl p-8 border border-line">
      <div class="flex flex-col items-center mb-6">
        <img :src="logoUrl" alt="Shirita" class="w-12 h-12 rounded-xl mb-3" />
        <h1 class="text-lg font-medium text-ink">{{ $t('login.title') }}</h1>
        <p class="text-[13px] text-muted mt-1">{{ $t('login.subtitle') }}</p>
      </div>
      <form class="flex flex-col gap-4" @submit.prevent="submit">
        <div>
          <label class="text-[13px] text-ink block mb-1.5" for="login-username">{{ $t('login.username') }}</label>
          <input
            id="login-username"
            v-model="username"
            :placeholder="$t('login.username')"
            class="field w-full"
            autocomplete="username"
            autocapitalize="none"
          />
        </div>
        <div>
          <label class="text-[13px] text-ink block mb-1.5" for="login-password">{{ $t('login.password') }}</label>
          <div class="relative">
            <input
              id="login-password"
              v-model="password"
              :type="showPassword ? 'text' : 'password'"
              :placeholder="$t('login.password')"
              class="field w-full pr-9"
              :class="{ 'tracking-[0.25em]': !showPassword }"
              autocomplete="current-password"
            />
            <button
              type="button"
              class="absolute right-2.5 top-2.5 text-muted hover:text-ink"
              :aria-label="$t('login.togglePassword')"
              @click="showPassword = !showPassword"
            >
              <Eye v-if="!showPassword" :size="16" /><EyeOff v-else :size="16" />
            </button>
          </div>
        </div>
        <p v-if="failed" class="text-[13px] text-red-500">{{ $t('login.error') }}</p>
        <button
          type="submit"
          :disabled="submitting || !username || !password"
          class="inline-flex items-center justify-center gap-2 rounded-lg bg-primary text-white py-2.5 text-sm font-medium transition-colors hover:bg-primary-strong disabled:opacity-50"
        >
          <LogIn :size="16" />
          {{ submitting ? $t('login.signingIn') : $t('login.signIn') }}
        </button>
      </form>
    </div>
  </div>
</template>
