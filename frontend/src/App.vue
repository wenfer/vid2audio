<script setup lang="ts">
import { onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useSettings } from './composables/useSettings'
import { useToast } from './composables/useToast'
import TopBar from './components/TopBar.vue'
import ToastStack from './components/ToastStack.vue'

const router = useRouter()
const { load: loadSettings } = useSettings()
const { show: showToast } = useToast()

onMounted(async () => {
  try {
    await loadSettings()
  } catch (e: unknown) {
    showToast((e as Error).message, 'error')
  }
})
</script>

<template>
  <div class="app">
    <TopBar />
    <main class="main">
      <router-view v-slot="{ Component }">
        <transition name="fade" mode="out-in">
          <component :is="Component" />
        </transition>
      </router-view>
    </main>
    <ToastStack />
  </div>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
