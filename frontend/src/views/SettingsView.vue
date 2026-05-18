<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useSettings } from '../composables/useSettings'
import { useToast } from '../composables/useToast'
import { api } from '../api'
import type { HardwareAccelInfo } from '../types'

const { settings, save } = useSettings()
const { show: showToast } = useToast()
const accelInfo = ref<HardwareAccelInfo | null>(null)
const saving = ref(false)

onMounted(async () => {
  accelInfo.value = await api.getHardwareAcceleration()
})

async function redetect() {
  accelInfo.value = await api.redetectHardwareAcceleration()
  showToast('硬件加速检测完成', 'success')
}

async function saveSettings() {
  if (!settings.value) return
  saving.value = true
  try {
    await save(settings.value)
    showToast('设置已保存', 'success')
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <section v-if="settings" class="card">
    <div class="card-header"><h2>⚙️ 系统配置</h2></div>
    <div class="settings-sections">
      <!-- Paths -->
      <fieldset class="settings-group">
        <legend>路径设置</legend>
        <div class="form-row">
          <label class="form-field"><span class="field-label">默认源目录</span><input v-model="settings.scan_directories[0]" /></label>
          <label class="form-field"><span class="field-label">输出目录</span><input v-model="settings.output_directory" /></label>
        </div>
      </fieldset>

      <!-- Filtering -->
      <fieldset class="settings-group">
        <legend>文件过滤</legend>
        <div class="form-row">
          <label class="form-field"><span class="field-label">最小文件大小 (MB)</span><input v-model.number="settings.min_file_size_mb" type="number" min="0" step="0.1" /></label>
          <label class="form-field"><span class="field-label">视频后缀白名单</span><input :value="settings.video_extensions.join(', ')" @change="settings.video_extensions = ($event.target as HTMLInputElement).value.split(',').map(s => s.trim()).filter(Boolean)" /></label>
          <label class="form-field"><span class="field-label">过滤后缀</span><input :value="settings.ignored_extensions.join(', ')" @change="settings.ignored_extensions = ($event.target as HTMLInputElement).value.split(',').map(s => s.trim()).filter(Boolean)" /></label>
        </div>
      </fieldset>

      <!-- Output -->
      <fieldset class="settings-group">
        <legend>输出设置</legend>
        <div class="form-row">
          <label class="form-field"><span class="field-label">播放设备</span>
            <select v-model="settings.filesystem_sorting">
              <option value="ntfs">故事机 / U盘 / NAS（推荐）</option>
              <option value="natural">电脑 / 手机播放器</option>
              <option value="name">按文件名排序</option>
            </select>
            <span class="field-hint">故事机模式使用前导零编号确保播放顺序正确</span>
          </label>
          <label class="form-field"><span class="field-label">默认格式</span>
            <select v-model="settings.default_output_format">
              <option value="mp3">MP3（兼容性最好）</option><option value="m4a">M4A</option><option value="flac">FLAC 无损</option><option value="wav">WAV</option><option value="opus">OPUS</option>
            </select>
          </label>
          <label class="form-field"><span class="field-label">质量</span>
            <select v-model="settings.default_quality">
              <option value="economy">经济（64kbps，省空间）</option><option value="standard">标准（128kbps）</option><option value="premium">优质（192kbps）</option><option value="lossless">高质量（320kbps）</option>
            </select>
          </label>
        </div>
      </fieldset>

      <!-- Hardware Acceleration -->
      <fieldset class="settings-group">
        <legend>硬件加速</legend>
        <div v-if="accelInfo" class="accel-panel">
          <div class="accel-status">
            <span class="status-dot" :class="accelInfo.available ? 'available' : 'unavailable'"></span>
            <span>{{ accelInfo.available ? `${accelInfo.supported.length} 个后端可用` : '未检测到' }}</span>
            <span class="text-muted text-sm" style="margin-left:auto">{{ accelInfo.ffmpeg_version ? `FFmpeg ${accelInfo.ffmpeg_version}` : '' }}</span>
            <button class="btn btn-ghost btn-sm" @click="redetect">🔄 重新检测</button>
          </div>
          <div class="backends-grid">
            <div v-for="b in accelInfo.backends" :key="b.id" class="backend-card" :class="{ detected: b.detected, recommended: b.is_recommended }">
              <div class="backend-name">{{ b.name }}</div>
              <div class="backend-desc">{{ b.description }}</div>
              <span v-if="b.is_recommended" class="backend-tag rec">推荐</span>
              <span v-else-if="b.detected" class="backend-tag ok">可用</span>
              <span v-else class="backend-tag na">未检测到</span>
            </div>
          </div>
          <p class="hint-text">{{ accelInfo.note }}</p>
        </div>
        <div class="form-row">
          <label class="form-field"><span class="field-label">加速策略</span>
            <select v-model="settings.hardware_acceleration">
              <option value="auto">自动（推荐）</option><option value="safe">CPU</option>
              <option value="qsv">Intel QSV</option><option value="vaapi">VAAPI</option>
              <option value="cuda">NVIDIA CUDA</option><option value="rkmpp">Rockchip MPP</option>
              <option value="videotoolbox">VideoToolbox</option>
            </select>
          </label>
          <label class="form-field"><span class="field-label">设备路径</span><input v-model="settings.hardware_acceleration_device" placeholder="可留空" /></label>
        </div>
      </fieldset>

      <!-- TTS -->
      <fieldset class="settings-group">
        <legend>TTS 片头</legend>
        <div class="form-row">
          <label class="form-field"><span class="field-label">TTS 通道</span>
            <select v-model="settings.tts_provider">
              <option value="piper">Piper 离线 TTS</option>
              <option value="silent">静音占位</option>
              <option value="disabled">禁用片头</option>
            </select>
          </label>
          <label class="form-field"><span class="field-label">失败策略</span>
            <select v-model="settings.tts_failure_mode">
              <option value="silent">静音占位</option><option value="skip">跳过</option><option value="fail">终止</option>
            </select>
          </label>
          <label class="form-field"><span class="field-label">语音模型</span><input v-model="settings.tts_voice" placeholder="zh_CN-huayan-medium" /></label>
          <label class="form-field"><span class="field-label">语速</span><input v-model="settings.tts_rate" placeholder="+0%" /></label>
        </div>
        <div class="form-row">
          <label class="form-field span-2"><span class="field-label">片头文本模板</span><input v-model="settings.intro_text_template" /></label>
        </div>
      </fieldset>
    </div>
    <div class="card-actions">
      <button class="btn btn-primary" :disabled="saving" @click="saveSettings">{{ saving ? '保存中…' : '✓ 保存设置' }}</button>
    </div>
  </section>
</template>

<style scoped>
.settings-sections { display: flex; flex-direction: column; gap: 20px; }
.settings-group { border: 1px solid var(--border); border-radius: var(--radius-md); padding: 16px; margin: 0; }
.settings-group legend { font-size: 13px; font-weight: 600; padding: 0 8px; }
.accel-panel { margin-bottom: 16px; }
.accel-status { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; font-size: 13px; font-weight: 500; }
.status-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-muted); }
.status-dot.available { background: var(--success); }
.status-dot.unavailable { background: var(--warning); }
.backends-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 8px; margin-bottom: 10px; }
.backend-card {
  position: relative; padding: 10px; border: 1px solid var(--border);
  border-radius: var(--radius-sm); background: var(--bg-elevated); opacity: 0.5;
}
.backend-card.detected { opacity: 1; border-color: var(--success); background: var(--success-soft); }
.backend-card.recommended { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-soft); }
.backend-name { font-size: 12px; font-weight: 600; }
.backend-desc { font-size: 11px; color: var(--text-muted); margin-top: 2px; }
.backend-tag { position: absolute; top: 4px; right: 6px; font-size: 9px; font-weight: 700; padding: 1px 5px; border-radius: 999px; }
.backend-tag.rec { background: var(--accent-soft); color: var(--accent-text); }
.backend-tag.ok { background: var(--success-soft); color: var(--success-text); }
.backend-tag.na { background: var(--bg-inset); color: var(--text-muted); }
.hint-text { font-size: 12px; color: var(--text-muted); margin-top: 8px; }
</style>
