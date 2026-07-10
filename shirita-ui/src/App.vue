<script setup lang="ts">
import AppShell from './components/AppShell.vue'
import { useTheme } from './composables/useTheme'
import { useCustomCss } from './composables/useCustomCss'

useTheme()
useCustomCss()
</script>

<template>
  <AppShell>
    <router-view v-slot="{ Component, route }">
      <transition name="page" mode="out-in">
        <!-- Key on the path so navigating between two chats (e.g. after a Fork
             branches into a new session) remounts the view. Without this,
             vue-router reuses the same ChatView instance and its onMounted-
             captured sessionId keeps targeting the original conversation. -->
        <component :is="Component" :key="route.path" />
      </transition>
    </router-view>
  </AppShell>
</template>
