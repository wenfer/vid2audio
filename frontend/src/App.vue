<script setup lang="ts">
import { onMounted } from 'vue'
import { useRoute } from 'vue-router'
import { useSettings } from './composables/useSettings'
import { useToast } from './composables/useToast'
import TopBar from './components/TopBar.vue'
import ToastStack from './components/ToastStack.vue'

const route = useRoute()
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
      <!-- Keep the route outlet direct. Transition wrappers can leave an empty
           outlet when navigation races with a cached/slow reverse proxy. -->
      <router-view :key="route.fullPath" />
    </main>
    <ToastStack />
  </div>
</template>

<style scoped>
</style>
