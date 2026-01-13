import { defineStore } from 'pinia'
import { ref, onMounted, onUnmounted } from 'vue'
import { translate } from '../i18n'
import { useLocaleStore } from './locale'

export const useCaptureStore = defineStore('capture', () => {
  const isCapturing = ref(false)
  const recordCount = ref(0)
  const lastCaptureTime = ref<string | null>(null)
  const desiredCapturing = ref(false)
  const autoRestarting = ref(false)
  const lastEvent = ref<{ id: number; type: 'warning' | 'success' | 'error'; message: string } | null>(null)
  const localeStore = useLocaleStore()
  const t = (key: string, params?: Record<string, string | number>) =>
    translate(localeStore.locale.value, key, params)

  let statusInterval: number | null = null
  let eventSeq = 0
  let lastAutoRestartAt = 0

  function pushEvent(type: 'warning' | 'success' | 'error', message: string) {
    eventSeq += 1
    lastEvent.value = { id: eventSeq, type, message }
  }

  async function startCapture() {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('start_capture')
      isCapturing.value = true
      desiredCapturing.value = true
    } catch (error) {
      console.error('Failed to start capture:', error)
      throw error
    }
  }

  async function stopCapture() {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('stop_capture')
      isCapturing.value = false
      desiredCapturing.value = false
    } catch (error) {
      console.error('Failed to stop capture:', error)
      throw error
    }
  }

  async function refreshStatus() {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      const status = await invoke<{
        is_capturing: boolean
        record_count: number
        last_capture_time: string | null
      }>('get_capture_status')

      isCapturing.value = status.is_capturing
      recordCount.value = status.record_count
      lastCaptureTime.value = status.last_capture_time

      if (desiredCapturing.value && !status.is_capturing) {
        await attemptAutoRestart()
      }
    } catch (error) {
      console.error('Failed to refresh status:', error)
    }
  }

  async function attemptAutoRestart() {
    const now = Date.now()
    if (autoRestarting.value || now - lastAutoRestartAt < 5000) {
      return
    }

    autoRestarting.value = true
    lastAutoRestartAt = now
    pushEvent('warning', t('capture.autoRestarting'))

    try {
      await startCapture()
      pushEvent('success', t('capture.autoRestored'))
    } catch (error) {
      pushEvent('error', t('capture.autoRestoreFailed', { error: String(error) }))
    } finally {
      autoRestarting.value = false
    }
  }

  function startStatusPolling() {
    refreshStatus()
    statusInterval = window.setInterval(refreshStatus, 5000)
  }

  function stopStatusPolling() {
    if (statusInterval !== null) {
      clearInterval(statusInterval)
      statusInterval = null
    }
  }

  return {
    isCapturing,
    recordCount,
    lastCaptureTime,
    desiredCapturing,
    autoRestarting,
    lastEvent,
    startCapture,
    stopCapture,
    refreshStatus,
    startStatusPolling,
    stopStatusPolling,
  }
})
