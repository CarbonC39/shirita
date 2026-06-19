<script setup lang="ts">
// Renders a SillyTavern "HTML card" front-end (a full HTML/CSS/JS document
// embedded in a message) inside a sandboxed iframe. `sandbox="allow-scripts"`
// without `allow-same-origin` gives the document an opaque origin: its script
// can run, but it cannot reach the parent DOM, cookies, localStorage, or this
// app's auth state, and it cannot navigate the top-level page.
defineProps<{ html: string }>()
</script>

<template>
  <iframe
    class="html-card-frame"
    sandbox="allow-scripts"
    referrerpolicy="no-referrer"
    :srcdoc="html"
  />
</template>

<style scoped>
.html-card-frame {
  display: block;
  width: 100%;
  height: 640px;
  border: 1px solid var(--color-line, #e5e5e5);
  border-radius: 10px;
  background: #fff;
}
</style>
