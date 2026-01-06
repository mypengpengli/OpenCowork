import { defineStore } from 'pinia'
import { ref } from 'vue'

export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
  timestamp: string
  isAlert?: boolean  // 是否是主动提示的警告消息
  alertKey?: string
}

export const useChatStore = defineStore('chat', () => {
  const messages = ref<ChatMessage[]>([])
  const seenAlerts = new Set<string>()

  function addMessage(message: ChatMessage) {
    messages.value.push(message)
  }

  function addAlert(message: ChatMessage) {
    const key = message.alertKey || `${message.timestamp}|${message.content}`
    if (seenAlerts.has(key)) {
      return
    }
    seenAlerts.add(key)
    messages.value.push({ ...message, isAlert: true, alertKey: key })
  }

  function clearMessages() {
    messages.value = []
    seenAlerts.clear()
  }

  return {
    messages,
    addMessage,
    addAlert,
    clearMessages,
  }
})
