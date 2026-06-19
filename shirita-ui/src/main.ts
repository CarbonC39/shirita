import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { router } from './router'
import { i18n } from './i18n'
import './styles.css'
import { bootCustomCss } from './composables/useCustomCss'

// Inject cached custom CSS before the app mounts to avoid a FOUC flash.
// useCustomCss() in App.vue reconciles with the server value on load.
bootCustomCss()
createApp(App).use(createPinia()).use(router).use(i18n).mount('#app')
