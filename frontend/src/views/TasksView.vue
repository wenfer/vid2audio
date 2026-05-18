<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { api } from '../api'
import { useToast } from '../composables/useToast'
import { statusBadgeClass } from '../utils'
import type { ExtractJob, ExtractJobDetail } from '../types'

const { show: showToast } = useToast()
const jobs = ref<ExtractJob[]>([])
const selectedJob = ref<ExtractJobDetail | null>(null)
const showModal = ref(false)
let pollTimer: ReturnType<typeof setInterval> | null = null

onMounted(loadJobs)
onUnmounted(() => { if (pollTimer) clearInterval(pollTimer) })

async function loadJobs() {
  jobs.value = await api.getJobs()
}

async function selectJob(id: string) {
  selectedJob.value = await api.getJob(id)
  showModal.value = true
  startPolling(id)
}

function startPolling(id: string) {
  if (pollTimer) clearInterval(pollTimer)
  pollTimer = setInterval(async () => {
    const job = await api.getJob(id)
    selectedJob.value = job
    await loadJobs()
    if (['completed', 'failed', 'cancelled'].includes(job.status)) {
      clearInterval(pollTimer!)
      pollTimer = null
      showToast(`任务结束: 成功 ${job.success_count}，失败 ${job.failure_count}`, job.failure_count ? 'warning' : 'success')
    }
  }, 1500)
}

async function deleteJob() {
  if (!selectedJob.value || !confirm('确认删除此任务？')) return
  await api.deleteJob(selectedJob.value.id)
  showModal.value = false
  selectedJob.value = null
  await loadJobs()
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
</script>

<template>
  <section class="card">
    <div class="card-header">
      <h2>📋 任务管理</h2>
      <span class="text-muted text-sm">{{ jobs.length }} 个任务</span>
    </div>
    <div class="jobs-list">
      <div v-for="job in jobs" :key="job.id" class="job-item" @click="selectJob(job.id)">
        <div class="job-ring" v-html="progressRing(job.progress)"></div>
        <div class="job-info">
          <div class="job-name">{{ job.name || job.id.slice(0, 8) }}</div>
          <div class="job-meta">
            成功 {{ job.success_count }} / 失败 {{ job.failure_count }}
            · <span class="badge" :class="statusBadgeClass(job.status)">{{ job.status }}</span>
          </div>
        </div>
      </div>
      <div v-if="!jobs.length" class="empty-state"><p>暂无任务</p></div>
    </div>
  </section>

  <!-- Job Detail Modal -->
  <Teleport to="body">
    <div v-if="showModal && selectedJob" class="modal-overlay" @click.self="showModal = false">
      <div class="modal-dialog modal-xl">
        <div class="modal-header">
          <div>
            <h2>{{ selectedJob.name || '任务详情' }}</h2>
            <p class="text-muted text-sm">{{ selectedJob.source_path }} · {{ selectedJob.status }}</p>
          </div>
          <div class="modal-actions">
            <button class="btn btn-danger btn-sm" @click="deleteJob">删除</button>
            <button class="btn btn-ghost btn-sm" @click="showModal = false">关闭</button>
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
          <div class="items-list">
            <div class="item-row header"><div>状态</div><div>文件</div><div>输出</div></div>
            <div v-for="item in selectedJob.items" :key="item.id" class="item-row">
              <div><span class="badge" :class="statusBadgeClass(item.status)">{{ item.status }}</span></div>
              <div class="truncate">{{ item.title || item.source_path }}</div>
              <div class="truncate text-muted">{{ item.output_path || item.error_message || '' }}</div>
            </div>
          </div>
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
  cursor: pointer; transition: all var(--transition);
}
.job-item:hover { border-color: var(--border-strong); box-shadow: var(--shadow-sm); }
.job-ring { width: 40px; height: 40px; flex-shrink: 0; }
.job-info { flex: 1; min-width: 0; }
.job-name { font-size: 14px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.job-meta { font-size: 12px; color: var(--text-muted); margin-top: 2px; }
.empty-state { text-align: center; color: var(--text-muted); padding: 32px; }

.modal-overlay { position: fixed; inset: 0; z-index: 90; display: grid; place-items: center; background: rgba(15,23,42,0.4); backdrop-filter: blur(4px); }
.modal-dialog { position: relative; width: min(960px, calc(100vw - 40px)); border-radius: var(--radius-xl); background: var(--bg-elevated); box-shadow: var(--shadow-xl); overflow: hidden; }
.modal-header { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; padding: 20px 24px; border-bottom: 1px solid var(--border); }
.modal-header h2 { font-size: 16px; font-weight: 600; }
.modal-actions { display: flex; gap: 8px; }
.modal-body { max-height: min(65vh, 700px); overflow: auto; padding: 24px; }

.summary-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; margin-bottom: 16px; }
.stat { padding: 14px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-subtle); text-align: center; }
.stat-value { font-size: 24px; font-weight: 700; color: var(--accent); }
.stat-label { font-size: 12px; color: var(--text-muted); margin-top: 2px; }
.progress-bar { height: 8px; border-radius: 999px; background: var(--bg-inset); overflow: hidden; margin-bottom: 16px; }
.progress-fill { height: 100%; border-radius: 999px; background: var(--accent); transition: width 0.3s ease; }
.items-list { border: 1px solid var(--border); border-radius: var(--radius-md); overflow: hidden; }
.item-row { display: grid; grid-template-columns: 80px 1fr 1fr; gap: 10px; padding: 10px 14px; border-bottom: 1px solid var(--border); font-size: 13px; }
.item-row:last-child { border-bottom: none; }
.item-row.header { background: var(--bg-subtle); font-weight: 600; font-size: 12px; color: var(--text-secondary); }
</style>
