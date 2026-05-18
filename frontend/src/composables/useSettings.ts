import { ref } from 'vue'
import { api } from '../api'
import type { AppSettings } from '../types'

const settings = ref<AppSettings | null>(null)

export function useSettings() {
  async function load() {
    settings.value = await api.getSettings()
  }

  async function save(data: Partial<AppSettings>) {
    settings.value = await api.updateSettings(data)
  }

  return { settings, load, save }
}
