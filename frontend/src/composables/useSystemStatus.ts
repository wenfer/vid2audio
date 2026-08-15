import { ref } from 'vue'
import { api } from '../api'
import type { SystemStatus } from '../types'

const status = ref<SystemStatus | null>(null)

export function useSystemStatus() {
  async function load() {
    status.value = await api.getSystemStatus()
  }

  return { status, load }
}
