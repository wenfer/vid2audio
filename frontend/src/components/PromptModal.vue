<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { usePrompt } from '../composables/usePrompt'

const { current, submit, cancel } = usePrompt()
const draft = ref('')
const field = ref<HTMLInputElement | null>(null)

watch(current, async (request) => {
  if (!request) return
  draft.value = request.value ?? ''
  await nextTick()
  field.value?.focus()
  // 重命名时整体选中：多数情况是改个别字，全选比把光标丢在末尾更好改。
  field.value?.select()
})

function submitDraft() {
  const value = draft.value.trim()
  if (!value) return
  submit(value)
}
</script>

<template>
  <Teleport to="body">
    <!-- 和其他弹窗一致：不响应遮罩点击，用按钮或 Esc（输入框上监听）关闭。 -->
    <div v-if="current" class="modal-overlay">
      <div class="prompt-dialog">
        <h2>{{ current.title }}</h2>
        <label class="form-field">
          <span v-if="current.label" class="field-label">{{ current.label }}</span>
          <input
            ref="field"
            v-model="draft"
            :placeholder="current.placeholder"
            @keydown.enter.prevent="submitDraft"
            @keydown.esc.prevent="cancel"
          />
        </label>
        <div class="prompt-actions">
          <button class="btn btn-ghost btn-sm" @click="cancel">取消</button>
          <button class="btn btn-primary btn-sm" :disabled="!draft.trim()" @click="submitDraft">
            {{ current.confirmLabel || '确定' }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
  position: fixed; inset: 0; z-index: 120;
  display: grid; place-items: center;
  background: rgba(15, 23, 42, 0.4);
  backdrop-filter: blur(4px);
}
.prompt-dialog {
  width: min(420px, calc(100vw - 32px));
  display: flex; flex-direction: column; gap: 14px;
  padding: 22px 24px;
  border-radius: var(--radius-xl);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-xl);
}
.prompt-dialog h2 { font-size: 15px; font-weight: 600; }
.prompt-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
