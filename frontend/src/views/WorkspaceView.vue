<script setup lang="ts">
import { ref } from 'vue'
import { api } from '../api'
import { useSettings } from '../composables/useSettings'
import { useToast } from '../composables/useToast'
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

function createTask(collection: Collection) {
  taskCollection.value = collection
  showAnalysis.value = false
  showTaskWizard.value = true
}

async function scanSelected() {
  if (!selectedPath.value) { showToast('请先选择文件夹或视频', 'warning'); return }
  try {
    const result = await api.startScan([selectedPath.value])
    if (result.collections.length) {
      analysisResults.value = result.collections
      showAnalysis.value = true
      showToast(`分析完成: ${result.files_found} 个视频`, 'success')
    } else {
      showToast('没有找到符合过滤条件的视频文件', 'warning')
    }
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
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
        <button class="btn btn-secondary" @click="scanSelected">🔍 分析选中项</button>
      </div>
    </section>

    <section class="card browser-help">
      <h2>文件管理</h2>
      <p>选择文件或文件夹后，可复制、剪切、粘贴、移动、重命名和删除。分析功能仅用于识别视频并创建提取任务。</p>
    </section>
  </div>
  <Teleport to="body">
    <div v-if="showAnalysis" class="modal-overlay" @click.self="showAnalysis = false">
      <div class="analysis-dialog">
        <div class="modal-header"><div><h2>分析结果</h2><p class="text-muted text-sm">共发现 {{ analysisResults.reduce((n, c) => n + c.episode_count, 0) }} 个视频</p></div><button class="btn-close" @click="showAnalysis = false">✕</button></div>
        <div class="analysis-list">
          <article v-for="c in analysisResults" :key="c.id" class="analysis-item">
            <div class="analysis-info"><strong>{{ c.name }}</strong><span>{{ c.source_path }}</span><span>{{ c.episode_count }} 个视频</span></div>
            <button class="btn btn-primary btn-sm" @click="createTask(c)">🎵 创建任务</button>
          </article>
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
.workspace-layout { display: grid; grid-template-columns: minmax(0, 1fr) 300px; gap: 20px; align-items: start; }
.browser-card { position: sticky; top: 84px; }
.browser-help { padding: 20px; color: var(--text-secondary); }
.browser-help h2 { font-size: 15px; margin-bottom: 8px; color: var(--text-primary); }
.browser-help p { font-size: 13px; line-height: 1.7; }
.modal-overlay { position: fixed; inset: 0; z-index: 90; display: grid; place-items: center; background: rgba(15,23,42,.4); backdrop-filter: blur(4px); }
.analysis-dialog { width: min(680px, calc(100vw - 32px)); max-height: 80vh; overflow: auto; background: var(--bg-elevated); border-radius: var(--radius-xl); box-shadow: var(--shadow-xl); }
.modal-header { display:flex; justify-content:space-between; align-items:center; padding:20px 24px; border-bottom:1px solid var(--border); }
.modal-header h2 { font-size:16px; }
.btn-close { border:0; background:transparent; color:var(--text-muted); font-size:16px; cursor:pointer; }
.analysis-list { padding: 16px 24px 24px; display:flex; flex-direction:column; gap:10px; }
.analysis-item { display:flex; align-items:center; justify-content:space-between; gap:16px; padding:14px; border:1px solid var(--border); border-radius:var(--radius-md); }
.analysis-info { min-width:0; display:flex; flex-direction:column; gap:3px; }
.analysis-info strong { font-size:14px; }
.analysis-info span { font-size:12px; color:var(--text-muted); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.empty-state { text-align: center; color: var(--text-muted); padding: 32px; }

@media (max-width: 1024px) { .workspace-layout { grid-template-columns: 1fr; } .browser-card { position: static; } }
</style>
