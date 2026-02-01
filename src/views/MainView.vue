<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue'
import {
  NLayout,
  NLayoutContent,
  NInput,
  NButton,
  NSpace,
  NSpin,
  NTag,
  NIcon,
  NModal,
  NRadioGroup,
  NRadio,
  useMessage,
} from 'naive-ui'
import { Send, PlayCircleOutline, StopCircleOutline, AttachOutline, CloseOutline, DocumentOutline } from '@vicons/ionicons5'
import { open } from '@tauri-apps/plugin-dialog'
import { useChatStore, type ChatAttachment, type AttachmentKind, type ToolStep } from '../stores/chat'
import { useCaptureStore } from '../stores/capture'
import { useSkillsStore } from '../stores/skills'
import MessageItem from '../components/Chat/MessageItem.vue'
import { useI18n } from '../i18n'

const chatStore = useChatStore()
const captureStore = useCaptureStore()
const skillsStore = useSkillsStore()
const message = useMessage()
const { t } = useI18n()

const inputMessage = ref('')
const messagesContainer = ref<HTMLElement | null>(null)
const isLoading = ref(false)
const isHistoryLoading = ref(false)
const attachments = ref<ChatAttachment[]>([])
let attachmentSeq = 0
const toolModeModalVisible = ref(false)
const toolModeSelection = ref<'whitelist' | 'allow_all'>('whitelist')
const pendingRequest = ref<PendingRequest | null>(null)

const showProcessPanel = ref(true)
const processVisible = ref(false)
const processExpanded = ref(true)
const processStatus = ref<'idle' | 'running' | 'done' | 'error'>('idle')
const processItems = ref<ProgressItem[]>([])
const activeRequestId = ref<string | null>(null)
let progressUnlisten: (() => void) | null = null

const MAX_ATTACHMENTS = 6
const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'])
const TOOL_MODE_UNSET_ERROR = 'TOOLS_MODE_UNSET'
const REQUEST_CANCELLED_ERROR = 'REQUEST_CANCELLED'
const cancelledRequestIds = new Set<string>()
const CLIPBOARD_IMAGE_EXT: Record<string, string> = {
  'image/png': 'png',
  'image/jpeg': 'jpg',
  'image/gif': 'gif',
  'image/webp': 'webp',
  'image/bmp': 'bmp',
}


interface PendingRequest {
  message: string
  history: { role: string; content: string }[]
  attachments: ChatAttachment[]
  isSkill: boolean
  skillName?: string
  skillArgs?: string | null
  requestId: string
}

interface ProgressEventPayload {
  request_id: string
  stage: 'start' | 'step' | 'done' | 'error' | 'info'
  message: string
  detail?: string | null
  timestamp: string
}

interface ProgressItem {
  id: string
  stage: 'start' | 'step' | 'done' | 'error' | 'info'
  message: string
  detail?: string
  timestamp: string
}

// Skill 鎻愮ず鐩稿叧
const showSkillHints = ref(false)
const skillFilterText = ref('')
const selectedSkillIndex = ref(0)

// 杩囨护鍚庣殑 Skills 鍒楄〃
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

function startProcessPanel() {
  processItems.value = []
  processStatus.value = 'running'
  processExpanded.value = true
  processVisible.value = true
}

function appendProcessItem(payload: ProgressEventPayload) {
  if (!showProcessPanel.value) return
  if (cancelledRequestIds.has(payload.request_id)) return
  if (!activeRequestId.value || payload.request_id !== activeRequestId.value) return
  const item: ProgressItem = {
    id: `${payload.timestamp}-${processItems.value.length}`,
    stage: payload.stage,
    message: payload.message,
    detail: payload.detail || undefined,
    timestamp: payload.timestamp,
  }
  processItems.value.push(item)
  if (processItems.value.length > 30) {
    processItems.value.shift()
  }
  processVisible.value = true
}

function finishProcessPanel(status: 'done' | 'error') {
  if (!showProcessPanel.value) return
  if (!processVisible.value) return
  processStatus.value = status
  processExpanded.value = false
}

function toggleProcessExpanded() {
  processExpanded.value = !processExpanded.value
}

function truncateText(value: string, max = 80): string {
  const trimmed = value.trim()
  if (trimmed.length <= max) return trimmed
  return trimmed.slice(0, max).trimEnd() + '...'
}

function buildCancelledSummary(): string {
  const steps = processItems.value.filter(item => item.stage === 'step')
  const recent = steps.slice(-5)
  const lines = recent.map(item => {
    const detail = item.detail ? ` (${truncateText(item.detail, 60)})` : ''
    return `- ${item.message}${detail}`
  })
  const summary = lines.length > 0 ? lines.join('\n') : t('main.chat.cancelledNoSteps')
  return `${t('main.chat.cancelledSummaryTitle')}\n${summary}\n\n${t('main.chat.cancelledResumeHint')}`
}

function collectToolSteps(): ToolStep[] {
  if (!showProcessPanel.value) return []
  return processItems.value
    .filter(item => item.stage === 'step')
    .map(item => ({ title: item.message, detail: item.detail || undefined }))
}


async function loadProcessSetting() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = await invoke<any>('get_config')
    showProcessPanel.value = config?.ui?.show_progress ?? true
  } catch {
    showProcessPanel.value = true
  }
}

function pathBasename(filePath: string): string {
  const normalized = filePath.replace(/\\/g, '/')
  const parts = normalized.split('/')
  return parts[parts.length - 1] || filePath
}

function attachmentKindFromPath(filePath: string): AttachmentKind {
  const ext = filePath.split('.').pop()?.toLowerCase() || ''
  return IMAGE_EXTENSIONS.has(ext) ? 'image' : 'document'
}

async function addAttachments() {
  try {
    const selection = await open({
      multiple: true,
      filters: [
        { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'] },
        { name: 'Documents', extensions: ['txt', 'md', 'json', 'csv', 'log', 'yaml', 'yml', 'pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx'] },
      ],
    })
    if (!selection) return

    const paths = Array.isArray(selection) ? selection : [selection]
    const existing = new Set(attachments.value.map(item => item.path))
    const next: ChatAttachment[] = []

    for (const filePath of paths) {
      if (existing.has(filePath)) continue
      if (attachments.value.length + next.length >= MAX_ATTACHMENTS) {
        message.warning(t('main.attachments.limit'))
        break
      }

      const name = pathBasename(filePath)
      const kind = attachmentKindFromPath(filePath)
      next.push({
        id: `att_${Date.now()}_${attachmentSeq++}`,
        name,
        path: filePath,
        kind,
      })
    }

    if (next.length > 0) {
      attachments.value = attachments.value.concat(next)
    }
  } catch (error) {
    message.error(String(error))
  }
}


function readFileAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const result = typeof reader.result === 'string' ? reader.result : ''
      const base64 = result.split(',')[1] || ''
      if (!base64) {
        reject(new Error('empty base64'))
        return
      }
      resolve(base64)
    }
    reader.onerror = () => reject(reader.error || new Error('read failed'))
    reader.readAsDataURL(file)
  })
}

function buildClipboardImageName(file: File): string {
  if (file.name) return file.name
  const ext = CLIPBOARD_IMAGE_EXT[file.type] || 'png'
  return `clipboard-${Date.now()}.${ext}`
}

async function handlePaste(event: ClipboardEvent) {
  const items = event.clipboardData?.items
  if (!items || items.length === 0) return

  const imageItems = Array.from(items).filter(
    item => item.kind === 'file' && item.type.startsWith('image/')
  )
  if (imageItems.length === 0) return

  event.preventDefault()
  for (const item of imageItems) {
    if (attachments.value.length >= MAX_ATTACHMENTS) {
      message.warning(t('main.attachments.limit'))
      break
    }
    const file = item.getAsFile()
    if (!file) continue

    try {
      const base64 = await readFileAsBase64(file)
      const { invoke } = await import('@tauri-apps/api/core')
      const name = buildClipboardImageName(file)
      const savedPath = await invoke<string>('save_clipboard_image', {
        base64,
        name,
      })
      attachments.value = attachments.value.concat([
        {
          id: `att_${Date.now()}_${attachmentSeq++}`,
          name,
          path: savedPath,
          kind: 'image',
        },
      ])
    } catch (error) {
      message.error(t('main.attachments.pasteFailed', { error: String(error) }))
    }
  }
}

function removeAttachment(id: string) {
  attachments.value = attachments.value.filter(item => item.id !== id)
}

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

async function executeRequest(payload: PendingRequest, includeUserMessage: boolean) {
  if (isLoading.value) return

  activeRequestId.value = payload.requestId
  await loadProcessSetting()
  processItems.value = []
  if (showProcessPanel.value) {
    startProcessPanel()
  }

  if (includeUserMessage) {
    chatStore.addMessage({
      role: 'user',
      content: payload.message,
      timestamp: new Date().toISOString(),
      attachments: payload.attachments.length > 0 ? payload.attachments : undefined,
    })
  }

  isLoading.value = true
  let placeholderAdded = false
  let wasCancelled = false

  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const attachmentsPayload = payload.attachments.map(item => ({
      path: item.path,
      name: item.name,
      kind: item.kind,
    }))

    let response: string
    if (payload.isSkill) {
      const skillName = payload.skillName || ''
      chatStore.addMessage({
        role: 'assistant',
        content: t('main.chat.invokingSkill', { skill: skillName }),
        timestamp: new Date().toISOString(),
      })
      placeholderAdded = true
      appendProcessItem({
        request_id: payload.requestId,
        stage: 'step',
        message: '\u8c03\u7528\u6280\u80fd',
        detail: skillName ? '/' + skillName : undefined,
        timestamp: new Date().toISOString(),
      })

      response = await invoke<string>('invoke_skill', {
        name: skillName.toLowerCase(),
        args: payload.skillArgs || null,
        history: payload.history.length > 0 ? payload.history : null,
        attachments: attachmentsPayload.length > 0 ? attachmentsPayload : null,
        request_id: payload.requestId,
      })
      chatStore.messages.pop()
      placeholderAdded = false
    } else {
      response = await invoke<string>('chat_with_assistant', {
        message: payload.message,
        history: payload.history.length > 0 ? payload.history : null,
        attachments: attachmentsPayload.length > 0 ? attachmentsPayload : null,
        request_id: payload.requestId,
      })
    }

    if (cancelledRequestIds.has(payload.requestId)) {
      cancelledRequestIds.delete(payload.requestId)
      wasCancelled = true
      return
    }

    const toolStepsSnapshot = collectToolSteps()

    chatStore.addMessage({
      role: 'assistant',
      content: response,
      timestamp: new Date().toISOString(),
      toolSteps: toolStepsSnapshot.length > 0 ? toolStepsSnapshot : undefined,
    })
  } catch (error) {
    const errorText = String(error)
    if (errorText.includes(REQUEST_CANCELLED_ERROR) || cancelledRequestIds.has(payload.requestId)) {
      cancelledRequestIds.delete(payload.requestId)
      wasCancelled = true
      return
    }
    if (errorText.includes(TOOL_MODE_UNSET_ERROR)) {
      if (placeholderAdded) {
        chatStore.messages.pop()
      }
      finishProcessPanel('error')
      pendingRequest.value = payload
      toolModeModalVisible.value = true
      return
    }

    chatStore.addMessage({
      role: 'assistant',
      content: t('main.chat.error', { error: errorText }),
      timestamp: new Date().toISOString(),
    })
  } finally {
    isLoading.value = false
    activeRequestId.value = null
    if (showProcessPanel.value && !wasCancelled) {
      finishProcessPanel('done')
    }
    await nextTick()
    scrollToBottom()
  }
}

async function stopRequest() {
  if (!isLoading.value) return
  const requestId = activeRequestId.value
  if (!requestId) return

  cancelledRequestIds.add(requestId)
  isLoading.value = false
  activeRequestId.value = null
  if (showProcessPanel.value) {
    finishProcessPanel('error')
  }
  pendingRequest.value = null
  toolModeModalVisible.value = false

  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('cancel_request', { request_id: requestId })
  } catch (error) {
    console.error('Failed to cancel request:', error)
  }

  message.info(t('main.chat.cancelled'))
  chatStore.addMessage({
    role: 'assistant',
    content: buildCancelledSummary(),
    timestamp: new Date().toISOString(),
  })
}

async function applyToolModeSelection() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = await invoke<any>('get_config')
    const updatedConfig = {
      ...config,
      tools: {
        ...(config?.tools || {}),
        mode: toolModeSelection.value,
      },
    }
    await invoke('save_config', { config: updatedConfig })
    toolModeModalVisible.value = false
    const payload = pendingRequest.value
    pendingRequest.value = null
    if (payload) {
      await executeRequest(payload, false)
    }
  } catch (error) {
    message.error(String(error))
  }
}

function cancelToolModeSelection() {
  toolModeModalVisible.value = false
  pendingRequest.value = null
}

async function sendMessage() {
  if (isLoading.value) return

  const userMessage = inputMessage.value.trim()
  const hasAttachments = attachments.value.length > 0
  if (!userMessage && !hasAttachments) return

  const messageAttachments = attachments.value.map(item => ({ ...item }))
  const historyForModel = chatStore.chatHistoryForModel
    .map(m => ({ role: m.role, content: m.content }))

  inputMessage.value = ''
  attachments.value = []

  const skillMatch = userMessage.match(/^\/([a-z0-9-]+)(?:\s+(.*))?$/i)
  const requestId = `req_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
  const payload: PendingRequest = {
    message: userMessage,
    history: historyForModel,
    attachments: messageAttachments,
    isSkill: Boolean(skillMatch),
    skillName: skillMatch?.[1],
    skillArgs: skillMatch?.[2] || null,
    requestId,
  }

  await executeRequest(payload, true)
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
      message.info(t('main.alert.noneToday'))
      return
    }

    for (const alert of alerts) {
      const alertType = alert.issue_type || 'unknown'
      const content = formatAlertContent(alertType, alert.message, alert.suggestion)

      chatStore.addAlert({
        role: 'assistant',
        content,
        timestamp: alert.timestamp,
        alertKey: `${alertType}|${alert.message}|${alert.timestamp}`,
      })
    }

    message.success(t('main.alert.loaded', { count: alerts.length }))
  } catch (error) {
    message.error(t('main.alert.loadFailed', { error: String(error) }))
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


function clearChat() {
  const confirmed = window.confirm(t('main.chat.clearConfirm'))
  if (!confirmed) return
  chatStore.clearMessages()
}

function scrollToBottom() {
  if (messagesContainer.value) {
    messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
  }
}

function handleKeydown(e: KeyboardEvent) {
  // 濡傛灉 Skill 鎻愮ず鍒楄〃鏄剧ず涓紝澶勭悊涓婁笅閿拰鍥炶溅
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

function formatAlertContent(alertTypeRaw: string, messageText: string, suggestion?: string) {
  const alertTypeLabel = alertTypeRaw && alertTypeRaw !== 'unknown' ? alertTypeRaw : t('common.unknown')
  let content = `${t('alert.detectedTitle')}\n\n`
  content += `${t('alert.typeLine', { type: alertTypeLabel })}\n`
  content += `${t('alert.messageLine', { message: messageText })}\n`
  if (suggestion) {
    content += `\n${t('alert.suggestionLine', { suggestion })}`
  }
  return content
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
    console.error('鍒囨崲鐩戞帶鐘舵�佸け锟?', error)
  }
}

onMounted(async () => {
  scrollToBottom()
  captureStore.startStatusPolling()
  try {
    const { listen } = await import('@tauri-apps/api/event')
    progressUnlisten = await listen<ProgressEventPayload>('assistant-progress', (event) => {
      const payload = event.payload
      if (!payload || !payload.request_id) return
      if (!activeRequestId.value || payload.request_id !== activeRequestId.value) return
      appendProcessItem(payload)
      if (payload.stage === 'done') {
        finishProcessPanel('done')
      } else if (payload.stage === 'error') {
        finishProcessPanel('error')
      }
    })
  } catch (error) {
    console.error('Failed to listen progress events:', error)
  }
  // 鍔犺浇 Skills 鍒楄〃
  await skillsStore.loadSkills()
  await loadProcessSetting()
})

onUnmounted(() => {
  captureStore.stopStatusPolling()
  if (progressUnlisten) {
    progressUnlisten()
    progressUnlisten = null
  }
})
</script>

<template>
  <NLayout class="main-layout">
    <NLayoutContent class="main-content">
      <!-- 鐘舵�佹爮 -->
      <div class="status-bar">
        <NSpace justify="space-between" align="center" style="width: 100%">
          <NSpace>
            <NTag :type="captureStore.isCapturing ? 'success' : 'default'" size="small">
              {{ captureStore.isCapturing ? t('main.status.capturing') : t('main.status.paused') }}
            </NTag>
            <NTag type="info" size="small">
              {{ t('main.status.records') }}: {{ captureStore.recordCount }}
            </NTag>
          </NSpace>
          <NSpace align="center">
            <NButton size="small" secondary :loading="isHistoryLoading" @click="loadAlertHistory">
              {{ t('main.buttons.loadAlerts') }}
            </NButton>
            <NButton size="small" secondary @click="clearChat">{{ t('common.clear') }}</NButton>
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
              {{ captureStore.isCapturing ? t('common.stop') : t('common.start') }}
            </NButton>
          </NSpace>
        </NSpace>
      </div>

      <!-- 娑堟伅鍒楄〃 -->
      <div class="messages-container" ref="messagesContainer">
        <div v-if="chatStore.messages.length === 0" class="empty-state">
          <h2>{{ t('app.name') }}</h2>
          <p>{{ t('main.empty.desc') }}</p>
          <ul>
            <li>{{ t('main.empty.item1') }}</li>
            <li>{{ t('main.empty.item2') }}</li>
            <li>{{ t('main.empty.item3') }}</li>
          </ul>
          <p style="margin-top: 20px; color: #63e2b7;">
            {{ t('main.empty.tip') }}
          </p>
        </div>

        <MessageItem
          v-for="(msg, index) in chatStore.messages"
          :key="index"
          :message="msg"
        />

        <div v-if="isLoading" class="loading-indicator">
          <NSpin size="small" />
          <span>{{ t('main.loading') }}</span>
        </div>
      </div>

      <!-- 杈撳叆鍖哄煙 -->
      <div class="input-area-wrapper">
        <div v-if="showProcessPanel && processVisible" class="process-panel">
          <div class="process-header" @click="toggleProcessExpanded">
            <div class="process-title">
              <span>{{ t('main.progress.title') }}</span>
              <span class="process-status" :class="processStatus">
                {{
                  processStatus === 'running'
                    ? t('main.progress.running')
                    : processStatus === 'error'
                      ? t('main.progress.error')
                      : t('main.progress.done')
                }}
              </span>
            </div>
            <button type="button" class="process-toggle">
              {{ processExpanded ? t('main.progress.collapse') : t('main.progress.expand') }}
            </button>
          </div>
          <div v-if="processExpanded" class="process-body">
            <div v-if="processItems.length === 0" class="process-empty">
              {{ t('main.progress.empty') }}
            </div>
            <div v-else class="process-list">
              <div v-for="item in processItems" :key="item.id" class="process-item">
                <span class="process-message">{{ item.message }}</span>
                <span v-if="item.detail" class="process-detail">{{ item.detail }}</span>
              </div>
            </div>
          </div>
        </div>

        <!-- Skill 鎻愮ず鍒楄〃 -->
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
          <div class="skill-hint-empty">{{ t('main.skill.empty') }}</div>
        </div>

        <div v-if="attachments.length > 0" class="attachments-bar">
          <div v-for="attachment in attachments" :key="attachment.id" class="attachment-chip">
            <NIcon size="16" class="attachment-icon">
              <DocumentOutline />
            </NIcon>
            <span class="attachment-name">{{ attachment.name }}</span>
            <button
              type="button"
              class="attachment-remove"
              :title="t('main.attachments.remove')"
              @click="removeAttachment(attachment.id)"
            >
              <NIcon size="12"><CloseOutline /></NIcon>
            </button>
          </div>
        </div>

        <div class="input-area">
          <NButton secondary @click="addAttachments" :title="t('main.attachments.add')">
            <template #icon>
              <NIcon><AttachOutline /></NIcon>
            </template>
          </NButton>
          <NInput
            v-model:value="inputMessage"
            type="textarea"
            :placeholder="t('main.input.placeholder')"
            :autosize="{ minRows: 1, maxRows: 4 }"
            @keydown="handleKeydown"
            @paste="handlePaste"
          />
          <NButton
            v-if="isLoading"
            type="error"
            :title="t('common.stop')"
            @click="stopRequest"
          >
            <template #icon>
              <NIcon><StopCircleOutline /></NIcon>
            </template>
          </NButton>
          <NButton
            v-else
            type="primary"
            :disabled="(!inputMessage.trim() && attachments.length === 0)"
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

  <NModal v-model:show="toolModeModalVisible" preset="card" :title="t('main.tools.mode.title')" style="width: 520px;">
    <p>{{ t('main.tools.mode.desc') }}</p>
    <NRadioGroup v-model:value="toolModeSelection" style="margin-top: 12px;">
      <NSpace vertical>
        <NRadio value="whitelist">{{ t('main.tools.mode.whitelist') }}</NRadio>
        <NRadio value="allow_all">{{ t('main.tools.mode.allowAll') }}</NRadio>
      </NSpace>
    </NRadioGroup>
    <p class="tool-mode-hint">{{ t('main.tools.mode.hint') }}</p>
    <template #footer>
      <NSpace justify="end">
        <NButton @click="cancelToolModeSelection">{{ t('common.cancel') }}</NButton>
        <NButton type="primary" @click="applyToolModeSelection">{{ t('common.save') }}</NButton>
      </NSpace>
    </template>
  </NModal>
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

.process-panel {
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.03);
  padding: 8px 10px;
  margin-bottom: 10px;
  font-size: 12px;
}

.process-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
  gap: 12px;
}

.process-title {
  display: flex;
  align-items: center;
  gap: 8px;
  color: rgba(255, 255, 255, 0.75);
}

.process-status {
  font-size: 11px;
  padding: 2px 6px;
  border-radius: 999px;
  background: rgba(99, 226, 183, 0.12);
  color: #63e2b7;
}

.process-status.done {
  background: rgba(255, 255, 255, 0.08);
  color: rgba(255, 255, 255, 0.6);
}

.process-status.error {
  background: rgba(255, 107, 107, 0.16);
  color: rgba(255, 107, 107, 0.9);
}

.process-toggle {
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.5);
  font-size: 12px;
  cursor: pointer;
}

.process-toggle:hover {
  color: rgba(255, 255, 255, 0.8);
}

.process-body {
  margin-top: 8px;
}

.process-empty {
  color: rgba(255, 255, 255, 0.45);
}

.process-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 120px;
  overflow-y: auto;
}

.process-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  color: rgba(255, 255, 255, 0.7);
}

.process-detail {
  color: rgba(255, 255, 255, 0.45);
  font-size: 11px;
  word-break: break-word;
  white-space: pre-wrap;
}

.attachments-bar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 10px;
}

.tool-mode-hint {
  margin-top: 12px;
  color: rgba(255, 255, 255, 0.6);
  font-size: 12px;
}

.attachment-chip {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.1);
  max-width: 260px;
}

.attachment-thumb {
  width: 40px;
  height: 40px;
  object-fit: cover;
  border-radius: 6px;
}

.attachment-icon {
  color: rgba(255, 255, 255, 0.7);
}

.attachment-name {
  font-size: 12px;
  color: rgba(255, 255, 255, 0.8);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.attachment-remove {
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.5);
  cursor: pointer;
  padding: 0;
}

.attachment-remove:hover {
  color: rgba(255, 255, 255, 0.8);
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






