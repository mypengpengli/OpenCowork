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
    skip_unchanged: boolean
    change_threshold: number
    recent_summary_limit: number
    recent_detail_limit: number
    alert_confidence_threshold: number
    alert_cooldown_seconds: number
  }
  storage: {
    retention_days: number
    max_screenshots: number
    max_context_chars: number
    auto_clear_on_start: boolean
  }
  tools: {
    mode: 'unset' | 'whitelist' | 'allow_all'
    allowed_commands: string[]
    allowed_dirs: string[]
  }
  ui: {
    show_progress: boolean
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
      skip_unchanged: true,
      change_threshold: 0.95,
      recent_summary_limit: 8,
      recent_detail_limit: 3,
      alert_confidence_threshold: 0.7,
      alert_cooldown_seconds: 120,
    },
    storage: {
      retention_days: 7,
      max_screenshots: 10000,
      max_context_chars: 10000,
      auto_clear_on_start: false,
    },
    tools: {
      mode: 'unset',
      allowed_commands: [],
      allowed_dirs: [],
    },
    ui: {
      show_progress: true,
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
