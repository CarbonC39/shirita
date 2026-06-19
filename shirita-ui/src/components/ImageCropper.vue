<script setup lang="ts">
import { ref, onMounted } from 'vue'

const props = defineProps<{ file: File }>()
const emit = defineEmits<{ cropped: [Blob]; cancel: [] }>()

const SIZE = 512
const canvas = ref<HTMLCanvasElement | null>(null)
const img = new Image()
let scale = 1, minScale = 1, offsetX = 0, offsetY = 0
let dragging = false, lastX = 0, lastY = 0

function draw() {
  const c = canvas.value; if (!c) return
  const ctx = c.getContext('2d')!; ctx.clearRect(0, 0, SIZE, SIZE)
  ctx.drawImage(img, offsetX, offsetY, img.width * scale, img.height * scale)
}
function clamp() {
  const w = img.width * scale, h = img.height * scale
  offsetX = Math.min(0, Math.max(SIZE - w, offsetX))
  offsetY = Math.min(0, Math.max(SIZE - h, offsetY))
}
onMounted(() => {
  const url = URL.createObjectURL(props.file)
  img.onload = () => {
    minScale = Math.max(SIZE / img.width, SIZE / img.height)
    scale = minScale
    offsetX = (SIZE - img.width * scale) / 2
    offsetY = (SIZE - img.height * scale) / 2
    draw(); URL.revokeObjectURL(url)
  }
  img.src = url
})
function onWheel(e: WheelEvent) {
  e.preventDefault()
  scale = Math.max(minScale, scale * (e.deltaY < 0 ? 1.05 : 0.95))
  clamp(); draw()
}
function onDown(e: PointerEvent) { dragging = true; lastX = e.clientX; lastY = e.clientY }
function onMove(e: PointerEvent) {
  if (!dragging) return
  offsetX += e.clientX - lastX; offsetY += e.clientY - lastY
  lastX = e.clientX; lastY = e.clientY; clamp(); draw()
}
function onUp() { dragging = false }
function confirmCrop() {
  canvas.value!.toBlob((b) => { if (b) emit('cropped', b) }, 'image/png')
}
</script>

<template>
  <div class="flex flex-col items-center gap-3">
    <canvas
      ref="canvas" :width="512" :height="512"
      class="w-[256px] h-[256px] rounded-full border border-line touch-none cursor-grab"
      @wheel="onWheel" @pointerdown="onDown" @pointermove="onMove" @pointerup="onUp" @pointerleave="onUp"
    />
    <div class="flex gap-2">
      <button class="btn btn-ghost" @click="emit('cancel')">{{ $t('common.cancel') }}</button>
      <button class="btn btn-primary" data-test="cropper-confirm" @click="confirmCrop">{{ $t('common.save') }}</button>
    </div>
  </div>
</template>
