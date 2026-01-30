<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { NAvatar, NIcon } from 'naive-ui'
import { PersonOutline, HardwareChipOutline, WarningOutline, DocumentOutline } from '@vicons/ionicons5'
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

const isUser = computed(() => props.message.role === 'user')
const isAlert = computed(() => props.message.isAlert)
const { t, locale } = useI18n()
const attachments = computed(() => props.message.attachments || [])
const toolSteps = computed(() => props.message.toolSteps || [])
const renderedHtml = computed(() => renderMarkdown(props.message.content))
const expanded = ref(false)
const expandedSteps = ref<Record<number, boolean>>({})

watch(
  () => props.message,
  () => {
    expanded.value = false
    expandedSteps.value = {}
  }
)

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
  <div class="message-item" :class="{ 'user-message': isUser, 'alert-message': isAlert }">
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
      <div v-else-if="attachments.length > 0" class="message-text placeholder">
        {{ t('main.attachmentOnly') }}
      </div>
      <div v-if="attachments.length > 0" class="attachment-list">
        <div v-for="attachment in attachments" :key="attachment.id" class="attachment-item">
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
  gap: 8px;
  font-size: 12px;
  color: rgba(255, 255, 255, 0.5);
}

.role {
  font-weight: 500;
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
}

.message-text :deep(pre code) {
  background: transparent;
  padding: 0;
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
</style>
