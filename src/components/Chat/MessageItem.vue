<script setup lang="ts">
import { computed, ref, watch, onMounted, nextTick } from 'vue'
import { NAvatar, NIcon, NImage, NImageGroup, NTooltip } from 'naive-ui'
import { PersonOutline, HardwareChipOutline, WarningOutline, DocumentOutline, CopyOutline, RefreshOutline } from '@vicons/ionicons5'
import { localeToDateLocale, useI18n } from '../../i18n'
import type { ChatAttachment, ToolStep } from '../../stores/chat'
import { renderMarkdown } from '../../utils/markdown'

interface Message {
  role: 'user' | 'assistant'
  content: string
  timestamp: string
  isAlert?: boolean
  attachments?: ChatAttachment[]
  toolSteps?: ToolStep[]
}

const props = defineProps<{
  message: Message
}>()

const emit = defineEmits<{
  (e: 'regenerate', message: Message): void
}>()

const isUser = computed(() => props.message.role === 'user')
const isAlert = computed(() => props.message.isAlert)
const { t, locale } = useI18n()
const attachments = computed(() => props.message.attachments || [])
const toolSteps = computed(() => props.message.toolSteps || [])
const renderedHtml = computed(() => renderMarkdown(props.message.content))
const expanded = ref(false)
const expandedSteps = ref<Record<number, boolean>>({})
const showActions = ref(false)
const copySuccess = ref(false)

// 图片预览相关
const imageAttachments = computed(() => attachments.value.filter(a => a.kind === 'image'))
const docAttachments = computed(() => attachments.value.filter(a => a.kind !== 'image'))
const imageUrls = ref<Record<string, string>>({})

watch(
  () => props.message,
  () => {
    expanded.value = false
    expandedSteps.value = {}
  }
)

// 加载图片附件的预览
async function loadImagePreviews() {
  const images = imageAttachments.value
  if (images.length === 0) return

  try {
    const { invoke } = await import('@tauri-apps/api/core')
    for (const img of images) {
      if (imageUrls.value[img.id]) continue
      try {
        const base64 = await invoke<string>('read_image_base64', {
          filePath: img.path,
          fileType: 'attachment',
        })
        imageUrls.value[img.id] = base64
      } catch (e) {
        console.error('加载图片预览失败:', img.path, e)
      }
    }
  } catch (e) {
    console.error('加载图片预览失败:', e)
  }
}

watch(imageAttachments, () => {
  loadImagePreviews()
}, { immediate: true })

onMounted(() => {
  loadImagePreviews()
  // 为代码块添加复制按钮
  nextTick(() => {
    addCodeBlockCopyButtons()
  })
})

// 监听内容变化，重新添加代码块复制按钮
watch(renderedHtml, () => {
  nextTick(() => {
    addCodeBlockCopyButtons()
  })
})

// 复制消息内容
async function copyMessage() {
  try {
    await navigator.clipboard.writeText(props.message.content)
    copySuccess.value = true
    setTimeout(() => {
      copySuccess.value = false
    }, 2000)
  } catch (e) {
    console.error('复制失败:', e)
  }
}

// 重新生成
function regenerate() {
  emit('regenerate', props.message)
}

// 为代码块添加复制按钮
function addCodeBlockCopyButtons() {
  const messageEl = document.querySelector(`[data-message-id="${props.message.timestamp}"]`)
  if (!messageEl) return

  const codeBlocks = messageEl.querySelectorAll('pre')
  codeBlocks.forEach((pre) => {
    // 避免重复添加
    if (pre.querySelector('.code-copy-btn')) return

    const btn = document.createElement('button')
    btn.className = 'code-copy-btn'
    btn.innerHTML = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>`
    btn.title = t('message.copyCode')
    btn.onclick = async (e) => {
      e.stopPropagation()
      const code = pre.querySelector('code')?.textContent || pre.textContent || ''
      try {
        await navigator.clipboard.writeText(code)
        btn.innerHTML = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"></polyline></svg>`
        btn.classList.add('copied')
        setTimeout(() => {
          btn.innerHTML = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>`
          btn.classList.remove('copied')
        }, 2000)
      } catch (err) {
        console.error('复制代码失败:', err)
      }
    }
    pre.style.position = 'relative'
    pre.appendChild(btn)
  })
}

const MESSAGE_COLLAPSE_THRESHOLD = 900
const MESSAGE_LINE_THRESHOLD = 14
const TOOL_DETAIL_LIMIT = 180

const canCollapseMessage = computed(() => {
  const content = props.message.content
  if (!content) return false
  const longByLength = content.length > MESSAGE_COLLAPSE_THRESHOLD
  const longByLines = content.split('\n').length > MESSAGE_LINE_THRESHOLD
  return longByLength || longByLines
})

function toggleExpanded() {
  expanded.value = !expanded.value
}

function isToolDetailLong(detail?: string) {
  return Boolean(detail && detail.length > TOOL_DETAIL_LIMIT)
}

function toggleToolDetail(index: number) {
  expandedSteps.value = {
    ...expandedSteps.value,
    [index]: !expandedSteps.value[index],
  }
}

function toolDetailText(detail?: string, index?: number) {
  if (!detail) return ''
  if (index === undefined) return detail
  if (expandedSteps.value[index] || detail.length <= TOOL_DETAIL_LIMIT) return detail
  return `${detail.slice(0, TOOL_DETAIL_LIMIT).trimEnd()}...`
}

function formatTime(timestamp: string): string {
  const date = new Date(timestamp)
  return date.toLocaleString(localeToDateLocale(locale.value), {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}
</script>

<template>
  <div
    class="message-item"
    :class="{ 'user-message': isUser, 'alert-message': isAlert }"
    :data-message-id="message.timestamp"
    @mouseenter="showActions = true"
    @mouseleave="showActions = false"
  >
    <NAvatar
      :size="32"
      round
      :style="{
        backgroundColor: isAlert ? '#d03050' : (isUser ? '#18a058' : '#2080f0')
      }"
    >
      <NIcon :component="isAlert ? WarningOutline : (isUser ? PersonOutline : HardwareChipOutline)" />
    </NAvatar>
    <div class="message-content">
      <div class="message-header">
        <span class="role">
          {{ isAlert ? t('message.role.alert') : (isUser ? t('message.role.user') : t('message.role.assistant')) }}
        </span>
        <span class="time">{{ formatTime(message.timestamp) }}</span>

        <!-- 操作按钮 -->
        <div class="message-actions" :class="{ visible: showActions }">
          <NTooltip trigger="hover" placement="top">
            <template #trigger>
              <button type="button" class="action-btn" @click="copyMessage">
                <NIcon size="14"><CopyOutline /></NIcon>
              </button>
            </template>
            {{ copySuccess ? t('message.copied') : t('message.copy') }}
          </NTooltip>
          <NTooltip v-if="!isUser && !isAlert" trigger="hover" placement="top">
            <template #trigger>
              <button type="button" class="action-btn" @click="regenerate">
                <NIcon size="14"><RefreshOutline /></NIcon>
              </button>
            </template>
            {{ t('message.regenerate') }}
          </NTooltip>
        </div>
      </div>
      <div
        v-if="message.content.trim().length > 0"
        class="message-text"
        :class="{ 'alert-text': isAlert }"
      >
        <div
          class="markdown-body"
          :class="{ collapsed: canCollapseMessage && !expanded }"
          v-html="renderedHtml"
        ></div>
      </div>
      <button
        v-if="canCollapseMessage"
        type="button"
        class="message-toggle"
        @click="toggleExpanded"
      >
        {{ expanded ? t('main.chat.collapseContent') : t('main.chat.expandContent') }}
      </button>

      <div v-if="toolSteps.length > 0" class="tool-cards">
        <div v-for="(step, index) in toolSteps" :key="index" class="tool-card">
          <div class="tool-card-header">
            <div class="tool-card-title">
              <NIcon size="14">
                <HardwareChipOutline />
              </NIcon>
              <span>{{ step.title }}</span>
            </div>
            <button
              v-if="isToolDetailLong(step.detail)"
              type="button"
              class="tool-card-toggle"
              @click="toggleToolDetail(index)"
            >
              {{ expandedSteps[index] ? t('main.chat.collapseDetail') : t('main.chat.expandDetail') }}
            </button>
          </div>
          <div v-if="step.detail" class="tool-card-detail">
            {{ toolDetailText(step.detail, index) }}
          </div>
        </div>
      </div>
      <div v-else-if="attachments.length > 0 && !message.content.trim()" class="message-text placeholder">
        {{ t('main.attachmentOnly') }}
      </div>

      <!-- 图片附件预览 -->
      <div v-if="imageAttachments.length > 0" class="attachment-images">
        <NImageGroup>
          <div class="image-grid">
            <div v-for="img in imageAttachments" :key="img.id" class="image-item">
              <NImage
                v-if="imageUrls[img.id]"
                :src="imageUrls[img.id]"
                :alt="img.name"
                object-fit="cover"
                width="120"
                height="120"
                lazy
                preview-disabled
                :previewed-img-props="{ style: { maxWidth: '90vw', maxHeight: '90vh' } }"
              />
              <div v-else class="image-placeholder">
                <NIcon size="24"><DocumentOutline /></NIcon>
              </div>
              <span class="image-name">{{ img.name }}</span>
            </div>
          </div>
        </NImageGroup>
      </div>

      <!-- 文档附件列表 -->
      <div v-if="docAttachments.length > 0" class="attachment-list">
        <div v-for="attachment in docAttachments" :key="attachment.id" class="attachment-item">
          <div class="attachment-doc">
            <NIcon size="16">
              <DocumentOutline />
            </NIcon>
            <span class="attachment-name">{{ attachment.name }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.message-item {
  display: flex;
  gap: 12px;
  padding: 12px 0;
}

.message-item.user-message {
  flex-direction: row-reverse;
}

.message-item.user-message .message-content {
  align-items: flex-end;
}

.message-item.user-message .message-header {
  flex-direction: row-reverse;
}

.message-content {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-width: 70%;
}

.message-header {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: rgba(255, 255, 255, 0.5);
}

.role {
  font-weight: 500;
}

/* 操作按钮 */
.message-actions {
  display: flex;
  gap: 4px;
  margin-left: auto;
  opacity: 0;
  transition: opacity 0.2s;
}

.message-actions.visible {
  opacity: 1;
}

.user-message .message-actions {
  margin-left: 0;
  margin-right: auto;
}

.action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  background: rgba(255, 255, 255, 0.08);
  border-radius: 6px;
  color: rgba(255, 255, 255, 0.6);
  cursor: pointer;
  transition: background 0.2s, color 0.2s;
}

.action-btn:hover {
  background: rgba(255, 255, 255, 0.15);
  color: rgba(255, 255, 255, 0.9);
}

.message-text {
  background: rgba(255, 255, 255, 0.05);
  padding: 12px 16px;
  border-radius: 12px;
  line-height: 1.6;
  word-break: break-word;
}

.markdown-body {
  display: block;
  color: inherit;
}

.markdown-body.collapsed {
  max-height: 260px;
  overflow: hidden;
  position: relative;
}

.markdown-body.collapsed::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 64px;
  background: linear-gradient(to bottom, rgba(15, 15, 16, 0), rgba(15, 15, 16, 0.9));
  pointer-events: none;
}

.message-toggle {
  align-self: flex-start;
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.65);
  font-size: 12px;
  cursor: pointer;
  padding: 0;
}

.message-toggle:hover {
  color: rgba(255, 255, 255, 0.85);
}

.message-text.placeholder {
  color: rgba(255, 255, 255, 0.6);
}

.user-message .message-text {
  background: rgba(24, 160, 88, 0.2);
}

.alert-message .message-text,
.alert-text {
  background: rgba(208, 48, 80, 0.2);
  border-left: 3px solid #d03050;
}

.message-text :deep(p) {
  margin: 0 0 8px;
}

.message-text :deep(p:last-child) {
  margin-bottom: 0;
}

.message-text :deep(ul),
.message-text :deep(ol) {
  margin: 0 0 8px 18px;
}

.message-text :deep(li) {
  margin: 4px 0;
}

.message-text :deep(code) {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  font-size: 0.9em;
  background: rgba(255, 255, 255, 0.1);
  padding: 1px 4px;
  border-radius: 4px;
}

.message-text :deep(pre) {
  background: rgba(0, 0, 0, 0.35);
  padding: 12px;
  border-radius: 8px;
  overflow-x: auto;
  margin: 8px 0;
  position: relative;
}

.message-text :deep(pre code) {
  background: transparent;
  padding: 0;
}

/* 代码块复制按钮 */
.message-text :deep(.code-copy-btn) {
  position: absolute;
  top: 8px;
  right: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: rgba(255, 255, 255, 0.1);
  border-radius: 6px;
  color: rgba(255, 255, 255, 0.5);
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.2s, background 0.2s, color 0.2s;
}

.message-text :deep(pre:hover .code-copy-btn) {
  opacity: 1;
}

.message-text :deep(.code-copy-btn:hover) {
  background: rgba(255, 255, 255, 0.2);
  color: rgba(255, 255, 255, 0.9);
}

.message-text :deep(.code-copy-btn.copied) {
  color: #63e2b7;
}

.message-text :deep(blockquote) {
  margin: 6px 0;
  padding-left: 10px;
  border-left: 3px solid rgba(255, 255, 255, 0.15);
  color: rgba(255, 255, 255, 0.75);
}

.message-text :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin: 8px 0;
  font-size: 0.92em;
}

.message-text :deep(th),
.message-text :deep(td) {
  border: 1px solid rgba(255, 255, 255, 0.12);
  padding: 6px 8px;
  text-align: left;
}

.message-text :deep(th) {
  background: rgba(255, 255, 255, 0.06);
}

.message-text :deep(a) {
  color: #7bd3ff;
  text-decoration: underline;
}

.tool-cards {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 6px;
}

.tool-card {
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 10px;
  padding: 10px 12px;
  font-size: 12px;
  color: rgba(255, 255, 255, 0.85);
}

.tool-card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}

.tool-card-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-weight: 600;
}

.tool-card-detail {
  color: rgba(255, 255, 255, 0.7);
  white-space: pre-wrap;
  word-break: break-word;
}

.tool-card-toggle {
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.55);
  font-size: 11px;
  cursor: pointer;
  padding: 0;
}

.tool-card-toggle:hover {
  color: rgba(255, 255, 255, 0.8);
}

.attachment-list {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 8px;
}

.attachment-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.08);
  max-width: 220px;
}

.attachment-image {
  width: 72px;
  height: 72px;
  object-fit: cover;
  border-radius: 6px;
}

.attachment-doc {
  display: flex;
  align-items: center;
  gap: 6px;
  color: rgba(255, 255, 255, 0.75);
  font-size: 12px;
}

.attachment-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 图片附件网格 */
.attachment-images {
  margin-top: 8px;
}

.image-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.image-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 6px;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.08);
  transition: border-color 0.2s;
}

.image-item:hover {
  border-color: rgba(99, 226, 183, 0.4);
}

.image-item :deep(.n-image) {
  border-radius: 8px;
  overflow: hidden;
}

.image-item :deep(.n-image img) {
  border-radius: 8px;
  cursor: pointer;
}

.image-placeholder {
  width: 120px;
  height: 120px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(255, 255, 255, 0.06);
  border-radius: 8px;
  color: rgba(255, 255, 255, 0.4);
}

.image-name {
  font-size: 11px;
  color: rgba(255, 255, 255, 0.6);
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: center;
}
</style>
