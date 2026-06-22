<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import morphdom from 'morphdom'
import { sanitizePanelHtml, fenceCss } from '../utils/panel'
import type { PanelAction } from '../api/types'

const props = defineProps<{
  html: string
  css: string
  values: Record<string, unknown>
}>()
const emit = defineEmits<{ action: [PanelAction] }>()

function interpolate(text: string): string {
  return text.replace(/\{\{\s*(\w+)\s*\}\}/g, (_, k) => String(props.values[k] ?? ''))
}

function onClick(e: Event) {
  const target = e.target as HTMLElement | null
  const el = target?.closest?.('[data-diff-key],[data-insert],[data-send]') as HTMLElement | null
  if (!el) return
  if (el.hasAttribute('data-diff-key')) {
    emit('action', {
      kind: 'diff',
      key: el.getAttribute('data-diff-key') || '',
      op: el.getAttribute('data-diff-op') || 'set',
      value: el.getAttribute('data-diff-value'),
    })
  } else if (el.hasAttribute('data-insert')) {
    emit('action', { kind: 'insert', text: interpolate(el.getAttribute('data-insert') || '') })
  } else if (el.hasAttribute('data-send')) {
    emit('action', { kind: 'send', text: interpolate(el.getAttribute('data-send') || '') })
  }
}

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

// CSS custom properties inherit through the shadow boundary, so the theme
// vars are visible here. Set them as the host's default background/color
// first so a panel that doesn't define its own background blends with the
// app theme instead of defaulting to white; pack CSS (appended after, same
// specificity) still wins if it sets its own.
const BASE_CSS = ':host{background:var(--color-card,#fff);color:var(--color-ink,#1b1b1b)}'

function applyCss() {
  if (styleEl) styleEl.textContent = BASE_CSS + fenceCss(props.css)
}

onMounted(() => {
  if (!host.value) return
  shadow = host.value.attachShadow({ mode: 'open' })
  styleEl = document.createElement('style')
  shadow.appendChild(styleEl)
  shadow.addEventListener('click', onClick)
  parseTemplate()
  applyCss()
  render()
})

onUnmounted(() => { shadow?.removeEventListener('click', onClick) })

watch(() => props.html, () => { parseTemplate(); render() })
watch(() => props.css, applyCss)
watch(() => props.values, render, { deep: true })
</script>

<template>
  <div ref="host" data-test="panel-host" style="position: relative; overflow: hidden; contain: content;" />
</template>
