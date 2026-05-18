<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { api } from '../api'
import { useSettings } from '../composables/useSettings'
import { useToast } from '../composables/useToast'
import { formatDuration } from '../utils'
import FileBrowser from '../components/FileBrowser.vue'
import type { Collection } from '../types'

const { settings } = useSettings()
const { show: showToast } = useToast()

const selectedPath = ref('')
const collections = ref<Collection[]>([])
const selectedCollection = ref<Collection | null>(null)

onMounted(async () => {
  await loadCollections()
})

async function loadCollections() {
  collections.value = await api.getCollections()
}

async function selectCollection(id: string) {
  selectedCollection.value = await api.getCollection(id)
}

async function deleteCollection(id: string) {
  if (!confirm('确认移除此合集？')) return
  await api.deleteCollection(id)
  if (selectedCollection.value?.id === id) selectedCollection.value = null
  await loadCollections()
  showToast('已移除合集', 'success')
}

async function scanSelected() {
  if (!selectedPath.value) { showToast('请先选择文件夹或视频', 'warning'); return }
  try {
    const result = await api.startScan([selectedPath.value])
    await loadCollections()
    if (result.collections.length) {
      await selectCollection(result.collections[0].id)
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

    <!-- Right -->
    <div class="workspace-right">
      <!-- Collections -->
      <section class="card">
        <div class="card-header">
          <h2>📚 已分析合集</h2>
          <span class="text-muted text-sm">{{ collections.length }} 个</span>
        </div>
        <div class="collections-list">
          <div
            v-for="c in collections"
            :key="c.id"
            class="collection-item"
            :class="{ active: selectedCollection?.id === c.id }"
            @click="selectCollection(c.id)"
          >
            <div class="coll-info">
              <div class="coll-name">{{ c.name }}</div>
              <div class="coll-meta">{{ c.episode_count }} 个视频</div>
            </div>
            <button class="coll-remove" @click.stop="deleteCollection(c.id)">×</button>
          </div>
          <div v-if="!collections.length" class="empty-state"><p>还没有已分析合集</p></div>
        </div>
      </section>

      <!-- Detail -->
      <section v-if="selectedCollection" class="card">
        <div class="card-header">
          <h2>{{ selectedCollection.name }}</h2>
          <span class="text-muted text-sm">{{ selectedCollection.source_path }} · {{ selectedCollection.episode_count }} 个视频</span>
        </div>
        <div class="file-list">
          <div class="file-row header"><div>序号</div><div>标题</div><div>时长</div></div>
          <div v-for="(v, i) in selectedCollection.video_files" :key="v.id" class="file-row">
            <div>{{ String(i + 1).padStart(3, '0') }}</div>
            <div class="truncate">{{ v.episode_title }}</div>
            <div>{{ formatDuration(v.duration) }}</div>
          </div>
        </div>
      </section>
      <section v-else class="card empty-card">
        <div class="empty-state"><p>选择文件夹并分析以查看合集详情</p></div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.workspace-layout { display: grid; grid-template-columns: 420px minmax(0, 1fr); gap: 20px; align-items: start; }
.workspace-right { display: flex; flex-direction: column; gap: 20px; }
.browser-card { position: sticky; top: 84px; }
.collections-list { display: flex; flex-direction: column; gap: 8px; }
.collection-item {
  display: flex; align-items: center; gap: 12px;
  padding: 12px; border: 1px solid var(--border); border-radius: var(--radius-md);
  cursor: pointer; transition: all var(--transition);
}
.collection-item:hover { border-color: var(--accent); box-shadow: var(--shadow-sm); }
.collection-item.active { border-color: var(--accent); background: var(--accent-soft); }
.coll-info { flex: 1; min-width: 0; }
.coll-name { font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.coll-meta { font-size: 12px; color: var(--text-muted); }
.coll-remove {
  width: 24px; height: 24px; border: none; border-radius: 50%;
  background: transparent; color: var(--text-muted); cursor: pointer; font-size: 16px;
}
.coll-remove:hover { background: var(--danger-soft); color: var(--danger); }
.file-list { border: 1px solid var(--border); border-radius: var(--radius-md); overflow: hidden; }
.file-row { display: grid; grid-template-columns: 50px 1fr 80px; gap: 10px; padding: 10px 14px; border-bottom: 1px solid var(--border); font-size: 13px; }
.file-row:last-child { border-bottom: none; }
.file-row.header { background: var(--bg-subtle); font-weight: 600; font-size: 12px; color: var(--text-secondary); }
.empty-card { min-height: 200px; display: flex; align-items: center; justify-content: center; }
.empty-state { text-align: center; color: var(--text-muted); padding: 32px; }

@media (max-width: 1024px) { .workspace-layout { grid-template-columns: 1fr; } .browser-card { position: static; } }
</style>
