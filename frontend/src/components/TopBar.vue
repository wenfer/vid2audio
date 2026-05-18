<script setup lang="ts">
import { ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import WizardModal from './WizardModal.vue'

const router = useRouter()
const route = useRoute()
const showWizard = ref(false)

const navItems = [
  { path: '/workspace', label: '文件管理', icon: '📁' },
  { path: '/tasks', label: '任务管理', icon: '📋' },
  { path: '/settings', label: '系统配置', icon: '⚙️' },
]
</script>

<template>
  <header class="topbar">
    <div class="topbar-left">
      <div class="brand">
        <span class="brand-icon">🎵</span>
        <span>Vid2Audio</span>
      </div>
    </div>
    <nav class="topbar-nav">
      <button
        v-for="item in navItems"
        :key="item.path"
        class="nav-tab"
        :class="{ active: route.path === item.path }"
        @click="router.push(item.path)"
      >
        <span class="nav-icon">{{ item.icon }}</span>
        <span class="nav-label">{{ item.label }}</span>
      </button>
    </nav>
    <div class="topbar-right"></div>
  </header>

  <!-- Floating Action Button -->
  <button class="fab" @click="showWizard = true" title="快速创建任务">
    <span class="fab-icon">＋</span>
    <span class="fab-label">创建任务</span>
  </button>

  <WizardModal v-model:visible="showWizard" />
</template>

<style scoped>
.topbar {
  position: sticky;
  top: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  gap: 16px;
  height: 60px;
  padding: 0 24px;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  box-shadow: var(--shadow-sm);
}
.topbar-left { flex-shrink: 0; }
.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 18px;
  font-weight: 700;
}
.brand-icon { font-size: 22px; }
.topbar-nav {
  display: flex;
  gap: 4px;
  margin-left: 32px;
}
.nav-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 36px;
  padding: 0 14px;
  border: none;
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--text-secondary);
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--transition);
}
.nav-tab:hover { background: var(--bg-subtle); color: var(--text-primary); }
.nav-tab.active { background: var(--accent-soft); color: var(--accent-text); }
.nav-icon { font-size: 15px; }
.topbar-right { margin-left: auto; }

/* Floating Action Button */
.fab {
  position: fixed;
  left: 24px;
  bottom: 50%;
  transform: translateY(50%);
  z-index: 60;
  display: flex;
  align-items: center;
  gap: 8px;
  height: 48px;
  padding: 0 20px;
  border: none;
  border-radius: 999px;
  background: var(--accent);
  color: var(--text-inverse);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  box-shadow: 0 6px 20px rgba(99, 102, 241, 0.4), var(--shadow-lg);
  transition: all 0.2s ease;
}
.fab:hover {
  background: var(--accent-hover);
  transform: translateY(50%) translateX(0) scale(1.03);
  box-shadow: 0 8px 28px rgba(99, 102, 241, 0.5), var(--shadow-xl);
}
.fab:active {
  transform: translateY(50%);
}
.fab-icon {
  font-size: 20px;
  line-height: 1;
}
.fab-label {
  white-space: nowrap;
}

@media (max-width: 768px) {
  .topbar { padding: 0 12px; }
  .topbar-nav { margin-left: 12px; }
  .nav-label { display: none; }
  .fab {
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%) translateY(0);
  }
  .fab:hover {
    transform: translateX(-50%) translateY(-2px);
  }
}
</style>
