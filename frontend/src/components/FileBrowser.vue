<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { api } from '../api'
import type { BrowserState, BrowserEntry } from '../types'
import { useToast } from '../composables/useToast'
import { usePrompt } from '../composables/usePrompt'
import { formatBytes } from '../utils'
import { confirmAction, isDesktop, pickDirectory, pickSavePath, revealInFileManager } from '../desktop'

const props = defineProps<{
  initialPath?: string
  modelValue?: string
}>()

const emit = defineEmits<{
  'update:modelValue': [path: string]
  'selection-change': [entry: BrowserEntry | null]
}>()

const { show: showToast } = useToast()
const { ask } = usePrompt()
const desktop = isDesktop()
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

/** 桌面版：用系统文件夹对话框跳转，比手输路径快得多。 */
async function browseTo() {
  const picked = await pickDirectory(state.value?.path)
  if (picked) await loadBrowser(picked)
}

function copySelection() {
  if (!selectedEntry.value) return
  clipboard.value = { path: selectedEntry.value.path }
  showToast('已复制，打开目标目录后点击粘贴', 'success')
}

async function moveSelection() {
  const entry = selectedEntry.value
  if (!entry || busy.value) return
  // 桌面版弹系统文件夹框；浏览器里只能手输——网页拿不到真实文件系统路径。
  const destination = desktop
    ? await pickDirectory(state.value?.path)
    : await ask({
        title: `移动“${entry.name}”`,
        label: '目标目录（绝对路径）',
        value: state.value?.path || '',
        confirmLabel: '移动',
      })
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
  // 用应用内输入框而不是 window.prompt：后者在 WebView2 里不弹窗、直接返回 null，
  // 桌面版的重命名会变成点了没反应。
  const next = await ask({
    title: '重命名',
    label: '新名称',
    value: entry.name,
    confirmLabel: '重命名',
  })
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
  if (!entry || busy.value) return
  const ok = await confirmAction(`确认删除“${entry.name}”吗？此操作不可恢复。`, {
    title: '删除',
    okLabel: '删除',
    danger: true,
  })
  if (!ok) return
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

/**
 * 「另存为」的默认路径：当前目录下的同名 zip。
 *
 * 分隔符从当前路径推断，不要写死 `/`——Windows 上默认路径会被原样交给系统对话框。
 */
function suggestedArchiveName(entry: BrowserEntry): string {
  const directory = state.value?.path || ''
  if (!directory) return `${entry.name}.zip`
  const separator = directory.includes('\\') ? '\\' : '/'
  const base = directory.endsWith(separator) ? directory : `${directory}${separator}`
  return `${base}${entry.name}.zip`
}

async function downloadSelection() {
  const entry = selectedEntry.value
  if (!entry || busy.value) return
  if (!desktop) {
    const link = document.createElement('a')
    link.href = api.archiveUrl(entry.path)
    link.download = `${entry.name}.zip`
    document.body.appendChild(link)
    link.click()
    link.remove()
    showToast('已开始打包下载', 'success')
    return
  }
  // 桌面版走「另存为」：WebView2 里 <a download> 最多把文件塞进浏览器下载目录，
  // 而用户要的是自己挑的 U 盘路径。选好路径后由后端直接写盘，不经过 WebView。
  const destination = await pickSavePath(suggestedArchiveName(entry), {
    name: 'ZIP 压缩包',
    extensions: ['zip'],
  })
  if (!destination) return
  busy.value = true
  showToast('正在打包…', 'info')
  try {
    const result = await api.archiveTo(entry.path, destination)
    showToast(`已保存，共 ${formatBytes(result.size)}`, 'success')
    await revealInFileManager(result.path)
  } catch (e: unknown) { showToast((e as Error).message, 'error') }
  finally { busy.value = false }
}

async function fatSortSelection() {
  const entry = selectedEntry.value
  if (!entry || entry.type !== 'directory' || busy.value) return
  const ok = await confirmAction(
    `确认对文件夹“${entry.name}”执行 FAT 排序？\n\n` +
    '将把其中的文件和子文件夹按文件名自然顺序（如 2 排在 10 之前）重新写入目录，' +
    '故事机 / 车机 / 老 U 盘播放器会按此顺序播放。\n' +
    '只调整存储顺序，不修改文件名和内容。',
    { title: 'FAT 排序', okLabel: '开始排序' }
  )
  if (!ok) return
  busy.value = true
  try {
    const result = await api.fatSort(entry.path)
    await loadBrowser(state.value?.path || '')
    showToast(
      result.recovered
        ? `已恢复上次中断的排序，共 ${result.count} 项`
        : `FAT 排序完成，共 ${result.count} 项`,
      'success'
    )
  } catch (e: unknown) { showToast((e as Error).message, 'error') }
  finally { busy.value = false }
}

defineExpose({ loadBrowser })
</script>

<template>
  <div class="file-browser">
    <div class="browser-toolbar">
      <button class="btn btn-ghost btn-sm" :disabled="!state?.parent" title="上一级" @click="goParent">⬅</button>
      <input
        v-model="pathInput"
        class="path-input"
        placeholder="输入路径..."
        @keydown.enter="loadBrowser(pathInput)"
      />
      <button class="btn btn-secondary btn-sm" @click="loadBrowser(pathInput)">打开</button>
      <button v-if="desktop" class="btn btn-ghost btn-sm" title="选择文件夹" @click="browseTo">📂</button>
      <button class="btn btn-ghost btn-sm" title="刷新" @click="loadBrowser(pathInput)">🔄</button>
    </div>
    <!-- 盘符 / 主目录快捷入口。Windows 上 C:\ 没有上一级，没有这排按钮就到不了
         U 盘所在的其他盘符。 -->
    <div v-if="state?.roots?.length" class="browser-roots">
      <button
        v-for="root in state.roots"
        :key="root.path"
        class="btn btn-ghost btn-sm root-chip"
        :title="root.path"
        @click="loadBrowser(root.path)"
      >{{ root.name }}</button>
    </div>
    <div class="browser-actions">
      <button class="btn btn-ghost btn-sm" :disabled="!selectedEntry" @click="copySelection">复制</button>
      <button class="btn btn-ghost btn-sm" :disabled="!selectedEntry || busy" @click="moveSelection">移动</button>
      <button class="btn btn-ghost btn-sm" :disabled="!clipboard || busy" @click="pasteSelection">粘贴</button>
      <button class="btn btn-ghost btn-sm" :disabled="!selectedEntry || busy" @click="renameSelection">重命名</button>
      <button class="btn btn-ghost btn-sm" :disabled="!selectedEntry || busy" @click="downloadSelection">打包下载</button>
      <button
        class="btn btn-ghost btn-sm"
        :disabled="!selectedEntry || selectedEntry.type !== 'directory' || busy"
        title="按自然顺序重排目录的物理存储顺序，修正故事机 / 车机的播放顺序"
        @click="fatSortSelection"
      >🔤 FAT 排序</button>
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
.browser-roots { display: flex; flex-wrap: wrap; gap: 4px; margin-bottom: 10px; }
.root-chip { font-family: 'SF Mono', 'Fira Code', monospace; }
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
