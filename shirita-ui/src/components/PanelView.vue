<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import morphdom from 'morphdom'
import { sanitizePanelHtml, fenceCss } from '../utils/panel'

const props = defineProps<{
  html: string
  css: string
  values: Record<string, unknown>
}>()

const host = ref<HTMLDivElement | null>(null)
let shadow: ShadowRoot | null = null
let styleEl: HTMLStyleElement | null = null
let template: HTMLElement | null = null // parsed, sanitized, inline-fenced (no bindings applied)

// Parse the author HTML once per html change: sanitize, then fence inline styles.
function parseTemplate() {
  const root = document.createElement('div')
  root.setAttribute('data-panel-root', '')
  root.innerHTML = sanitizePanelHtml(props.html)
  root.querySelectorAll<HTMLElement>('[style]').forEach((el) => {
    el.setAttribute('style', fenceCss(el.getAttribute('style') || ''))
  })
  template = root
}

// Build a target tree from the template + current values, then morph the live tree.
function render() {
  if (!shadow || !template) return
  const target = template.cloneNode(true) as HTMLElement

  // {{var}} text interpolation (textContent assignment auto-escapes — no injection).
  const walker = document.createTreeWalker(target, NodeFilter.SHOW_TEXT)
  const texts: Text[] = []
  for (let n = walker.nextNode(); n; n = walker.nextNode()) texts.push(n as Text)
  for (const t of texts) {
    if (t.nodeValue && t.nodeValue.includes('{{')) {
      t.nodeValue = t.nodeValue.replace(/\{\{\s*(\w+)\s*\}\}/g, (_, k) => String(props.values[k] ?? ''))
    }
  }
  // data-bind → textContent; data-show → display.
  target.querySelectorAll<HTMLElement>('[data-bind]').forEach((el) => {
    el.textContent = String(props.values[el.getAttribute('data-bind')!] ?? '')
  })
  target.querySelectorAll<HTMLElement>('[data-show]').forEach((el) => {
    el.style.display = props.values[el.getAttribute('data-show')!] ? '' : 'none'
  })

  const live = shadow.querySelector('[data-panel-root]') as HTMLElement | null
  if (!live) {
    shadow.appendChild(target) // first paint
  } else {
    morphdom(live, target, {
      onBeforeElUpdated(fromEl, toEl) {
        // preserve user-toggled disclosure widgets the variables don't own
        if (fromEl instanceof HTMLDetailsElement && toEl instanceof HTMLDetailsElement) {
          toEl.open = fromEl.open
        }
        return true
      },
    })
  }
}

function applyCss() {
  if (styleEl) styleEl.textContent = fenceCss(props.css)
}

onMounted(() => {
  if (!host.value) return
  shadow = host.value.attachShadow({ mode: 'open' })
  styleEl = document.createElement('style')
  shadow.appendChild(styleEl)
  parseTemplate()
  applyCss()
  render()
})

watch(() => props.html, () => { parseTemplate(); render() })
watch(() => props.css, applyCss)
watch(() => props.values, render, { deep: true })
</script>

<template>
  <div ref="host" data-test="panel-host" style="position: relative; overflow: hidden; contain: content;" />
</template>
