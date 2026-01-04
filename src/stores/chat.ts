import { defineStore } from 'pinia'
import { ref } from 'vue'

export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
  timestamp: string
  isAlert?: boolean  // 是否是主动提示的警告消息
}

export const useChatStore = defineStore('chat', () => {
  const messages = ref<ChatMessage[]>([])

  function addMessage(message: ChatMessage) {
    messages.value.push(message)
  }

  function clearMessages() {
    messages.value = []
  }

  return {
    messages,
    addMessage,
    clearMessages,
  }
})
