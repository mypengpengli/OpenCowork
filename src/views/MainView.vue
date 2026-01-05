<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { NLayout, NLayoutContent, NInput, NButton, NSpace, NSpin, NTag, NIcon, useMessage } from 'naive-ui'
import { Send, PlayCircleOutline, StopCircleOutline } from '@vicons/ionicons5'
import { useChatStore } from '../stores/chat'
import { useCaptureStore } from '../stores/capture'
import MessageItem from '../components/Chat/MessageItem.vue'

const chatStore = useChatStore()
const captureStore = useCaptureStore()
const message = useMessage()

const inputMessage = ref('')
const messagesContainer = ref<HTMLElement | null>(null)
const isLoading = ref(false)

let unlistenAlert: (() => void) | null = null

watch(
  () => captureStore.lastEvent,
  (event) => {
    if (!event) return
    if (event.type === 'warning') {
      message.warning(event.message)
    } else if (event.type === 'success') {
      message.success(event.message)
    } else {
      message.error(event.message)
    }
  }
)

async function sendMessage() {
  if (!inputMessage.value.trim() || isLoading.value) return

  const userMessage = inputMessage.value.trim()
  inputMessage.value = ''

  chatStore.addMessage({
    role: 'user',
    content: userMessage,
    timestamp: new Date().toISOString()
  })

  isLoading.value = true

  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const response = await invoke<string>('chat_with_assistant', {
      message: userMessage
    })

    chatStore.addMessage({
      role: 'assistant',
      content: response,
      timestamp: new Date().toISOString()
    })
  } catch (error) {
    chatStore.addMessage({
      role: 'assistant',
      content: `错误: ${error}`,
      timestamp: new Date().toISOString()
    })
  } finally {
    isLoading.value = false
    await nextTick()
    scrollToBottom()
  }
}

function scrollToBottom() {
  if (messagesContainer.value) {
    messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    sendMessage()
  }
}

async function toggleCapture() {
  try {
    if (captureStore.isCapturing) {
      await captureStore.stopCapture()
    } else {
      await captureStore.startCapture()
    }
  } catch (error) {
    console.error('切换监控状态失败:', error)
  }
}

async function setupAlertListener() {
  try {
    const { listen } = await import('@tauri-apps/api/event')

    unlistenAlert = await listen<{
      timestamp: string
      error_type: string
      message: string
      suggestion: string
    }>('assistant-alert', (event) => {
      const alert = event.payload

      // 构建提示消息
      let content = `⚠️ **检测到错误**\n\n`
      content += `**类型**: ${alert.error_type}\n`
      content += `**信息**: ${alert.message}\n`
      if (alert.suggestion) {
        content += `\n**建议**: ${alert.suggestion}`
      }

      chatStore.addMessage({
        role: 'assistant',
        content,
        timestamp: alert.timestamp,
        isAlert: true
      })

      nextTick(() => scrollToBottom())
    })
  } catch (error) {
    console.error('设置事件监听失败:', error)
  }
}

onMounted(async () => {
  scrollToBottom()
  await setupAlertListener()
  captureStore.startStatusPolling()
})

onUnmounted(() => {
  if (unlistenAlert) {
    unlistenAlert()
  }
  captureStore.stopStatusPolling()
})
</script>

<template>
  <NLayout class="main-layout">
    <NLayoutContent class="main-content">
      <!-- 状态栏 -->
      <div class="status-bar">
        <NSpace justify="space-between" align="center" style="width: 100%">
          <NSpace>
            <NTag :type="captureStore.isCapturing ? 'success' : 'default'" size="small">
              {{ captureStore.isCapturing ? '监控中' : '已暂停' }}
            </NTag>
            <NTag type="info" size="small">
              记录: {{ captureStore.recordCount }}
            </NTag>
          </NSpace>
          <NButton
            size="small"
            :type="captureStore.isCapturing ? 'error' : 'success'"
            @click="toggleCapture"
          >
            <template #icon>
              <NIcon>
                <StopCircleOutline v-if="captureStore.isCapturing" />
                <PlayCircleOutline v-else />
              </NIcon>
            </template>
            {{ captureStore.isCapturing ? '停止' : '开始' }}
          </NButton>
        </NSpace>
      </div>

      <!-- 消息列表 -->
      <div class="messages-container" ref="messagesContainer">
        <div v-if="chatStore.messages.length === 0" class="empty-state">
          <h2>Screen Assistant</h2>
          <p>我会记录你的屏幕操作，随时可以问我：</p>
          <ul>
            <li>刚才我做了什么？</li>
            <li>帮我回顾一下过去10分钟的操作</li>
            <li>我刚才在哪个文件里修改了代码？</li>
          </ul>
          <p style="margin-top: 20px; color: #63e2b7;">
            点击右上角「开始」按钮启动监控
          </p>
        </div>

        <MessageItem
          v-for="(msg, index) in chatStore.messages"
          :key="index"
          :message="msg"
        />

        <div v-if="isLoading" class="loading-indicator">
          <NSpin size="small" />
          <span>思考中...</span>
        </div>
      </div>

      <!-- 输入区域 -->
      <div class="input-area">
        <NInput
          v-model:value="inputMessage"
          type="textarea"
          placeholder="输入你的问题..."
          :autosize="{ minRows: 1, maxRows: 4 }"
          @keydown="handleKeydown"
        />
        <NButton
          type="primary"
          :disabled="!inputMessage.trim() || isLoading"
          @click="sendMessage"
        >
          <template #icon>
            <NIcon><Send /></NIcon>
          </template>
        </NButton>
      </div>
    </NLayoutContent>
  </NLayout>
</template>

<style scoped>
.main-layout {
  height: 100%;
}

.main-content {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 16px;
}

.status-bar {
  padding: 8px 0;
  border-bottom: 1px solid rgba(255, 255, 255, 0.09);
  margin-bottom: 16px;
}

.messages-container {
  flex: 1;
  overflow-y: auto;
  padding: 16px 0;
}

.empty-state {
  text-align: center;
  color: rgba(255, 255, 255, 0.6);
  padding: 40px;
}

.empty-state h2 {
  color: #63e2b7;
  margin-bottom: 16px;
}

.empty-state ul {
  text-align: left;
  display: inline-block;
  margin-top: 16px;
}

.empty-state li {
  margin: 8px 0;
}

.loading-indicator {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 16px;
  color: rgba(255, 255, 255, 0.6);
}

.input-area {
  display: flex;
  gap: 12px;
  padding-top: 16px;
  border-top: 1px solid rgba(255, 255, 255, 0.09);
}

.input-area :deep(.n-input) {
  flex: 1;
}
</style>
