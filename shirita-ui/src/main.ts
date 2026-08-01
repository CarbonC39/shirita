import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { router } from './router'
import { i18n } from './i18n'
import './styles.css'
import { bootCustomCss } from './composables/useCustomCss'
import { useAuthStore } from './stores/auth'
import { setAuthAccessor } from './api/client'

// Inject cached custom CSS before the app mounts to avoid a FOUC flash.
// useCustomCss() in App.vue reconciles with the server value on load.
bootCustomCss()

const pinia = createPinia()
const app = createApp(App)
app.use(pinia).use(router).use(i18n)

// Wire the auth store into the HTTP layer (live token source + 401 → clear &
// redirect), then restore any held session before mounting so the first
// navigation guard has a token to check.
const auth = useAuthStore(pinia)
setAuthAccessor(() => auth.token, () => auth.clearAndRedirect())
void auth.restore()

app.mount('#app')
