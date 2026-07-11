import { ref } from 'vue'

// A single transient toast, shared app-wide. Used for brief action feedback
// (e.g. "Copied") and surfaced errors (e.g. a failed upload) that would
// otherwise be swallowed. Not a queue: a newer toast replaces an older one.
type ToastKind = 'info' | 'error'
type Toast = { message: string; kind: ToastKind }

const toast = ref<Toast | null>(null)
let timer: ReturnType<typeof setTimeout> | undefined

export function useToast() {
  function show(message: string, kind: ToastKind = 'info') {
    toast.value = { message, kind }
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => { toast.value = null }, 2600)
  }
  function dismiss() {
    if (timer) clearTimeout(timer)
    toast.value = null
  }
  return { toast, show, dismiss }
}
