<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { api } from '../api'
import { useSettings } from '../composables/useSettings'
import { useToast } from '../composables/useToast'
import { formatTimestamp, statusBadgeClass, statusLabel } from '../utils'
import { confirmAction } from '../desktop'
import FileBrowser from '../components/FileBrowser.vue'
import WizardModal from '../components/WizardModal.vue'
import type { Collection } from '../types'

const { settings } = useSettings()
const { show: showToast } = useToast()

const selectedPath = ref('')
const analysisResults = ref<Collection[]>([])
const taskCollection = ref<Collection | null>(null)
const showAnalysis = ref(false)
const showTaskWizard = ref(false)
const history = ref<Collection[]>([])
const historyLoading = ref(false)
const scanning = ref(false)

onMounted(loadHistory)

async function loadHistory() {
  historyLoading.value = true
  try {
    history.value = await api.getCollections()
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  } finally {
    historyLoading.value = false
  }
}

function createTask(collection: Collection) {
  taskCollection.value = collection
  showAnalysis.value = false
  showTaskWizard.value = true
}

// 历史里的合集只有概要，没有 video_files/audio_tracks，创建任务前先取完整数据。
async function createTaskFromHistory(collection: Collection) {
  try {
    taskCollection.value = await api.getCollection(collection.id)
    showTaskWizard.value = true
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  }
}

async function removeFromHistory(collection: Collection) {
  const ok = await confirmAction(
    `确认从分析历史中删除「${collection.name}」？\n\n只删除分析记录，不会删除任何视频或音频文件。`,
    { title: '删除分析记录', okLabel: '删除', danger: true }
  )
  if (!ok) return
  try {
    await api.deleteCollection(collection.id)
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
    return
  }
  await loadHistory()
  showToast('已从分析历史中删除', 'success')
}

async function rescan(collection: Collection) {
  try {
    const result = await api.rescanCollection(collection.id)
    analysisResults.value = result.collections
    showAnalysis.value = true
    await loadHistory()
    showToast(`重新分析完成: ${result.files_found} 个视频`, 'success')
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  }
}

async function scanSelected() {
  if (!selectedPath.value) { showToast('请先选择文件夹或视频', 'warning'); return }
  scanning.value = true
  try {
    const result = await api.startScan([selectedPath.value])
    if (result.collections.length) {
      analysisResults.value = result.collections
      showAnalysis.value = true
      await loadHistory()
      showToast(`分析完成: ${result.files_found} 个视频`, 'success')
    } else {
      showToast('没有找到符合过滤条件的视频文件', 'warning')
    }
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  } finally {
    scanning.value = false
  }
}
</script>

<template>
  <div class="workspace-layout">
    <!-- Left: File Browser -->
    <section class="card browser-card">
      <div class="card-header">
        <h2>📁 文件浏览器</h2>
      </div>
      <FileBrowser v-model="selectedPath" :initial-path="settings?.scan_directories[0] || ''" />
      <div class="card-actions">
        <button class="btn btn-secondary" :disabled="scanning" @click="scanSelected">
          {{ scanning ? '分析中…' : '🔍 分析选中项' }}
        </button>
      </div>
    </section>

    <div class="side-column">
      <section class="card browser-help">
        <h2>文件管理</h2>
        <p>选择文件或文件夹后，可复制、剪切、粘贴、移动、重命名和删除。分析功能仅用于识别视频并创建提取任务。</p>
        <p>选中文件夹后可执行「FAT 排序」：按文件名自然顺序重写目录，修正故事机 / 车机按存储顺序播放导致的乱序。</p>
      </section>

      <section class="card history-card">
        <div class="card-header">
          <h2>🕘 分析历史</h2>
          <button class="btn btn-ghost btn-sm" :disabled="historyLoading" @click="loadHistory">刷新</button>
        </div>
        <div class="history-list">
          <article v-for="c in history" :key="c.id" class="history-item">
            <div class="history-info">
              <strong :title="c.name">{{ c.name }}</strong>
              <span :title="c.source_path">{{ c.source_path }}</span>
              <span class="history-meta">
                {{ c.episode_count }} 个视频
                · <span class="badge" :class="statusBadgeClass(c.status)">{{ statusLabel(c.status) }}</span>
                · {{ formatTimestamp(c.updated_at || c.created_at) }}
              </span>
            </div>
            <div class="history-actions">
              <button class="btn btn-primary btn-sm" @click="createTaskFromHistory(c)">🎵 创建任务</button>
              <button class="btn btn-ghost btn-sm" title="重新扫描源目录" @click="rescan(c)">↻</button>
              <button class="btn btn-ghost btn-sm danger-text" title="删除这条分析记录" @click="removeFromHistory(c)">✕</button>
            </div>
          </article>
          <div v-if="!history.length && !historyLoading" class="empty-state">
            <p>暂无分析历史</p>
            <p class="text-sm">分析结果会自动保存在这里，弹窗关掉也能找回。</p>
          </div>
        </div>
      </section>
    </div>
  </div>
  <Teleport to="body">
    <!-- 不响应遮罩点击：误触关掉后用户以为结果丢了，只能通过按钮关闭。 -->
    <div v-if="showAnalysis" class="modal-overlay">
      <div class="analysis-dialog">
        <div class="modal-header">
          <div>
            <h2>分析结果</h2>
            <p class="text-muted text-sm">共发现 {{ analysisResults.reduce((n, c) => n + c.episode_count, 0) }} 个视频 · 已保存到分析历史</p>
          </div>
          <button class="btn-close" title="关闭" @click="showAnalysis = false">✕</button>
        </div>
        <div class="analysis-list">
          <article v-for="c in analysisResults" :key="c.id" class="analysis-item">
            <div class="analysis-info"><strong>{{ c.name }}</strong><span>{{ c.source_path }}</span><span>{{ c.episode_count }} 个视频</span></div>
            <button class="btn btn-primary btn-sm" @click="createTask(c)">🎵 创建任务</button>
          </article>
        </div>
        <div class="analysis-footer">
          <span class="text-muted text-sm">关闭后可在右侧「分析历史」里重新找到这些结果。</span>
          <button class="btn btn-secondary btn-sm" @click="showAnalysis = false">关闭</button>
        </div>
      </div>
    </div>
  </Teleport>
  <WizardModal
    v-model:visible="showTaskWizard"
    :initial-collection="taskCollection"
    :initial-path="taskCollection?.source_path"
  />
</template>

<style scoped>
.workspace-layout { display: grid; grid-template-columns: minmax(0, 1fr) 320px; gap: 20px; align-items: start; }
.browser-card { position: sticky; top: 84px; }
.side-column { display: flex; flex-direction: column; gap: 20px; }
.browser-help { padding: 20px; color: var(--text-secondary); }
.browser-help h2 { font-size: 15px; margin-bottom: 8px; color: var(--text-primary); }
.browser-help p { font-size: 13px; line-height: 1.7; }
.history-card { display: flex; flex-direction: column; }
.history-list { display: flex; flex-direction: column; gap: 8px; padding: 12px 16px 16px; max-height: 420px; overflow-y: auto; }
.history-item { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--radius-md); }
.history-item:hover { border-color: var(--border-strong); }
.history-info { min-width: 0; display: flex; flex-direction: column; gap: 3px; }
.history-info strong { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.history-info span { font-size: 11px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.history-meta { display: flex; align-items: center; gap: 4px; }
.history-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
.danger-text { color: var(--danger-text); }
.modal-overlay { position: fixed; inset: 0; z-index: 90; display: grid; place-items: center; background: rgba(15,23,42,.4); backdrop-filter: blur(4px); }
.analysis-dialog { width: min(680px, calc(100vw - 32px)); max-height: 80vh; display: flex; flex-direction: column; background: var(--bg-elevated); border-radius: var(--radius-xl); box-shadow: var(--shadow-xl); overflow: hidden; }
.modal-header { display:flex; justify-content:space-between; align-items:center; gap:16px; padding:20px 24px; border-bottom:1px solid var(--border); }
.modal-header h2 { font-size:16px; }
.btn-close { border:0; background:transparent; color:var(--text-muted); font-size:16px; cursor:pointer; }
.analysis-list { flex:1; overflow-y:auto; padding: 16px 24px; display:flex; flex-direction:column; gap:10px; }
.analysis-footer { display:flex; align-items:center; justify-content:space-between; gap:16px; padding:14px 24px; border-top:1px solid var(--border); }
.analysis-item { display:flex; align-items:center; justify-content:space-between; gap:16px; padding:14px; border:1px solid var(--border); border-radius:var(--radius-md); }
.analysis-info { min-width:0; display:flex; flex-direction:column; gap:3px; }
.analysis-info strong { font-size:14px; }
.analysis-info span { font-size:12px; color:var(--text-muted); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.empty-state { text-align: center; color: var(--text-muted); padding: 24px 8px; }

@media (max-width: 1024px) { .workspace-layout { grid-template-columns: 1fr; } .browser-card { position: static; } }
</style>
