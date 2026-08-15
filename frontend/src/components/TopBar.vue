<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import WizardModal from './WizardModal.vue'
import { useSystemStatus } from '../composables/useSystemStatus'

const router = useRouter()
const route = useRoute()
const showWizard = ref(false)
const { status, load } = useSystemStatus()
onMounted(load)

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
        <span v-if="status" class="brand-version" :title="`ffmpeg ${status.ffmpeg_available ? '可用' : '不可用'} · ffprobe ${status.ffprobe_available ? '可用' : '不可用'}`">v{{ status.version }}</span>
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
    <div class="topbar-right">
      <button class="btn btn-primary create-task-btn" @click="showWizard = true" title="快速创建任务">
        <span>＋</span>
        <span>创建任务</span>
      </button>
    </div>
  </header>

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
.brand-version {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary);
  margin-left: 2px;
  user-select: none;
}
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
.create-task-btn { gap: 4px; }

@media (max-width: 768px) {
  .topbar { padding: 0 12px; }
  .topbar-nav { margin-left: 12px; }
  .nav-label { display: none; }
  .create-task-btn span:last-child { display: none; }
}
</style>
