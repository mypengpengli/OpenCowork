import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { translate } from '../i18n'
import { useLocaleStore } from './locale'

export type AttachmentKind = 'image' | 'document'

export interface ChatAttachment {
  id: string
  name: string
  path: string
  size?: number
  kind: AttachmentKind
  mime?: string
}

export interface ToolCallInfo {
  id: string
  name: string
  arguments: string
}

export interface ToolContextMessage {
  role: string
  content?: string
  tool_call_id?: string
  tool_calls?: ToolCallInfo[]
}

export interface ToolStep {
  title: string
  detail?: string
}

export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
  timestamp: string
  isAlert?: boolean  // 是否是主动提示的警告消息
  alertKey?: string
  attachments?: ChatAttachment[]
  toolSteps?: ToolStep[]
  toolContext?: ToolContextMessage[]  // 工具调用上下文
  activeSkill?: string  // 当前活跃的技能
}

export interface SavedConversation {
  id: string
  title: string
  messages: ChatMessage[]
  createdAt: string
  updatedAt: string
}

const STORAGE_KEY = 'opencowork-conversations'
const LEGACY_STORAGE_KEY = 'screen-assistant-conversations'
const MAX_HISTORY_FOR_CONTEXT = 50  // 发送给模型的最大对话轮�?

export const useChatStore = defineStore('chat', () => {
  const messages = ref<ChatMessage[]>([])
  const activeConversationId = ref<string | null>(null)
  const savedConversations = ref<SavedConversation[]>([])
  const conversationVersion = ref(0)
  const seenAlerts = new Set<string>()
  const localeStore = useLocaleStore()
  const t = (key: string, params?: Record<string, string | number>) =>
    translate(localeStore.locale, key, params)

  // 获取用于发送给模型的对话历史（只取最近N轮，不包含alert�?
  const chatHistoryForModel = computed(() => {
    const nonAlertMessages = messages.value.filter(m => !m.isAlert)
    // 取最近的对话（最�?MAX_HISTORY_FOR_CONTEXT * 2 条消息，因为一轮包含user+assistant�?
    const maxMessages = MAX_HISTORY_FOR_CONTEXT * 2
    if (nonAlertMessages.length <= maxMessages) {
      return nonAlertMessages
    }
    return nonAlertMessages.slice(-maxMessages)
  })

  function addMessage(message: ChatMessage) {
    messages.value.push(message)
    saveCurrentConversation()
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
    activeConversationId.value = null
    conversationVersion.value += 1
  }

  // 新建对话（清空当前对话）
  function newConversation() {
    saveCurrentConversation()
    clearMessages()
  }

  // 保存当前对话
  function saveCurrentConversation() {
    syncActiveConversation()
  }

  // 加载已保存的对话
  function loadConversation(id: string) {
    const conversation = savedConversations.value.find(c => c.id === id)
    if (conversation) {
      messages.value = [...conversation.messages]
      seenAlerts.clear()
      activeConversationId.value = conversation.id
      conversationVersion.value += 1
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
      if (activeConversationId.value === id) {
        clearMessages()
      }
      return true
    }
    return false
  }

  // �?localStorage 加载保存的对话列�?
  function loadSavedConversations() {
    try {
      const data = localStorage.getItem(STORAGE_KEY) || localStorage.getItem(LEGACY_STORAGE_KEY)
      if (data) {
        savedConversations.value = JSON.parse(data)
        if (!localStorage.getItem(STORAGE_KEY)) {
          localStorage.setItem(STORAGE_KEY, data)
        }
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

  function syncActiveConversation() {
    const nonAlertMessages = messages.value.filter(m => !m.isAlert)
    if (nonAlertMessages.length === 0) {
      return
    }

    const now = new Date().toISOString()
    const id = activeConversationId.value || `conv_${Date.now()}`
    const title = generateTitle(nonAlertMessages)
    const existingIndex = savedConversations.value.findIndex(c => c.id === id)
    const existing = existingIndex === -1 ? null : savedConversations.value[existingIndex]

    const conversation: SavedConversation = {
      id,
      title,
      messages: nonAlertMessages,
      createdAt: existing?.createdAt || now,
      updatedAt: now,
    }

    if (existingIndex !== -1) {
      savedConversations.value.splice(existingIndex, 1)
    }

    savedConversations.value.unshift(conversation)
    activeConversationId.value = id
    persistConversations()
  }

  // 自动生成对话标题（取第一条用户消息的�?0个字符）
  function generateTitle(msgs: ChatMessage[]): string {
    const firstUserMsg = msgs.find(m => m.role === 'user')
    if (firstUserMsg) {
      const content = firstUserMsg.content.trim()
      return content.length > 20 ? content.slice(0, 20) + '...' : content
    }
    return t('chat.defaultTitle')
  }

  // 初始化时加载保存的对�?
  loadSavedConversations()

  return {
    messages,
    activeConversationId,
    conversationVersion,
    savedConversations,
    chatHistoryForModel,
    addMessage,
    addAlert,
    clearMessages,
    newConversation,
    loadConversation,
    deleteConversation,
  }
})


