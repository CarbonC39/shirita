<script setup lang="ts">
import AppShell from './components/AppShell.vue'
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
