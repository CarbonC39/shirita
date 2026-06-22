<script setup lang="ts">
import { computed } from 'vue'

// Renders a SillyTavern "HTML card" front-end (a full HTML/CSS/JS document
// embedded in a message) inside a sandboxed iframe. `sandbox="allow-scripts"`
// without `allow-same-origin` gives the document an opaque origin: its script
// can run, but it cannot reach the parent DOM, cookies, localStorage, or this
// app's auth state, and it cannot navigate the top-level page.
const props = defineProps<{ html: string }>()

function themeVar(name: string, fallback: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return v || fallback
}

// The srcdoc document has no access to the app's CSS (opaque origin), so without
// this it always falls back to the browser's white default when a card doesn't
// set its own background. Inject the app's theme colors as a base rule the
// card's own CSS (which comes later in source order) can still override.
const srcdoc = computed(() => {
  const bg = themeVar('--color-card', '#fff')
  const fg = themeVar('--color-ink', '#1b1b1b')
  return `<style>html,body{background:${bg};color:${fg};margin:0}</style>${props.html}`
})
</script>

<template>
  <iframe
    class="html-card-frame"
    sandbox="allow-scripts"
    referrerpolicy="no-referrer"
    :srcdoc="srcdoc"
  />
</template>

<style scoped>
.html-card-frame {
  display: block;
  width: 100%;
  height: 640px;
  border: 1px solid var(--color-line, #e5e5e5);
  border-radius: 10px;
  background: var(--color-card, #fff);
}
</style>
