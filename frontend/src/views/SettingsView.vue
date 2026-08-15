<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useSettings } from '../composables/useSettings'
import { useSystemStatus } from '../composables/useSystemStatus'
import { useToast } from '../composables/useToast'
import { checkForUpdates, installUpdate, isDesktop, pickDirectory } from '../desktop'

const { settings, save } = useSettings()
const { status, load: loadStatus } = useSystemStatus()
const { show: showToast } = useToast()
const saving = ref(false)
const desktop = isDesktop()
const updateState = ref<'idle' | 'checking' | 'current' | 'available' | 'installing' | 'error'>('idle')
const updateVersion = ref('')
const updateError = ref('')
onMounted(loadStatus)

async function checkUpdate() {
  updateState.value = 'checking'
  const result = await checkForUpdates()
  if (result.state === 'available') {
    updateState.value = 'available'
    updateVersion.value = result.version
  } else if (result.state === 'current') {
    updateState.value = 'current'
  } else {
    updateState.value = 'error'
    updateError.value = result.message
  }
}

async function doUpdate() {
  updateState.value = 'installing'
  try {
    await installUpdate()
    showToast('更新已下载并安装，请重启应用生效', 'success')
    updateState.value = 'current'
  } catch (e: unknown) {
    updateState.value = 'error'
    updateError.value = (e as Error).message
  }
}

/** 桌面版：用系统文件夹对话框填路径，省得手输。 */
async function browseScanDirectory() {
  if (!settings.value) return
  const picked = await pickDirectory(settings.value.scan_directories[0])
  if (picked) settings.value.scan_directories[0] = picked
}

async function browseOutputDirectory() {
  if (!settings.value) return
  const picked = await pickDirectory(settings.value.output_directory)
  if (picked) settings.value.output_directory = picked
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
          <label class="form-field">
            <span class="field-label">默认源目录</span>
            <div class="path-field">
              <input v-model="settings.scan_directories[0]" />
              <button v-if="desktop" class="btn btn-ghost btn-sm" title="选择文件夹" @click="browseScanDirectory">📂</button>
            </div>
          </label>
          <label class="form-field">
            <span class="field-label">输出目录</span>
            <div class="path-field">
              <input v-model="settings.output_directory" />
              <button v-if="desktop" class="btn btn-ghost btn-sm" title="选择文件夹" @click="browseOutputDirectory">📂</button>
            </div>
          </label>
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

      <!-- Performance -->
      <fieldset class="settings-group">
        <legend>性能</legend>
        <div class="form-row">
          <label class="form-field">
            <span class="field-label">同时提取任务数</span>
            <input v-model.number="settings.extraction_concurrency" type="number" min="1" max="32" step="1" />
            <span class="field-hint">默认 2。数值越大速度越快，但会增加 CPU、磁盘和 NAS 负载。</span>
          </label>
        </div>
      </fieldset>
    </div>
    <div class="card-actions">
      <button class="btn btn-primary" :disabled="saving" @click="saveSettings">{{ saving ? '保存中…' : '✓ 保存设置' }}</button>
    </div>
    <div v-if="status" class="about">
      <span class="about-item">版本 v{{ status.version }}</span>
      <span class="about-item" :class="{ ok: status.ffmpeg_available }">FFmpeg {{ status.ffmpeg_available ? '✅ 可用' : '❌ 不可用' }}</span>
      <span class="about-item" :class="{ ok: status.ffprobe_available }">ffprobe {{ status.ffprobe_available ? '✅ 可用' : '❌ 不可用' }}</span>
      <span class="about-item update-area">
        <template v-if="desktop">
          <button v-if="updateState === 'idle' || updateState === 'current'" class="btn btn-ghost btn-sm" @click="checkUpdate">🔄 检查更新</button>
          <span v-else-if="updateState === 'checking'">检查更新中…</span>
          <template v-else-if="updateState === 'available'">
            <span class="update-available">发现新版本 v{{ updateVersion }}</span>
            <button class="btn btn-primary btn-sm" @click="doUpdate">立即更新</button>
          </template>
          <span v-else-if="updateState === 'installing'">正在下载并安装更新…</span>
          <span v-else-if="updateState === 'error'" class="update-error">更新检查失败：{{ updateError }}</span>
        </template>
        <span v-else class="text-muted">浏览器版不支持自动更新</span>
      </span>
    </div>
  </section>
</template>

<style scoped>
.settings-sections { display: flex; flex-direction: column; gap: 20px; }
.settings-group { border: 1px solid var(--border); border-radius: var(--radius-md); padding: 16px; margin: 0; }
.settings-group legend { font-size: 13px; font-weight: 600; padding: 0 8px; }
/* 输入框 + 「浏览」按钮并排；按钮只在桌面版出现。 */
.path-field { display: flex; align-items: center; gap: 6px; }
.path-field input { flex: 1; min-width: 0; }
.about {
  display: flex;
  gap: 16px;
  margin-top: 20px;
  padding-top: 12px;
  border-top: 1px dashed var(--border);
  font-size: 12px;
  color: var(--text-secondary);
}
.about-item { white-space: nowrap; }
.update-area { display: flex; align-items: center; gap: 8px; }
.update-available { color: var(--accent-text); font-weight: 600; }
.update-error { color: var(--danger-text); }
</style>
