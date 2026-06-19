<script setup lang="ts">
import AppShell from './components/AppShell.vue'
import { useTheme } from './composables/useTheme'
import { useCustomCss } from './composables/useCustomCss'

useTheme()
useCustomCss()
</script>

<template>
  <AppShell>
    <router-view v-slot="{ Component }">
      <transition name="page" mode="out-in">
        <component :is="Component" />
      </transition>
    </router-view>
  </AppShell>
</template>
