import { defineStore } from 'pinia'
import { ref } from 'vue'
import { authChangePassword, authLogin, authLogout, authMe, type AuthUser } from '../api/client'
import { router } from '../router'

const TOKEN_KEY = 'auth.token'
const RT = (globalThis as { __SHIRITA_RUNTIME__?: { token?: string } }).__SHIRITA_RUNTIME__

export const useAuthStore = defineStore('auth', () => {
  const token = ref<string | null>(null)
  const user = ref<AuthUser | null>(null)
  // `restore()` has settled — the /me validation is done. Lets the app tell
  // "still booting" from "definitely logged out" if needed.
  const ready = ref(false)

  function persist(t: string | null) {
    token.value = t
    if (t) localStorage.setItem(TOKEN_KEY, t)
    else localStorage.removeItem(TOKEN_KEY)
  }

  /** Clear the session and send the user to the login screen, preserving where
   *  they were so login can return there. Idempotent — safe to call from the
   *  apiFetch 401 handler and from `restore()` alike. */
  function clearAndRedirect() {
    persist(null)
    user.value = null
    const current = router.currentRoute.value
    if (current.name !== 'login') {
      const query = current.fullPath && current.fullPath !== '/' ? { redirect: current.fullPath } : undefined
      void router.push({ name: 'login', query })
    }
  }

  /** Load any persisted (or Tauri-injected) token, then validate it via /me.
   *  Sets the token synchronously first so the route guard can decide before
   *  the fetch resolves; clears it if the server rejects it. */
  async function restore() {
    const stored = localStorage.getItem(TOKEN_KEY) ?? RT?.token ?? null
    if (stored) {
      token.value = stored
      try {
        user.value = await authMe()
      } catch {
        // 401 already triggered `clearAndRedirect` via apiFetch's handler.
      }
    }
    ready.value = true
  }

  async function login(username: string, password: string): Promise<AuthUser> {
    const res = await authLogin(username, password)
    persist(res.token)
    user.value = res.user
    return res.user
  }

  async function logout() {
    await authLogout()
    clearAndRedirect()
  }

  async function changePassword(current: string, next: string) {
    await authChangePassword(current, next)
    try {
      user.value = await authMe()
    } catch {
      /* password changed; me refresh is best-effort */
    }
  }

  return { token, user, ready, restore, login, logout, changePassword, clearAndRedirect }
})
