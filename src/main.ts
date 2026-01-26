import { createApp, watch } from 'vue'
import { createPinia } from 'pinia'
import { createRouter, createWebHistory } from 'vue-router'
import App from './App.vue'
import MainView from './views/MainView.vue'
import SettingsView from './views/SettingsView.vue'
import HistoryView from './views/HistoryView.vue'
import NotificationView from './views/NotificationView.vue'
import { useChatStore } from './stores/chat'
import { useLocaleStore } from './stores/locale'
import { useSkillsStore } from './stores/skills'
import { translate } from './i18n'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'main', component: MainView },
    { path: '/settings', name: 'settings', component: SettingsView },
    { path: '/history', name: 'history', component: HistoryView },
    { path: '/notification', name: 'notification', component: NotificationView },
  ],
})

const pinia = createPinia()
const app = createApp(App)

app.use(pinia)
app.use(router)
app.mount('#app')

const chatStore = useChatStore(pinia)
const localeStore = useLocaleStore(pinia)
const skillsStore = useSkillsStore(pinia)
const startupGraceMs = 2000
let lastAlertTimestamp: string | null = formatLocalTimestamp(new Date(Date.now() - startupGraceMs))

watch(
  localeStore.locale,
  (locale) => {
    document.documentElement.lang = locale === 'zh' ? 'zh-CN' : 'en'
  },
  { immediate: true }
)

async function syncLocaleWithSystem() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    let storedLocale = ''
    let storedVersion = ''
    try {
      storedLocale = localStorage.getItem('opencowork-locale') || ''
      storedVersion = localStorage.getItem('opencowork-locale-version') || ''
    } catch {
      // localStorage unavailable
    }
    const systemLocale = await invoke<string>('get_system_locale', {
      ui_locale: localeStore.locale,
      stored_locale: storedLocale || undefined,
      stored_version: storedVersion || undefined,
    })
    const normalized = String(systemLocale || '')
      .trim()
      .toLowerCase()
      .startsWith('zh')
      ? 'zh'
      : 'en'
    if (normalized !== localeStore.locale) {
      localeStore.setLocale(normalized)
    }
  } catch (error) {
    console.error('Failed to sync system locale:', error)
  }
}

syncLocaleWithSystem()
skillsStore.startSkillsWatcher()

const t = (key: string, params?: Record<string, string | number>) =>
  translate(localeStore.locale, key, params)

function formatLocalTimestamp(date: Date): string {
  const pad = (value: number) => value.toString().padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
}

function formatAlertContent(
  alertTypeRaw: string,
  message: string,
  suggestion?: string
): string {
  const alertTypeLabel = alertTypeRaw && alertTypeRaw !== 'unknown' ? alertTypeRaw : t('common.unknown')
  let content = `${t('alert.detectedTitle')}\n\n`
  content += `${t('alert.typeLine', { type: alertTypeLabel })}\n`
  content += `${t('alert.messageLine', { message })}\n`
  if (suggestion) {
    content += `\n${t('alert.suggestionLine', { suggestion })}`
  }
  return content
}

async function setupAlertListener() {
  try {
    const { listen } = await import('@tauri-apps/api/event')
    const { invoke } = await import('@tauri-apps/api/core')
    await listen<{
      timestamp: string
      issue_type?: string
      error_type?: string
      message: string
      suggestion?: string
      intent?: string
      scene?: string
      help_type?: string
      urgency?: string
      related_skill?: string
    }>('assistant-alert', async (event) => {
      const alert = event.payload
      const alertType = alert.issue_type || alert.error_type || 'unknown'
      const content = formatAlertContent(alertType, alert.message, alert.suggestion)

      // 添加到聊天记录
      chatStore.addAlert({
        role: 'assistant',
        content,
        timestamp: alert.timestamp,
        alertKey: `${alertType}|${alert.message}|${alert.timestamp}`,
      })
      lastAlertTimestamp = alert.timestamp

      // 显示右下角通知窗口
      try {
        await invoke('show_notification', {
          intent: alert.intent || '',
          scene: alert.scene || '',
          helpType: alert.help_type || 'info',
          summary: alert.message || '',
          suggestion: alert.suggestion || '',
          urgency: alert.urgency || 'medium',
        })
      } catch (err) {
        console.error('显示通知窗口失败:', err)
      }
    })
  } catch (error) {
    console.error('设置提醒监听失败:', error)
  }
}

setupAlertListener()

async function pollAlerts() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const alerts = await invoke<Array<{
      timestamp: string
      issue_type: string
      message: string
      suggestion?: string
    }>>('get_recent_alerts', { since: lastAlertTimestamp })

    if (alerts && alerts.length > 0) {
      for (const alert of alerts) {
        const alertType = alert.issue_type || 'unknown'
        const content = formatAlertContent(alertType, alert.message, alert.suggestion)

        chatStore.addAlert({
          role: 'assistant',
          content,
          timestamp: alert.timestamp,
          alertKey: `${alertType}|${alert.message}|${alert.timestamp}`,
        })
        lastAlertTimestamp = alert.timestamp
      }
    }
  } catch (error) {
    console.error('轮询提醒失败:', error)
  }
}

setInterval(pollAlerts, 5000)
