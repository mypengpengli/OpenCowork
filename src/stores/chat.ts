import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
  timestamp: string
  isAlert?: boolean  // 是否是主动提示的警告消息
  alertKey?: string
}

export interface SavedConversation {
  id: string
  title: string
  messages: ChatMessage[]
  createdAt: string
  updatedAt: string
}

const STORAGE_KEY = 'screen-assistant-conversations'
const MAX_HISTORY_FOR_CONTEXT = 10  // 发送给模型的最大对话轮数

export const useChatStore = defineStore('chat', () => {
  const messages = ref<ChatMessage[]>([])
  const savedConversations = ref<SavedConversation[]>([])
  const seenAlerts = new Set<string>()

  // 获取用于发送给模型的对话历史（只取最近N轮，不包含alert）
  const chatHistoryForModel = computed(() => {
    const nonAlertMessages = messages.value.filter(m => !m.isAlert)
    // 取最近的对话（最多 MAX_HISTORY_FOR_CONTEXT * 2 条消息，因为一轮包含user+assistant）
    const maxMessages = MAX_HISTORY_FOR_CONTEXT * 2
    if (nonAlertMessages.length <= maxMessages) {
      return nonAlertMessages
    }
    return nonAlertMessages.slice(-maxMessages)
  })

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

  // 新建对话（清空当前对话）
  function newConversation() {
    messages.value = []
    seenAlerts.clear()
  }

  // 保存当前对话
  function saveCurrentConversation(title?: string) {
    const nonAlertMessages = messages.value.filter(m => !m.isAlert)
    if (nonAlertMessages.length === 0) {
      return null
    }

    const now = new Date().toISOString()
    const id = `conv_${Date.now()}`
    const autoTitle = title || generateTitle(nonAlertMessages)

    const conversation: SavedConversation = {
      id,
      title: autoTitle,
      messages: nonAlertMessages,
      createdAt: now,
      updatedAt: now,
    }

    savedConversations.value.unshift(conversation)
    persistConversations()
    return conversation
  }

  // 加载已保存的对话
  function loadConversation(id: string) {
    const conversation = savedConversations.value.find(c => c.id === id)
    if (conversation) {
      messages.value = [...conversation.messages]
      seenAlerts.clear()
      return true
    }
    return false
  }

  // 删除已保存的对话
  function deleteConversation(id: string) {
    const index = savedConversations.value.findIndex(c => c.id === id)
    if (index !== -1) {
      savedConversations.value.splice(index, 1)
      persistConversations()
      return true
    }
    return false
  }

  // 从 localStorage 加载保存的对话列表
  function loadSavedConversations() {
    try {
      const data = localStorage.getItem(STORAGE_KEY)
      if (data) {
        savedConversations.value = JSON.parse(data)
      }
    } catch (e) {
      console.error('Failed to load saved conversations:', e)
    }
  }

  // 持久化到 localStorage
  function persistConversations() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(savedConversations.value))
    } catch (e) {
      console.error('Failed to persist conversations:', e)
    }
  }

  // 自动生成对话标题（取第一条用户消息的前20个字符）
  function generateTitle(msgs: ChatMessage[]): string {
    const firstUserMsg = msgs.find(m => m.role === 'user')
    if (firstUserMsg) {
      const content = firstUserMsg.content.trim()
      return content.length > 20 ? content.slice(0, 20) + '...' : content
    }
    return '新对话'
  }

  // 初始化时加载保存的对话
  loadSavedConversations()

  return {
    messages,
    savedConversations,
    chatHistoryForModel,
    addMessage,
    addAlert,
    clearMessages,
    newConversation,
    saveCurrentConversation,
    loadConversation,
    deleteConversation,
  }
})
