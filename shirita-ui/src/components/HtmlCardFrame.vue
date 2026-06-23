<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'

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

// Unique per component instance, so a `message` event can be matched back to
// this card without relying on cross-frame window-identity (`event.source`),
// which is unset until the iframe document has actually loaded and is
// awkward to assert on in tests.
const token = Math.random().toString(36).slice(2)

// Real cards are authored assuming the host sizes the iframe to their
// content (the SillyTavern/JS-Slash-Runner convention — see
// examples/JS-Slash-Runner/src/iframe/adjust_iframe_height.js, which writes
// `frameElement.style.height` directly; that requires `allow-same-origin`,
// which this sandbox intentionally omits). `postMessage` works without it,
// so this injected script relays the same measurement across the boundary
// instead of writing the DOM directly.
function resizeScript(t: string): string {
  return `
<script>
(function () {
  function measure() {
    return Math.max(
      document.body.scrollHeight,
      document.body.offsetHeight,
      document.documentElement.scrollHeight,
      document.documentElement.offsetHeight,
    )
  }
  function report() {
    window.parent.postMessage({ source: 'shirita-html-card', token: '${t}', height: measure() }, '*')
  }
  if (document.body) {
    new ResizeObserver(report).observe(document.body)
  }
  report()
})()
<\/script>
`
}

// The srcdoc document has no access to the app's CSS (opaque origin), so without
// this it always falls back to the browser's white default when a card doesn't
// set its own background. Inject the app's theme colors as a base rule the
// card's own CSS (which comes later in source order) can still override.
const srcdoc = computed(() => {
  const bg = themeVar('--color-card', '#fff')
  const fg = themeVar('--color-ink', '#1b1b1b')
  return `<style>html,body{background:${bg};color:${fg};margin:0}</style>${props.html}${resizeScript(token)}`
})

const DEFAULT_HEIGHT = 640
const MIN_HEIGHT = 80
const MAX_HEIGHT = 4000
const height = ref(DEFAULT_HEIGHT)

function onMessage(e: MessageEvent) {
  const data = e.data as { source?: string; token?: string; height?: number } | null
  if (!data || data.source !== 'shirita-html-card' || data.token !== token || typeof data.height !== 'number') return
  height.value = Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, data.height))
}

onMounted(() => window.addEventListener('message', onMessage))
onUnmounted(() => window.removeEventListener('message', onMessage))
</script>

<template>
  <iframe
    class="html-card-frame"
    sandbox="allow-scripts"
    referrerpolicy="no-referrer"
    :srcdoc="srcdoc"
    :style="{ height: height + 'px' }"
  />
</template>

<style scoped>
.html-card-frame {
  display: block;
  width: 100%;
  border: 1px solid var(--color-line, #e5e5e5);
  border-radius: 10px;
  background: var(--color-card, #fff);
}
</style>
