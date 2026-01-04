import { defineStore } from 'pinia'
import { ref, onMounted, onUnmounted } from 'vue'

export const useCaptureStore = defineStore('capture', () => {
  const isCapturing = ref(false)
  const recordCount = ref(0)
  const lastCaptureTime = ref<string | null>(null)

  let statusInterval: number | null = null

  async function startCapture() {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('start_capture')
      isCapturing.value = true
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
    } catch (error) {
      console.error('Failed to refresh status:', error)
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
    startCapture,
    stopCapture,
    refreshStatus,
    startStatusPolling,
    stopStatusPolling,
  }
})
