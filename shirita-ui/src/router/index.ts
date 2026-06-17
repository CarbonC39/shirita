import { createRouter, createWebHistory } from 'vue-router'
import HomeView from '../views/HomeView.vue'
import ChatView from '../views/ChatView.vue'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'home', component: HomeView },
    { path: '/chat/:id', name: 'chat', component: ChatView },
    // Crumb labels are i18n keys, resolved with $t in AppShell.
    { path: '/new', name: 'new', component: () => import('../views/NewChatView.vue'), meta: { crumbs: [{ label: 'chat.title', to: '/' }, { label: 'shell.new' }] } },
    { path: '/new/prompt', name: 'newPrompt', component: () => import('../views/NewChatPromptView.vue'), meta: { crumbs: [{ label: 'chat.title', to: '/' }, { label: 'shell.new', to: '/new' }, { label: 'prompt.crumb' }] } },
    { path: '/book', name: 'book', component: () => import('../views/BookView.vue') },
    { path: '/settings', name: 'settings', component: () => import('../views/SettingsView.vue') },
  ],
})
