import { ref } from 'vue'

export interface Toast {
  id: number
  message: string
  type: 'info' | 'success' | 'warning' | 'error'
  visible: boolean
}

const toasts = ref<Toast[]>([])
let nextId = 0

export function useToast() {
  function show(message: string, type: Toast['type'] = 'info') {
    const id = nextId++
    const toast: Toast = { id, message, type, visible: false }
    toasts.value.push(toast)
    requestAnimationFrame(() => {
      const t = toasts.value.find((t) => t.id === id)
      if (t) t.visible = true
    })
    setTimeout(() => remove(id), 3000)
  }

  function remove(id: number) {
    const t = toasts.value.find((t) => t.id === id)
    if (t) t.visible = false
    setTimeout(() => {
      toasts.value = toasts.value.filter((t) => t.id !== id)
    }, 250)
  }

  return { toasts, show, remove }
}
