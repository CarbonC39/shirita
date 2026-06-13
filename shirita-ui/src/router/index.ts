import { createRouter, createWebHistory } from 'vue-router'
import HomeView from '../views/HomeView.vue'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'home', component: HomeView },
    { path: '/chat/:id', name: 'chat', component: () => import('../views/ChatView.vue') },
    { path: '/new', name: 'new', component: () => import('../views/NewChatView.vue') },
    { path: '/book', name: 'book', component: () => import('../views/BookView.vue') },
    { path: '/settings', name: 'settings', component: () => import('../views/SettingsView.vue') },
  ],
})
