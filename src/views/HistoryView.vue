<script setup lang="ts">
import { ref, onMounted } from 'vue'
import {
  NLayout, NLayoutContent, NTimeline, NTimelineItem,
  NCard, NEmpty, NDatePicker, NSpace, NButton, NTag,
  NDrawer, NDrawerContent, NDescriptions, NDescriptionsItem, NEllipsis, NDivider,
  useMessage
} from 'naive-ui'
import { localeToDateLocale, useI18n } from '../i18n'

interface SummaryRecord {
  timestamp: string
  summary: string
  app: string
  action: string
  keywords: string[]
  has_issue?: boolean
  issue_type?: string
  issue_summary?: string
  suggestion?: string
  confidence?: number
  detail?: string
  detail_ref?: string
}

const records = ref<SummaryRecord[]>([])
const selectedDate = ref<number>(Date.now())
const isLoading = ref(false)
const isClearing = ref(false)
const isClearingAll = ref(false)
const drawerVisible = ref(false)
const selectedRecord = ref<SummaryRecord | null>(null)
const message = useMessage()
const { t, locale } = useI18n()

async function loadHistory() {
  isLoading.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const date = new Date(selectedDate.value)
    const dateStr = date.toISOString().split('T')[0]

    const data = await invoke<SummaryRecord[]>('get_summaries', { date: dateStr })
    records.value = data || []
  } catch (error) {
    console.error('加载历史记录失败:', error)
    records.value = []
  } finally {
    isLoading.value = false
  }
}

async function clearHistory() {
  const confirmed = window.confirm(t('history.clearConfirm'))
  if (!confirmed) return

  isClearing.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const date = new Date(selectedDate.value)
    const dateStr = date.toISOString().split('T')[0]
    const removed = await invoke<number>('clear_summaries', { date: dateStr })
    message.success(t('history.clearSuccess', { count: removed }))
    await loadHistory()
  } catch (error) {
    message.error(t('history.clearFailed', { error: String(error) }))
  } finally {
    isClearing.value = false
  }
}

async function clearAllHistory() {
  const confirmed = window.confirm(t('history.clearAllConfirm'))
  if (!confirmed) return

  isClearingAll.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const removed = await invoke<number>('clear_all_summaries')
    message.success(t('history.clearSuccess', { count: removed }))
    await loadHistory()
  } catch (error) {
    message.error(t('history.clearFailed', { error: String(error) }))
  } finally {
    isClearingAll.value = false
  }
}

async function openScreenshotsDir() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('open_screenshots_dir')
  } catch (error) {
    message.error(t('history.openScreenshotsFailed', { error: String(error) }))
  }
}

function formatTime(timestamp: string): string {
  const date = new Date(timestamp)
  return date.toLocaleTimeString(localeToDateLocale(locale.value), {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

function getActionType(action: string): 'success' | 'info' | 'warning' | 'error' {
  const typeMap: Record<string, 'success' | 'info' | 'warning' | 'error'> = {
    editing: 'info',
    browsing: 'success',
    typing: 'info',
    idle: 'warning',
    active: 'success',
    issue: 'error',
  }
  return typeMap[action] || 'info'
}

function hasIssue(record: SummaryRecord): boolean {
  return record.has_issue ?? record.action === 'issue'
}

function formatConfidence(confidence?: number): string {
  if (typeof confidence !== 'number') {
    return '—'
  }
  return `${Math.round(confidence * 100)}%`
}

function openDetail(record: SummaryRecord) {
  selectedRecord.value = record
  drawerVisible.value = true
}

function formatAppName(app: string): string {
  if (!app || app.toLowerCase() === 'unknown') {
    return t('common.unknown')
  }
  return app
}

onMounted(() => {
  loadHistory()
})
</script>

<template>
  <NLayout class="history-layout">
    <NLayoutContent class="history-content">
      <div class="history-header">
        <h2>{{ t('history.title') }}</h2>
        <NSpace>
          <NDatePicker
            v-model:value="selectedDate"
            type="date"
            @update:value="loadHistory"
          />
          <NButton @click="loadHistory" :loading="isLoading">{{ t('history.refresh') }}</NButton>
          <NButton secondary @click="openScreenshotsDir">{{ t('history.openScreenshots') }}</NButton>
          <NButton
            type="error"
            secondary
            @click="clearHistory"
            :loading="isClearing"
          >
            {{ t('history.clearDay') }}
          </NButton>
          <NButton
            type="error"
            @click="clearAllHistory"
            :loading="isClearingAll"
          >
            {{ t('history.clearAll') }}
          </NButton>
        </NSpace>
      </div>

      <div class="timeline-container">
        <NEmpty v-if="records.length === 0 && !isLoading" :description="t('history.empty')" />

        <NTimeline v-else>
          <NTimelineItem
            v-for="(record, index) in records"
            :key="index"
            :type="getActionType(record.action)"
            :title="record.summary"
            :time="formatTime(record.timestamp)"
          >
            <NCard size="small" :bordered="false">
              <NSpace vertical size="small">
                <NSpace align="center">
                  <NTag size="small" type="info">{{ formatAppName(record.app) }}</NTag>
                  <NTag size="small" :type="hasIssue(record) ? 'error' : 'success'">
                    {{ hasIssue(record) ? t('history.status.issue') : t('history.status.ok') }}
                  </NTag>
                  <NTag v-if="record.issue_type" size="small" type="warning">{{ record.issue_type }}</NTag>
                  <NTag size="small">{{ t('history.confidence', { value: formatConfidence(record.confidence) }) }}</NTag>
                  <NButton text size="tiny" @click="openDetail(record)">{{ t('history.detail') }}</NButton>
                </NSpace>

                <NDescriptions
                  v-if="record.issue_summary || record.suggestion"
                  size="small"
                  :column="1"
                  label-placement="left"
                >
                  <NDescriptionsItem v-if="record.issue_summary" :label="t('history.issueSummary')">
                    <NEllipsis :line-clamp="2">{{ record.issue_summary }}</NEllipsis>
                  </NDescriptionsItem>
                  <NDescriptionsItem v-if="record.suggestion" :label="t('history.suggestion')">
                    <NEllipsis :line-clamp="2">{{ record.suggestion }}</NEllipsis>
                  </NDescriptionsItem>
                </NDescriptions>

                <NSpace>
                  <NTag
                    v-for="keyword in record.keywords"
                    :key="keyword"
                    size="small"
                  >
                    {{ keyword }}
                  </NTag>
                </NSpace>
              </NSpace>
            </NCard>
          </NTimelineItem>
        </NTimeline>
      </div>

      <NDrawer v-model:show="drawerVisible" placement="right" width="520">
        <NDrawerContent :title="t('history.drawer.title')">
          <div v-if="selectedRecord" class="detail-content">
            <NDescriptions size="small" :column="1" label-placement="left">
              <NDescriptionsItem :label="t('history.drawer.time')">
                {{ formatTime(selectedRecord.timestamp) }}
              </NDescriptionsItem>
              <NDescriptionsItem :label="t('history.drawer.app')">
                {{ formatAppName(selectedRecord.app) }}
              </NDescriptionsItem>
              <NDescriptionsItem :label="t('history.drawer.status')">
                {{ hasIssue(selectedRecord) ? t('history.status.issue') : t('history.status.ok') }}
              </NDescriptionsItem>
              <NDescriptionsItem v-if="selectedRecord.issue_type" :label="t('history.drawer.issueType')">
                {{ selectedRecord.issue_type }}
              </NDescriptionsItem>
              <NDescriptionsItem :label="t('history.drawer.confidence')">
                {{ formatConfidence(selectedRecord.confidence) }}
              </NDescriptionsItem>
              <NDescriptionsItem v-if="selectedRecord.issue_summary" :label="t('history.drawer.issueSummary')">
                {{ selectedRecord.issue_summary }}
              </NDescriptionsItem>
              <NDescriptionsItem v-if="selectedRecord.suggestion" :label="t('history.drawer.suggestion')">
                {{ selectedRecord.suggestion }}
              </NDescriptionsItem>
              <NDescriptionsItem v-if="selectedRecord.detail_ref" :label="t('history.drawer.screenshot')">
                {{ selectedRecord.detail_ref }}
              </NDescriptionsItem>
            </NDescriptions>

            <NDivider />
            <div class="detail-label">{{ t('history.detailLabel') }}</div>
            <div class="detail-text">{{ selectedRecord.detail || t('history.detailEmpty') }}</div>
          </div>
        </NDrawerContent>
      </NDrawer>
    </NLayoutContent>
  </NLayout>
</template>

<style scoped>
.history-layout {
  height: 100%;
}

.history-content {
  padding: 24px;
  overflow-y: auto;
}

.history-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
}

.history-header h2 {
  color: #63e2b7;
  margin: 0;
}

.timeline-container {
  padding: 16px 0;
}

.detail-content {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.detail-label {
  color: #9aa4b2;
  font-size: 12px;
}

.detail-text {
  white-space: pre-wrap;
  word-break: break-word;
  line-height: 1.6;
}
</style>
