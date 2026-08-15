<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useRouter } from 'vue-router'
import { api } from '../api'
import { useSettings } from '../composables/useSettings'
import { useToast } from '../composables/useToast'
import FileBrowser from './FileBrowser.vue'
import { confirmAction, isDesktop, pickDirectory } from '../desktop'
import type { Collection } from '../types'

const props = defineProps<{
  visible: boolean
  initialCollection?: Collection | null
  initialPath?: string
}>()
const emit = defineEmits<{ 'update:visible': [v: boolean] }>()
const router = useRouter()
const { settings } = useSettings()
const { show: showToast } = useToast()

const step = ref(1)
const selectedPath = ref('')
const collection = ref<Collection | null>(null)
const loading = ref(false)

// Step 2 form
const jobName = ref('')
const format = ref('mp3')
const quality = ref('standard')
const sampleRate = ref(44100)
const trackIndex = ref(0)
const trimStart = ref(0)
const trimEnd = ref(0)
const outputDirectory = ref('')
const tracks = ref<{ index: number; label: string }[]>([{ index: 0, label: '默认音轨' }])

const hasAnalyzedCollection = computed(() => !!props.initialCollection)

const canNext = computed(() => {
  if (step.value === 1) return !!selectedPath.value
  return true
})

function applyCollection(value: Collection | null | undefined) {
  if (!value) return
  collection.value = value
  selectedPath.value = props.initialPath || value.source_path
  jobName.value = value.name
  format.value = settings.value?.default_output_format || 'mp3'
  quality.value = settings.value?.default_quality || 'standard'
  sampleRate.value = settings.value?.default_sample_rate || 44100
  outputDirectory.value = settings.value?.output_directory || ''
  const first = value.video_files.find((v) => v.audio_tracks.length)
  tracks.value = first
    ? first.audio_tracks.map((t) => ({
        index: t.index,
        label: `${t.language_full || '未知'} · ${t.codec || 'audio'} · ${t.channels || '?'}ch`,
      }))
    : [{ index: 0, label: '默认音轨' }]
  trackIndex.value = tracks.value[0]?.index || 0
  step.value = 2
}

watch(() => props.initialCollection, (value) => {
  if (props.visible) applyCollection(value)
})

watch(() => props.visible, (visible) => {
  if (visible && props.initialCollection) applyCollection(props.initialCollection)
})

function close() {
  emit('update:visible', false)
  step.value = 1
  selectedPath.value = ''
  collection.value = null
  outputDirectory.value = ''
}

/// 第 2/3 步已经填过表单，关闭前确认一次，避免误点丢掉填好的内容。
async function requestClose() {
  if (step.value > 1) {
    const ok = await confirmAction('确认关闭？已填写的任务设置将不会保存。', {
      title: '关闭向导',
      okLabel: '关闭',
    })
    if (!ok) return
  }
  close()
}

/** 桌面版：用系统文件夹对话框选输出目录。 */
async function browseOutputDirectory() {
  const picked = await pickDirectory(outputDirectory.value || settings.value?.output_directory)
  if (picked) outputDirectory.value = picked
}

async function next() {
  if (step.value === 1) {
    if (!selectedPath.value) return
    loading.value = true
    try {
      const result = await api.startScan([selectedPath.value])
      if (!result.collections.length) {
        showToast('没有找到符合过滤条件的视频文件', 'error')
        return
      }
      collection.value = await api.getCollection(result.collections[0].id)
      // Populate defaults
      jobName.value = collection.value.name
      format.value = settings.value?.default_output_format || 'mp3'
      quality.value = settings.value?.default_quality || 'standard'
      sampleRate.value = settings.value?.default_sample_rate || 44100
      outputDirectory.value = settings.value?.output_directory || ''
      // Build track list
      const first = collection.value.video_files.find((v) => v.audio_tracks.length)
      tracks.value = first
        ? first.audio_tracks.map((t) => ({
            index: t.index,
            label: `${t.language_full || '未知'} · ${t.codec || 'audio'} · ${t.channels || '?'}ch`,
          }))
        : [{ index: 0, label: '默认音轨' }]
      trackIndex.value = tracks.value[0]?.index || 0
      showToast(`分析完成: ${result.files_found} 个视频`, 'success')
      step.value = 2
    } catch (e: unknown) {
      showToast((e as Error).message, 'error')
    } finally {
      loading.value = false
    }
  } else if (step.value === 2) {
    step.value = 3
  }
}

function prev() {
  if (step.value > 1) step.value--
}

async function startExtract() {
  if (!collection.value) return
  loading.value = true
  try {
    const job = await api.createJob({
      collection_id: collection.value.id,
      job_name: jobName.value || collection.value.name,
      track_index: trackIndex.value,
      output_format: format.value,
      quality: quality.value,
      sample_rate: sampleRate.value,
      trim_start_seconds: trimStart.value,
      trim_end_seconds: trimEnd.value,
      filesystem_sorting: settings.value?.filesystem_sorting || 'ntfs',
      padding_digits: settings.value?.padding_digits || 'auto',
      output_directory: outputDirectory.value.trim() || undefined,
    })
    close()
    showToast('任务已创建，正在提取…', 'success')
    router.push('/tasks')
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  } finally {
    loading.value = false
  }
}

const qualityLabels: Record<string, string> = {
  economy: '经济 64kbps', standard: '标准 128kbps', premium: '优质 192kbps', lossless: '高质量 320kbps'
}
const desktop = isDesktop()
</script>

<template>
  <Teleport to="body">
    <!-- 不响应遮罩点击：误触会丢掉已填的设置，只能通过按钮关闭。 -->
    <div v-if="visible" class="modal-overlay">
      <div class="wizard-dialog">
        <!-- Header -->
        <div class="wizard-header">
          <h2>快速创建任务</h2>
          <div class="wizard-steps">
            <div class="wiz-step" :class="{ active: step === 1, done: step > 1 }"><span class="step-num">1</span> 选择源</div>
            <div class="wiz-line"></div>
            <div class="wiz-step" :class="{ active: step === 2, done: step > 2 }"><span class="step-num">2</span> 设置</div>
            <div class="wiz-line"></div>
            <div class="wiz-step" :class="{ active: step === 3 }"><span class="step-num">3</span> 开始</div>
          </div>
          <button class="btn-close" title="关闭" @click="requestClose">✕</button>
        </div>

        <!-- Step 1 -->
        <div v-show="step === 1" class="wizard-body">
          <div class="step-intro">
            <h3>选择视频文件夹或单个视频</h3>
            <p>浏览文件系统，单击选择，双击进入文件夹。</p>
          </div>
          <FileBrowser
            v-model="selectedPath"
            :initial-path="settings?.scan_directories[0] || ''"
          />
          <div class="selection-bar" :class="{ selected: !!selectedPath }">
            {{ selectedPath ? `✓ 已选择: ${selectedPath.split('/').pop()}` : '未选择文件' }}
          </div>
        </div>

        <!-- Step 2 -->
        <div v-show="step === 2" class="wizard-body">
          <div class="step-intro">
            <h3>{{ hasAnalyzedCollection ? '为已分析合集创建任务' : '确认提取设置' }}</h3>
            <p v-if="hasAnalyzedCollection" class="analyzed-hint">✓ 已分析 {{ collection?.episode_count || 0 }} 个视频，可以直接创建任务</p>
            <p v-else>默认设置适合大多数场景，可直接点击下一步。</p>
          </div>
          <div class="settings-form">
            <div class="form-row">
              <label class="form-field span-2">
                <span class="field-label">任务名称</span>
                <input v-model="jobName" />
              </label>
            </div>
            <div class="form-row">
              <label class="form-field span-2">
                <span class="field-label">输出目录</span>
                <div class="path-field">
                  <input v-model="outputDirectory" placeholder="留空使用设置中的默认输出目录" />
                  <button v-if="desktop" class="btn btn-ghost btn-sm" title="选择文件夹" @click="browseOutputDirectory">📂</button>
                </div>
              </label>
            </div>
            <div class="form-row">
              <label class="form-field">
                <span class="field-label">音轨</span>
                <select v-model.number="trackIndex">
                  <option v-for="t in tracks" :key="t.index" :value="t.index">{{ t.label }}</option>
                </select>
              </label>
              <label class="form-field">
                <span class="field-label">格式</span>
                <select v-model="format">
                  <option value="mp3">MP3</option>
                  <option value="m4a">AAC/M4A</option>
                  <option value="flac">FLAC</option>
                  <option value="wav">WAV</option>
                  <option value="opus">OPUS</option>
                </select>
              </label>
              <label class="form-field">
                <span class="field-label">质量</span>
                <select v-model="quality">
                  <option value="economy">经济 64kbps</option>
                  <option value="standard">标准 128kbps</option>
                  <option value="premium">优质 192kbps</option>
                  <option value="lossless">高质量 320kbps</option>
                </select>
              </label>
            </div>
            <div class="form-row">
              <label class="form-field">
                <span class="field-label">采样率</span>
                <select v-model.number="sampleRate">
                  <option :value="22050">22050Hz</option>
                  <option :value="44100">44100Hz</option>
                  <option :value="48000">48000Hz</option>
                </select>
              </label>
              <label class="form-field">
                <span class="field-label">开头偏移秒</span>
                <input v-model.number="trimStart" type="number" min="0" step="0.1" />
              </label>
              <label class="form-field">
                <span class="field-label">结尾偏移秒</span>
                <input v-model.number="trimEnd" type="number" min="0" step="0.1" />
              </label>
            </div>
          </div>
        </div>

        <!-- Step 3 -->
        <div v-show="step === 3" class="wizard-body">
          <div class="step-intro">
            <h3>确认并开始</h3>
            <p>检查以下信息，点击「开始提取」创建任务。</p>
          </div>
          <div class="summary-list">
            <div class="summary-row"><span class="sum-icon">📁</span><div><div class="sum-label">源路径</div><div class="sum-value">{{ selectedPath }}</div></div></div>
            <div class="summary-row"><span class="sum-icon">📝</span><div><div class="sum-label">任务名称</div><div class="sum-value">{{ jobName }}</div></div></div>
            <div class="summary-row"><span class="sum-icon">🎬</span><div><div class="sum-label">视频数量</div><div class="sum-value">{{ collection?.episode_count || 0 }} 个</div></div></div>
            <div class="summary-row"><span class="sum-icon">🎵</span><div><div class="sum-label">输出</div><div class="sum-value">{{ format.toUpperCase() }} · {{ qualityLabels[quality] || quality }}</div></div></div>
            <div class="summary-row"><span class="sum-icon">📂</span><div><div class="sum-label">输出目录</div><div class="sum-value">{{ outputDirectory || settings?.output_directory }}/{{ collection?.name }}/</div></div></div>
          </div>
        </div>

        <!-- Footer -->
        <div class="wizard-footer">
          <button class="btn btn-ghost" :disabled="step === 1" @click="prev">上一步</button>
          <div class="footer-right">
            <button v-if="step < 3" class="btn btn-primary" :disabled="!canNext || loading" @click="next">
              {{ loading ? '处理中…' : '下一步' }}
            </button>
            <button v-else class="btn btn-primary" :disabled="loading" @click="startExtract">
              {{ loading ? '创建中…' : '🎵 开始提取' }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed; inset: 0; z-index: 90;
  display: grid; place-items: center;
  background: rgba(15, 23, 42, 0.4);
  backdrop-filter: blur(4px);
}
.wizard-dialog {
  width: min(720px, calc(100vw - 32px));
  max-height: min(85vh, 760px);
  display: flex; flex-direction: column;
  border-radius: var(--radius-xl);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-xl);
  overflow: hidden;
  animation: modalIn 0.2s ease;
}
@keyframes modalIn {
  from { opacity: 0; transform: scale(0.96) translateY(8px); }
  to { opacity: 1; transform: scale(1) translateY(0); }
}
.wizard-header {
  display: flex; align-items: center; gap: 16px;
  padding: 20px 24px; border-bottom: 1px solid var(--border);
}
.wizard-header h2 { font-size: 16px; font-weight: 600; flex-shrink: 0; }
.wizard-steps { display: flex; align-items: center; gap: 0; margin-left: auto; margin-right: 16px; }
.wiz-step {
  display: flex; align-items: center; gap: 6px;
  padding: 4px 10px; border-radius: 999px;
  font-size: 12px; color: var(--text-muted); font-weight: 500;
}
.wiz-step .step-num {
  width: 22px; height: 22px;
  display: flex; align-items: center; justify-content: center;
  border-radius: 50%; background: var(--bg-inset);
  font-weight: 700; font-size: 11px;
}
.wiz-step.active { color: var(--accent-text); }
.wiz-step.active .step-num { background: var(--accent); color: white; }
.wiz-step.done { color: var(--success-text); }
.wiz-step.done .step-num { background: var(--success); color: white; }
.wiz-line { width: 20px; height: 2px; background: var(--bg-inset); margin: 0 4px; }
.btn-close {
  width: 30px; height: 30px; border: none; border-radius: var(--radius-sm);
  background: transparent; cursor: pointer; font-size: 16px; color: var(--text-muted);
}
.btn-close:hover { background: var(--bg-subtle); }
.wizard-body { flex: 1; overflow-y: auto; padding: 24px; }
.step-intro { margin-bottom: 20px; }
.step-intro h3 { font-size: 15px; font-weight: 600; margin-bottom: 4px; }
.step-intro p { font-size: 13px; color: var(--text-muted); }
.selection-bar {
  margin-top: 12px; padding: 10px 14px;
  border: 1px solid var(--border); border-radius: var(--radius-md);
  background: var(--bg-subtle); font-size: 13px; color: var(--text-muted);
}
.selection-bar.selected { border-color: var(--accent); background: var(--accent-soft); color: var(--accent-text); font-weight: 500; }
.settings-form {
  padding: 16px; border: 1px solid var(--border);
  border-radius: var(--radius-md); background: var(--bg-subtle);
}
.analyzed-hint { color: var(--success-text); font-size: 13px; }
.summary-list { display: flex; flex-direction: column; gap: 10px; }
.summary-row {
  display: flex; align-items: center; gap: 12px;
  padding: 12px 14px; border: 1px solid var(--border);
  border-radius: var(--radius-md); background: var(--bg-elevated);
}
.sum-icon { font-size: 20px; flex-shrink: 0; }
.sum-label { font-size: 11px; color: var(--text-muted); text-transform: uppercase; font-weight: 600; }
.sum-value { font-size: 14px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.wizard-footer {
  display: flex; align-items: center; justify-content: space-between;
  padding: 16px 24px; border-top: 1px solid var(--border);
}
.footer-right { display: flex; gap: 8px; }
/* 输入框 + 「浏览」按钮并排；按钮只在桌面版出现。 */
.path-field { display: flex; align-items: center; gap: 6px; }
.path-field input { flex: 1; min-width: 0; }
</style>
