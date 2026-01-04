import { defineStore } from 'pinia'
import { ref } from 'vue'

export interface AppConfig {
  model: {
    provider: 'api' | 'ollama'
    api: {
      type: 'openai' | 'claude' | 'custom'
      endpoint: string
      api_key: string
      model: string
    }
    ollama: {
      endpoint: string
      model: string
    }
  }
  capture: {
    enabled: boolean
    interval_ms: number
    compress_quality: number
  }
  storage: {
    retention_days: number
    max_screenshots: number
  }
}

export const useSettingsStore = defineStore('settings', () => {
  const config = ref<AppConfig>({
    model: {
      provider: 'api',
      api: {
        type: 'openai',
        endpoint: 'https://api.openai.com/v1',
        api_key: '',
        model: 'gpt-4-vision-preview',
      },
      ollama: {
        endpoint: 'http://localhost:11434',
        model: 'llava',
      },
    },
    capture: {
      enabled: true,
      interval_ms: 1000,
      compress_quality: 80,
    },
    storage: {
      retention_days: 7,
      max_screenshots: 10000,
    },
  })

  const isLoaded = ref(false)

  async function loadConfig() {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      const loadedConfig = await invoke<AppConfig>('get_config')
      if (loadedConfig) {
        config.value = loadedConfig
      }
      isLoaded.value = true
    } catch (error) {
      console.error('Failed to load config:', error)
    }
  }

  async function saveConfig() {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('save_config', { config: config.value })
    } catch (error) {
      console.error('Failed to save config:', error)
      throw error
    }
  }

  return {
    config,
    isLoaded,
    loadConfig,
    saveConfig,
  }
})
