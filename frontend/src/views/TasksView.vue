<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { api } from '../api'
import { useToast } from '../composables/useToast'
import { statusBadgeClass, statusLabel } from '../utils'
import { confirmAction } from '../desktop'
import type { ExtractJob, ExtractJobDetail, ExtractJobItem } from '../types'

const { show: showToast } = useToast()
const jobs = ref<ExtractJob[]>([])
const selectedJob = ref<ExtractJobDetail | null>(null)
const logItem = ref<ExtractJobItem | null>(null)
const showModal = ref(false)
const busyId = ref('')
let pollTimer: ReturnType<typeof setInterval> | null = null

const RUNNING = ['queued', 'processing']
const RESUMABLE = ['paused', 'failed', 'cancelled']

onMounted(async () => {
  await loadJobs()
  startListPolling()
})
onUnmounted(stopPolling)

function isRunning(status: string) {
  return RUNNING.includes(status)
}
function canResume(status: string) {
  return RESUMABLE.includes(status)
}

async function loadJobs() {
  jobs.value = await api.getJobs()
}

async function selectJob(id: string) {
  selectedJob.value = await api.getJob(id)
  showModal.value = true
  startPolling(id)
}

function closeModal() {
  showModal.value = false
  selectedJob.value = null
  startListPolling()
}

/// 列表页也要自动刷新，否则暂停/继续后的状态要手动刷新才看得到。
function startListPolling() {
  stopPolling()
  pollTimer = setInterval(async () => {
    try {
      await loadJobs()
    } catch {
      stopPolling()
    }
  }, 3000)
}

function startPolling(id: string) {
  stopPolling()
  pollTimer = setInterval(async () => {
    let job: ExtractJobDetail
    try {
      job = await api.getJob(id)
    } catch {
      // 任务已被删除或后端暂时不可用，停止轮询即可，不打扰用户。
      stopPolling()
      return
    }
    selectedJob.value = job
    await loadJobs()
    if (['completed', 'failed', 'cancelled'].includes(job.status)) {
      stopPolling()
      showToast(`任务结束: 成功 ${job.success_count}，失败 ${job.failure_count}`, job.failure_count ? 'warning' : 'success')
    }
  }, 1500)
}

function stopPolling() {
  if (pollTimer) clearInterval(pollTimer)
  pollTimer = null
}

async function pauseJob(id: string) {
  busyId.value = id
  try {
    await api.pauseJob(id)
    showToast('已请求暂停，当前文件提取完后停止', 'success')
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  } finally {
    busyId.value = ''
  }
  await refreshAfterAction(id)
}

async function resumeJob(id: string) {
  busyId.value = id
  try {
    await api.resumeJob(id)
    showToast('任务已继续，已完成的文件不会重做', 'success')
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  } finally {
    busyId.value = ''
  }
  await refreshAfterAction(id)
}

async function cancelJob(id: string) {
  const ok = await confirmAction('确认取消此任务？已提取的文件会保留，可稍后点「继续」接着做。', {
    title: '取消任务',
    okLabel: '取消任务',
    danger: true,
  })
  if (!ok) return
  busyId.value = id
  try {
    await api.cancelJob(id)
    showToast('任务已取消', 'success')
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  } finally {
    busyId.value = ''
  }
  await refreshAfterAction(id)
}

async function refreshAfterAction(id: string) {
  await loadJobs()
  if (selectedJob.value?.id === id) {
    selectedJob.value = await api.getJob(id)
    startPolling(id)
  }
}

async function deleteJob(id: string) {
  const ok = await confirmAction('确认删除此任务？\n\n只删除任务记录，已提取的音频文件不会被删除。', {
    title: '删除任务',
    okLabel: '删除',
    danger: true,
  })
  if (!ok) return
  busyId.value = id
  try {
    await api.deleteJob(id)
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
    return
  } finally {
    busyId.value = ''
  }
  if (selectedJob.value?.id === id) {
    showModal.value = false
    selectedJob.value = null
  }
  stopPolling()
  await loadJobs()
  startListPolling()
  showToast('任务已删除', 'success')
}

function progressRing(percent: number): string {
  const r = 16, c = 2 * Math.PI * r, offset = c - (percent / 100) * c
  return `<svg viewBox="0 0 40 40" width="40" height="40">
    <circle cx="20" cy="20" r="${r}" fill="none" stroke="var(--bg-inset)" stroke-width="4"/>
    <circle cx="20" cy="20" r="${r}" fill="none" stroke="var(--accent)" stroke-width="4"
            stroke-dasharray="${c}" stroke-dashoffset="${offset}" stroke-linecap="round" transform="rotate(-90 20 20)"/>
    <text x="20" y="20" text-anchor="middle" dominant-baseline="central" font-size="9" font-weight="600" fill="var(--text-primary)">${percent}%</text>
  </svg>`
}

function formatElapsed(seconds?: number): string {
  if (seconds == null) return '—'
  if (seconds < 1) return `${seconds.toFixed(2)} 秒`
  if (seconds < 60) return `${seconds.toFixed(1)} 秒`
  const minutes = Math.floor(seconds / 60)
  const rest = Math.round(seconds % 60)
  return `${minutes} 分 ${rest} 秒`
}
</script>

<template>
  <section class="card">
    <div class="card-header">
      <h2>📋 任务管理</h2>
      <span class="text-muted text-sm">{{ jobs.length }} 个任务</span>
    </div>
    <div class="jobs-list">
      <div v-for="job in jobs" :key="job.id" class="job-item">
        <div class="job-main" @click="selectJob(job.id)">
          <div class="job-ring" v-html="progressRing(job.progress)"></div>
          <div class="job-info">
            <div class="job-name">{{ job.name || job.id.slice(0, 8) }}</div>
            <div class="job-meta">
              成功 {{ job.success_count }} / 失败 {{ job.failure_count }} / 共 {{ job.total_count }}
              · <span class="badge" :class="statusBadgeClass(job.status)">{{ statusLabel(job.status) }}</span>
            </div>
          </div>
        </div>
        <div class="job-actions">
          <button v-if="isRunning(job.status)" class="btn btn-ghost btn-sm" :disabled="busyId === job.id"
                  title="暂停任务" @click.stop="pauseJob(job.id)">⏸ 暂停</button>
          <button v-if="canResume(job.status)" class="btn btn-ghost btn-sm" :disabled="busyId === job.id"
                  title="从上次中断处继续" @click.stop="resumeJob(job.id)">▶ 继续</button>
          <button v-if="isRunning(job.status)" class="btn btn-ghost btn-sm" :disabled="busyId === job.id"
                  title="取消任务" @click.stop="cancelJob(job.id)">✕ 取消</button>
          <button class="btn btn-ghost btn-sm danger-text" :disabled="busyId === job.id || isRunning(job.status)"
                  :title="isRunning(job.status) ? '请先暂停或取消后再删除' : '删除任务记录'"
                  @click.stop="deleteJob(job.id)">🗑 删除</button>
        </div>
      </div>
      <div v-if="!jobs.length" class="empty-state"><p>暂无任务</p></div>
    </div>
  </section>

  <!-- Job Detail Modal — 不响应遮罩点击，避免误触关闭 -->
  <Teleport to="body">
    <div v-if="showModal && selectedJob" class="modal-overlay">
      <div class="modal-dialog modal-xl">
        <div class="modal-header">
          <div>
            <h2>{{ selectedJob.name || '任务详情' }}</h2>
            <p class="text-muted text-sm">{{ selectedJob.source_path }} · {{ statusLabel(selectedJob.status) }}</p>
          </div>
          <div class="modal-actions">
            <button v-if="isRunning(selectedJob.status)" class="btn btn-secondary btn-sm"
                    :disabled="busyId === selectedJob.id" @click="pauseJob(selectedJob.id)">⏸ 暂停</button>
            <button v-if="canResume(selectedJob.status)" class="btn btn-secondary btn-sm"
                    :disabled="busyId === selectedJob.id" @click="resumeJob(selectedJob.id)">▶ 继续</button>
            <button v-if="isRunning(selectedJob.status)" class="btn btn-secondary btn-sm"
                    :disabled="busyId === selectedJob.id" @click="cancelJob(selectedJob.id)">✕ 取消</button>
            <button class="btn btn-danger btn-sm"
                    :disabled="busyId === selectedJob.id || isRunning(selectedJob.status)"
                    @click="deleteJob(selectedJob.id)">删除</button>
            <button class="btn btn-ghost btn-sm" @click="closeModal">关闭</button>
          </div>
        </div>
        <div class="modal-body">
          <div class="summary-grid">
            <div class="stat"><div class="stat-value">{{ selectedJob.progress }}%</div><div class="stat-label">进度</div></div>
            <div class="stat"><div class="stat-value">{{ selectedJob.total_count }}</div><div class="stat-label">总数</div></div>
            <div class="stat"><div class="stat-value">{{ selectedJob.success_count }}</div><div class="stat-label">成功</div></div>
            <div class="stat"><div class="stat-value">{{ selectedJob.failure_count }}</div><div class="stat-label">失败</div></div>
          </div>
          <div class="progress-bar"><div class="progress-fill" :style="{ width: selectedJob.progress + '%' }"></div></div>
          <p v-if="selectedJob.status === 'paused'" class="paused-hint">任务已暂停，点「继续」从中断处接着提取，已完成的文件不会重做。</p>
          <div class="items-list">
            <div class="item-row header"><div>状态</div><div>文件</div><div>提取耗时</div><div>输出</div></div>
            <div v-for="item in selectedJob.items" :key="item.id" class="item-row">
              <div><span class="badge" :class="statusBadgeClass(item.status)">{{ statusLabel(item.status) }}</span></div>
              <div class="truncate">{{ item.title || item.source_path }}</div>
              <div class="elapsed" :class="{ pending: item.duration_seconds == null }">{{ item.status === 'processing' ? '计时中…' : formatElapsed(item.duration_seconds) }}</div>
              <div class="truncate text-muted">
                <button v-if="item.error_message" class="btn btn-ghost btn-sm" title="查看运行日志" @click="logItem = item">📋 日志</button>
                <template v-else>{{ item.output_path || '' }}</template>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </Teleport>

  <Teleport to="body">
    <div v-if="logItem" class="modal-overlay" @click.self="logItem = null">
      <div class="modal-dialog modal-lg">
        <div class="modal-header">
          <div>
            <h2>运行日志</h2>
            <p class="text-muted text-sm">{{ logItem.title || logItem.source_path }}</p>
          </div>
          <div class="modal-actions">
            <button class="btn btn-ghost btn-sm" @click="logItem = null">关闭</button>
          </div>
        </div>
        <div class="modal-body">
          <pre class="log-body">{{ logItem.error_message }}</pre>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.jobs-list { display: flex; flex-direction: column; gap: 8px; }
.job-item {
  display: flex; align-items: center; gap: 12px;
  padding: 14px 16px; border: 1px solid var(--border); border-radius: var(--radius-md);
  transition: all var(--transition);
}
.job-item:hover { border-color: var(--border-strong); box-shadow: var(--shadow-sm); }
.job-main { display: flex; align-items: center; gap: 12px; flex: 1; min-width: 0; cursor: pointer; }
.job-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
.danger-text { color: var(--danger-text); }
.job-ring { width: 40px; height: 40px; flex-shrink: 0; }
.job-info { flex: 1; min-width: 0; }
.job-name { font-size: 14px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.job-meta { font-size: 12px; color: var(--text-muted); margin-top: 2px; }
.empty-state { text-align: center; color: var(--text-muted); padding: 32px; }

.modal-overlay { position: fixed; inset: 0; z-index: 90; display: grid; place-items: center; background: rgba(15,23,42,0.4); backdrop-filter: blur(4px); }
.modal-dialog { position: relative; width: min(960px, calc(100vw - 40px)); border-radius: var(--radius-xl); background: var(--bg-elevated); box-shadow: var(--shadow-xl); overflow: hidden; }
.modal-lg { width: min(760px, calc(100vw - 40px)); }
.log-body {
  max-height: 60vh;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-all;
  font-family: ui-monospace, Consolas, monospace;
  font-size: 12px;
  line-height: 1.5;
  background: var(--bg-subtle);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: 12px;
  margin: 0;
}
.modal-header { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; padding: 20px 24px; border-bottom: 1px solid var(--border); }
.modal-header h2 { font-size: 16px; font-weight: 600; }
.modal-actions { display: flex; gap: 8px; flex-wrap: wrap; justify-content: flex-end; }
.modal-body { max-height: min(65vh, 700px); overflow: auto; padding: 24px; }

.summary-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; margin-bottom: 16px; }
.stat { padding: 14px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-subtle); text-align: center; }
.stat-value { font-size: 24px; font-weight: 700; color: var(--accent); }
.stat-label { font-size: 12px; color: var(--text-muted); margin-top: 2px; }
.progress-bar { height: 8px; border-radius: 999px; background: var(--bg-inset); overflow: hidden; margin-bottom: 16px; }
.progress-fill { height: 100%; border-radius: 999px; background: var(--accent); transition: width 0.3s ease; }
.paused-hint { font-size: 13px; color: var(--warning-text); background: var(--warning-soft); padding: 10px 14px; border-radius: var(--radius-md); margin-bottom: 16px; }
.items-list { border: 1px solid var(--border); border-radius: var(--radius-md); overflow: hidden; }
.item-row { display: grid; grid-template-columns: 80px minmax(180px, 1fr) 100px minmax(180px, 1fr); gap: 10px; padding: 10px 14px; border-bottom: 1px solid var(--border); font-size: 13px; }
.item-row:last-child { border-bottom: none; }
.item-row.header { background: var(--bg-subtle); font-weight: 600; font-size: 12px; color: var(--text-secondary); }
.elapsed { font-variant-numeric: tabular-nums; color: var(--text-secondary); }
.elapsed.pending { color: var(--text-muted); }
</style>
