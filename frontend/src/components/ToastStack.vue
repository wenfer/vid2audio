<script setup lang="ts">
import { useToast } from '../composables/useToast'

const { toasts } = useToast()
</script>

<template>
  <div class="toast-stack">
    <div
      v-for="t in toasts"
      :key="t.id"
      class="toast"
      :class="[t.type, { show: t.visible }]"
    >
      {{ t.message }}
    </div>
  </div>
</template>

<style scoped>
.toast-stack {
  position: fixed;
  top: 72px;
  right: 20px;
  z-index: 100;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.toast {
  min-width: 280px;
  max-width: 400px;
  padding: 12px 16px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border);
  background: var(--bg-elevated);
  color: var(--text-primary);
  font-size: 13px;
  box-shadow: var(--shadow-lg);
  transform: translateX(20px);
  opacity: 0;
  transition: all 0.25s ease;
}
.toast.show { transform: translateX(0); opacity: 1; }
.toast.success { border-left: 3px solid var(--success); background: var(--success-soft); }
.toast.warning { border-left: 3px solid var(--warning); background: var(--warning-soft); }
.toast.error { border-left: 3px solid var(--danger); background: var(--danger-soft); }
</style>
