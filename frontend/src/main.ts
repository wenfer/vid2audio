import { createApp } from 'vue'
import { createRouter, createWebHashHistory } from 'vue-router'
import App from './App.vue'
import SettingsView from './views/SettingsView.vue'
import TasksView from './views/TasksView.vue'
import WorkspaceView from './views/WorkspaceView.vue'
import './styles/main.css'

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', redirect: '/workspace' },
    { path: '/workspace', component: WorkspaceView },
    { path: '/tasks', component: TasksView },
    { path: '/settings', component: SettingsView },
  ],
})

const app = createApp(App)
app.use(router)
router.isReady().then(() => app.mount('#app'))
