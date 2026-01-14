<script setup lang="ts">
import { computed } from 'vue'
import { NAvatar, NIcon } from 'naive-ui'
import { PersonOutline, HardwareChipOutline, WarningOutline, DocumentOutline } from '@vicons/ionicons5'
import { convertFileSrc } from '@tauri-apps/api/core'
import { localeToDateLocale, useI18n } from '../../i18n'
import type { ChatAttachment } from '../../stores/chat'

interface Message {
  role: 'user' | 'assistant'
  content: string
  timestamp: string
  isAlert?: boolean
  attachments?: ChatAttachment[]
}

const props = defineProps<{
  message: Message
}>()

const isUser = computed(() => props.message.role === 'user')
const isAlert = computed(() => props.message.isAlert)
const { t, locale } = useI18n()
const attachments = computed(() => props.message.attachments || [])

function attachmentPreview(attachment: ChatAttachment): string {
  if (attachment.kind !== 'image') {
    return ''
  }
  return convertFileSrc(attachment.path)
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
        {{ message.content }}
      </div>
      <div v-else-if="attachments.length > 0" class="message-text placeholder">
        {{ t('main.attachmentOnly') }}
      </div>
      <div v-if="attachments.length > 0" class="attachment-list">
        <div v-for="attachment in attachments" :key="attachment.id" class="attachment-item">
          <img
            v-if="attachment.kind === 'image'"
            :src="attachmentPreview(attachment)"
            :alt="attachment.name"
            class="attachment-image"
          />
          <div v-else class="attachment-doc">
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
  white-space: pre-wrap;
  word-break: break-word;
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
