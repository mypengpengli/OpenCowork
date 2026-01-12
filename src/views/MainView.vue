<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { NLayout, NLayoutContent, NInput, NButton, NSpace, NSpin, NTag, NIcon, NDropdown, useMessage } from 'naive-ui'
import { Send, PlayCircleOutline, StopCircleOutline, AddOutline, SaveOutline } from '@vicons/ionicons5'
import { useChatStore } from '../stores/chat'
import { useCaptureStore } from '../stores/capture'
import { useSkillsStore } from '../stores/skills'
import MessageItem from '../components/Chat/MessageItem.vue'

const chatStore = useChatStore()
const captureStore = useCaptureStore()
const skillsStore = useSkillsStore()
const message = useMessage()

const inputMessage = ref('')
const messagesContainer = ref<HTMLElement | null>(null)
const isLoading = ref(false)
const isHistoryLoading = ref(false)

// Skill 提示相关
const showSkillHints = ref(false)
const skillFilterText = ref('')
const selectedSkillIndex = ref(0)

// 过滤后的 Skills 列表
const filteredSkills = computed(() => {
  const skills = skillsStore.availableSkills.filter(s => s.user_invocable !== false)
  if (!skillFilterText.value) return skills
  const filter = skillFilterText.value.toLowerCase()
  return skills.filter(s =>
    s.name.toLowerCase().includes(filter) ||
    s.description.toLowerCase().includes(filter)
  )
})

// 监听输入变化，检测 / 触发
watch(inputMessage, (newVal) => {
  // 检测是否以 / 开头
  if (newVal.startsWith('/')) {
    const afterSlash = newVal.slice(1)
    // 如果 / 后面没有空格，显示提示
    if (!afterSlash.includes(' ')) {
      skillFilterText.value = afterSlash
      showSkillHints.value = true
      selectedSkillIndex.value = 0
    } else {
      showSkillHints.value = false
    }
  } else {
    showSkillHints.value = false
  }
})

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

watch(
  () => chatStore.messages.length,
  async () => {
    await nextTick()
    scrollToBottom()
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
    // Get chat history for context (excluding the message we just added)
    const historyForModel = chatStore.chatHistoryForModel
      .slice(0, -1)  // Exclude the user message we just added
      .map(m => ({ role: m.role, content: m.content }))

    let response: string

    // 检测 /skill-name 语法
    const skillMatch = userMessage.match(/^\/([a-z0-9-]+)(?:\s+(.*))?$/i)
    if (skillMatch) {
      const [, skillName, args] = skillMatch

      // 显示正在调用 skill 的提示
      chatStore.addMessage({
        role: 'assistant',
        content: `🔧 正在调用技能 \`/${skillName}\`...`,
        timestamp: new Date().toISOString()
      })

      // 调用 skill
      response = await invoke<string>('invoke_skill', {
        name: skillName.toLowerCase(),
        args: args || null,
        history: historyForModel.length > 0 ? historyForModel : null
      })

      // 移除"正在调用"的临时消息，替换为实际结果
      chatStore.messages.pop()
    } else {
      // 普通对话
      response = await invoke<string>('chat_with_assistant', {
        message: userMessage,
        history: historyForModel.length > 0 ? historyForModel : null
      })
    }

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

async function loadAlertHistory() {
  if (isHistoryLoading.value) return
  isHistoryLoading.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const since = startOfTodayTimestamp()
    const alerts = await invoke<Array<{
      timestamp: string
      issue_type: string
      message: string
      suggestion?: string
    }>>('get_recent_alerts', { since })

    if (!alerts || alerts.length === 0) {
      message.info('今天没有历史提醒')
      return
    }

    for (const alert of alerts) {
      const alertType = alert.issue_type || 'unknown'
      let content = `⚠️ **检测到问题**\n\n`
      content += `**类型**: ${alertType}\n`
      content += `**信息**: ${alert.message}\n`
      if (alert.suggestion) {
        content += `\n**建议**: ${alert.suggestion}`
      }

      chatStore.addAlert({
        role: 'assistant',
        content,
        timestamp: alert.timestamp,
        alertKey: `${alertType}|${alert.message}|${alert.timestamp}`,
      })
    }

    message.success(`已加载今天 ${alerts.length} 条提醒`)
  } catch (error) {
    message.error(`加载今天提醒失败: ${error}`)
  } finally {
    isHistoryLoading.value = false
  }
}

function startOfTodayTimestamp(): string {
  const now = new Date()
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 0, 0, 0)
  return formatLocalTimestamp(start)
}

function formatLocalTimestamp(date: Date): string {
  const pad = (value: number) => value.toString().padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
}


function newConversation() {
  if (chatStore.messages.length > 0) {
    const confirmed = window.confirm('确定新建对话吗？当前对话将被清空。')
    if (!confirmed) return
  }
  chatStore.newConversation()
  message.success('已新建对话')
}

function saveConversation() {
  const result = chatStore.saveCurrentConversation()
  if (result) {
    message.success(`对话已保存: ${result.title}`)
  } else {
    message.warning('没有可保存的对话内容')
  }
}

function loadSavedConversation(id: string) {
  if (chatStore.loadConversation(id)) {
    message.success('对话已加载')
  }
}

const savedConversationOptions = computed(() => {
  return chatStore.savedConversations.map(conv => ({
    label: conv.title,
    key: conv.id,
  }))
})

function clearChat() {
  const confirmed = window.confirm('确定清空当前对话吗？')
  if (!confirmed) return
  chatStore.clearMessages()
}

function scrollToBottom() {
  if (messagesContainer.value) {
    messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
  }
}

function handleKeydown(e: KeyboardEvent) {
  // 如果 Skill 提示列表显示中，处理上下键和回车
  if (showSkillHints.value && filteredSkills.value.length > 0) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      selectedSkillIndex.value = (selectedSkillIndex.value + 1) % filteredSkills.value.length
      return
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      selectedSkillIndex.value = (selectedSkillIndex.value - 1 + filteredSkills.value.length) % filteredSkills.value.length
      return
    }
    if (e.key === 'Tab' || (e.key === 'Enter' && !e.shiftKey)) {
      e.preventDefault()
      selectSkill(filteredSkills.value[selectedSkillIndex.value].name)
      return
    }
    if (e.key === 'Escape') {
      e.preventDefault()
      showSkillHints.value = false
      return
    }
  }

  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    sendMessage()
  }
}

function selectSkill(skillName: string) {
  inputMessage.value = `/${skillName} `
  showSkillHints.value = false
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

onMounted(async () => {
  scrollToBottom()
  captureStore.startStatusPolling()
  // 加载 Skills 列表
  await skillsStore.loadSkills()
})

onUnmounted(() => {
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
          <NSpace align="center">
            <NButton size="small" secondary @click="newConversation">
              <template #icon>
                <NIcon><AddOutline /></NIcon>
              </template>
              新建
            </NButton>
            <NButton size="small" secondary @click="saveConversation">
              <template #icon>
                <NIcon><SaveOutline /></NIcon>
              </template>
              保存
            </NButton>
            <NDropdown
              v-if="savedConversationOptions.length > 0"
              :options="savedConversationOptions"
              @select="loadSavedConversation"
            >
              <NButton size="small" secondary>
                历史对话 ({{ savedConversationOptions.length }})
              </NButton>
            </NDropdown>
            <NButton size="small" secondary :loading="isHistoryLoading" @click="loadAlertHistory">
              加载今天提醒
            </NButton>
            <NButton size="small" secondary @click="clearChat">清空</NButton>
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
      <div class="input-area-wrapper">
        <!-- Skill 提示列表 -->
        <div v-if="showSkillHints && filteredSkills.length > 0" class="skill-hints">
          <div
            v-for="(skill, index) in filteredSkills"
            :key="skill.name"
            class="skill-hint-item"
            :class="{ selected: index === selectedSkillIndex }"
            @click="selectSkill(skill.name)"
            @mouseenter="selectedSkillIndex = index"
          >
            <span class="skill-name">/{{ skill.name }}</span>
            <span class="skill-desc">{{ skill.description }}</span>
          </div>
        </div>
        <div v-else-if="showSkillHints && filteredSkills.length === 0" class="skill-hints">
          <div class="skill-hint-empty">没有匹配的技能</div>
        </div>

        <div class="input-area">
          <NInput
            v-model:value="inputMessage"
            type="textarea"
            placeholder="输入你的问题... (输入 / 查看可用技能)"
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
  position: sticky;
  top: 0;
  z-index: 10;
  background: #0f0f10;
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

.input-area-wrapper {
  position: relative;
}

.skill-hints {
  position: absolute;
  bottom: 100%;
  left: 0;
  right: 60px;
  background: #1a1a1c;
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 8px;
  margin-bottom: 8px;
  max-height: 200px;
  overflow-y: auto;
  box-shadow: 0 -4px 12px rgba(0, 0, 0, 0.3);
}

.skill-hint-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  cursor: pointer;
  transition: background 0.15s;
}

.skill-hint-item:hover,
.skill-hint-item.selected {
  background: rgba(99, 226, 183, 0.1);
}

.skill-name {
  color: #63e2b7;
  font-weight: 500;
  font-family: monospace;
  white-space: nowrap;
}

.skill-desc {
  color: rgba(255, 255, 255, 0.6);
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.skill-hint-empty {
  padding: 12px 14px;
  color: rgba(255, 255, 255, 0.4);
  font-size: 13px;
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
