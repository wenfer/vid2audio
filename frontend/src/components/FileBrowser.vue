<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { api } from '../api'
import type { BrowserState, BrowserEntry } from '../types'
import { useToast } from '../composables/useToast'
import { formatBytes } from '../utils'

const props = defineProps<{
  initialPath?: string
  modelValue?: string
}>()

const emit = defineEmits<{
  'update:modelValue': [path: string]
}>()

const { show: showToast } = useToast()
const state = ref<BrowserState | null>(null)
const pathInput = ref('')
const selectedPath = ref(props.modelValue || '')

watch(() => props.modelValue, (v) => { if (v !== undefined) selectedPath.value = v })

onMounted(() => loadBrowser(props.initialPath || ''))

async function loadBrowser(path: string) {
  try {
    state.value = await api.getFiles(path || undefined)
    pathInput.value = state.value.path
    if (state.value.warning) showToast(state.value.warning, 'warning')
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  }
}

function handleClick(entry: BrowserEntry) {
  if (!entry.selectable) return
  selectedPath.value = entry.path
  emit('update:modelValue', entry.path)
}

function handleDblClick(entry: BrowserEntry) {
  if (!entry.selectable) return
  if (entry.type === 'directory') {
    loadBrowser(entry.path)
  } else {
    handleClick(entry)
  }
}

function goParent() {
  if (state.value?.parent) loadBrowser(state.value.parent)
}

defineExpose({ loadBrowser })
</script>

<template>
  <div class="file-browser">
    <div class="browser-toolbar">
      <button class="btn btn-ghost btn-sm" :disabled="!state?.parent" @click="goParent">⬅</button>
      <input
        v-model="pathInput"
        class="path-input"
        placeholder="输入路径..."
        @keydown.enter="loadBrowser(pathInput)"
      />
      <button class="btn btn-secondary btn-sm" @click="loadBrowser(pathInput)">打开</button>
      <button class="btn btn-ghost btn-sm" @click="loadBrowser(pathInput)">🔄</button>
    </div>
    <div class="browser-list">
      <div
        v-for="entry in state?.entries"
        :key="entry.path"
        class="browser-row"
        :class="{ active: selectedPath === entry.path, 'muted-row': !entry.selectable }"
        @click="handleClick(entry)"
        @dblclick="handleDblClick(entry)"
      >
        <div class="row-icon" :class="entry.type === 'directory' ? 'folder' : entry.is_video ? 'video' : 'other'">
          {{ entry.type === 'directory' ? '📁' : entry.is_video ? '🎬' : '📄' }}
        </div>
        <div class="row-info">
          <div class="row-name">{{ entry.name }}</div>
          <div class="row-meta">{{ entry.type === 'directory' ? '文件夹' : entry.is_video ? '视频' : (entry.reason || '文件') }}</div>
        </div>
        <div class="row-size">{{ entry.type === 'file' ? formatBytes(entry.size) : '' }}</div>
      </div>
      <div v-if="state && !state.entries.length" class="empty-state">
        <p>这个目录是空的</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.file-browser { display: flex; flex-direction: column; }
.browser-toolbar { display: flex; align-items: center; gap: 6px; margin-bottom: 12px; }
.path-input {
  flex: 1;
  height: 32px;
  font-size: 12px;
  font-family: 'SF Mono', 'Fira Code', monospace;
}
.browser-list {
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  max-height: 400px;
  overflow-y: auto;
}
.browser-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border);
  cursor: pointer;
  transition: background var(--transition);
  user-select: none;
}
.browser-row:last-child { border-bottom: none; }
.browser-row:hover { background: var(--bg-subtle); }
.browser-row.active { background: var(--accent-soft); }
.browser-row.muted-row { opacity: 0.5; cursor: default; }
.row-icon {
  width: 28px; height: 28px;
  display: flex; align-items: center; justify-content: center;
  border-radius: var(--radius-sm);
  font-size: 14px; flex-shrink: 0;
}
.row-icon.folder { background: var(--warning-soft); }
.row-icon.video { background: var(--accent-soft); }
.row-icon.other { background: var(--bg-subtle); }
.row-info { flex: 1; min-width: 0; }
.row-name { font-size: 13px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.row-meta { font-size: 11px; color: var(--text-muted); }
.row-size { font-size: 11px; color: var(--text-muted); flex-shrink: 0; }
.empty-state { text-align: center; color: var(--text-muted); padding: 32px; }
</style>
