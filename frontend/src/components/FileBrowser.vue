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
  'selection-change': [entry: BrowserEntry | null]
}>()

const { show: showToast } = useToast()
const state = ref<BrowserState | null>(null)
const pathInput = ref('')
const selectedPath = ref(props.modelValue || '')
const selectedEntry = ref<BrowserEntry | null>(null)
const clipboard = ref<{ path: string } | null>(null)
const busy = ref(false)

watch(() => props.modelValue, (v) => { if (v !== undefined) selectedPath.value = v })

onMounted(() => loadBrowser(props.initialPath || ''))

async function loadBrowser(path: string) {
  try {
    state.value = await api.getFiles(path || undefined)
    pathInput.value = state.value.path
    selectedEntry.value = null
    emit('selection-change', null)
    if (state.value.warning) showToast(state.value.warning, 'warning')
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  }
}

function handleClick(entry: BrowserEntry) {
  selectedEntry.value = entry
  emit('selection-change', entry)
  if (entry.selectable) {
    selectedPath.value = entry.path
    emit('update:modelValue', entry.path)
  }
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

function copySelection() {
  if (!selectedEntry.value) return
  clipboard.value = { path: selectedEntry.value.path }
  showToast('已复制，打开目标目录后点击粘贴', 'success')
}

async function moveSelection() {
  const entry = selectedEntry.value
  if (!entry || busy.value) return
  const destination = window.prompt('请输入目标目录', state.value?.path || '')
  if (!destination || destination === state.value?.path) return
  busy.value = true
  try {
    await api.moveFiles([entry.path], destination.trim())
    await loadBrowser(state.value?.path || '')
    showToast('移动成功', 'success')
  } catch (e: unknown) { showToast((e as Error).message, 'error') }
  finally { busy.value = false }
}

async function pasteSelection() {
  if (!clipboard.value || !state.value || busy.value) return
  busy.value = true
  try {
    const { path } = clipboard.value
    await api.copyFiles([path], state.value.path)
    clipboard.value = null
    await loadBrowser(state.value.path)
    showToast('操作完成', 'success')
  } catch (e: unknown) { showToast((e as Error).message, 'error') }
  finally { busy.value = false }
}

async function renameSelection() {
  const entry = selectedEntry.value
  if (!entry || busy.value) return
  const next = window.prompt('请输入新名称', entry.name)
  if (!next || next === entry.name) return
  busy.value = true
  try {
    await api.renameFile(entry.path, next.trim())
    await loadBrowser(state.value?.path || '')
    selectedEntry.value = null
    emit('selection-change', null)
    showToast('重命名成功', 'success')
  } catch (e: unknown) { showToast((e as Error).message, 'error') }
  finally { busy.value = false }
}

async function deleteSelection() {
  const entry = selectedEntry.value
  if (!entry || busy.value || !window.confirm(`确认删除“${entry.name}”吗？此操作不可恢复。`)) return
  busy.value = true
  try {
    await api.deleteFiles([entry.path])
    await loadBrowser(state.value?.path || '')
    selectedEntry.value = null
    emit('selection-change', null)
    showToast('删除成功', 'success')
  } catch (e: unknown) { showToast((e as Error).message, 'error') }
  finally { busy.value = false }
}

function downloadSelection() {
  const entry = selectedEntry.value
  if (!entry || busy.value) return
  const link = document.createElement('a')
  link.href = api.archiveUrl(entry.path)
  link.download = `${entry.name}.zip`
  document.body.appendChild(link)
  link.click()
  link.remove()
  showToast('已开始打包下载', 'success')
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
    <div class="browser-actions">
      <button class="btn btn-ghost btn-sm" :disabled="!selectedEntry" @click="copySelection">复制</button>
      <button class="btn btn-ghost btn-sm" :disabled="!selectedEntry || busy" @click="moveSelection">移动</button>
      <button class="btn btn-ghost btn-sm" :disabled="!clipboard || busy" @click="pasteSelection">粘贴</button>
      <button class="btn btn-ghost btn-sm" :disabled="!selectedEntry || busy" @click="renameSelection">重命名</button>
      <button class="btn btn-ghost btn-sm" :disabled="!selectedEntry || busy" @click="downloadSelection">打包下载</button>
      <button class="btn btn-ghost btn-sm danger-action" :disabled="!selectedEntry || busy" @click="deleteSelection">删除</button>
    </div>
    <div class="browser-list">
      <div
        v-for="entry in state?.entries"
        :key="entry.path"
        class="browser-row"
        :class="{ active: selectedEntry?.path === entry.path }"
        @click="handleClick(entry)"
        @dblclick="handleDblClick(entry)"
      >
        <div class="row-icon" :class="entry.type === 'directory' ? 'folder' : entry.is_video ? 'video' : 'other'">
          {{ entry.type === 'directory' ? '📁' : entry.is_video ? '🎬' : '📄' }}
        </div>
        <div class="row-info">
          <div class="row-name">{{ entry.name }}</div>
          <div class="row-meta">{{ entry.type === 'directory' ? '文件夹' : entry.is_video ? '视频' : '文件' }}</div>
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
.browser-actions {
  display: flex; align-items: center; flex-wrap: wrap; gap: 6px;
  margin-bottom: 10px; padding-bottom: 10px; border-bottom: 1px solid var(--border);
}
.danger-action:not(:disabled) { color: var(--danger); }
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
