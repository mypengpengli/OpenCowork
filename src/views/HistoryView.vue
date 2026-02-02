<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import {
  NLayout, NLayoutContent, NTimeline, NTimelineItem,
  NCard, NEmpty, NDatePicker, NSpace, NButton, NTag,
  NDrawer, NDrawerContent, NDescriptions, NDescriptionsItem, NEllipsis, NDivider,
  NImage, NSpin,
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

// 截图预览相关
const screenshotUrls = ref<Record<string, string>>({})
const drawerScreenshotUrl = ref<string>('')
const drawerScreenshotLoading = ref(false)

async function loadHistory() {
  isLoading.value = true
  screenshotUrls.value = {}
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const date = new Date(selectedDate.value)
    const dateStr = date.toISOString().split('T')[0]

    const data = await invoke<SummaryRecord[]>('get_summaries', { date: dateStr })
    records.value = data || []

    // 加载截图缩略图（只加载前20条）
    loadScreenshotThumbnails(records.value.slice(0, 20))
  } catch (error) {
    console.error('加载历史记录失败:', error)
    records.value = []
  } finally {
    isLoading.value = false
  }
}

async function loadScreenshotThumbnails(recordsToLoad: SummaryRecord[]) {
  const { invoke } = await import('@tauri-apps/api/core')
  for (const record of recordsToLoad) {
    if (!record.detail_ref) continue
    if (screenshotUrls.value[record.detail_ref]) continue
    try {
      const base64 = await invoke<string>('read_image_base64', {
        filePath: record.detail_ref,
        fileType: 'screenshot',
      })
      screenshotUrls.value[record.detail_ref] = base64
    } catch (e) {
      // 截图可能已被清理，忽略错误
    }
  }
}

async function loadDrawerScreenshot(detailRef: string) {
  if (!detailRef) {
    drawerScreenshotUrl.value = ''
    return
  }

  // 如果已经加载过，直接使用
  if (screenshotUrls.value[detailRef]) {
    drawerScreenshotUrl.value = screenshotUrls.value[detailRef]
    return
  }

  drawerScreenshotLoading.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const base64 = await invoke<string>('read_image_base64', {
      filePath: detailRef,
      fileType: 'screenshot',
    })
    drawerScreenshotUrl.value = base64
    screenshotUrls.value[detailRef] = base64
  } catch (e) {
    drawerScreenshotUrl.value = ''
  } finally {
    drawerScreenshotLoading.value = false
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
  drawerScreenshotUrl.value = ''
  if (record.detail_ref) {
    loadDrawerScreenshot(record.detail_ref)
  }
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
            <NCard size="small" :bordered="false" class="record-card">
              <div class="record-content">
                <!-- 截图缩略图 -->
                <div v-if="record.detail_ref && screenshotUrls[record.detail_ref]" class="record-thumbnail" @click="openDetail(record)">
                  <img :src="screenshotUrls[record.detail_ref]" :alt="record.summary" />
                </div>

                <NSpace vertical size="small" class="record-info">
                  <NSpace align="center" wrap>
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

                <NSpace wrap>
                  <NTag
                    v-for="keyword in record.keywords"
                    :key="keyword"
                    size="small"
                  >
                    {{ keyword }}
                  </NTag>
                </NSpace>
                </NSpace>
              </div>
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

            <!-- 截图预览 -->
            <div v-if="selectedRecord.detail_ref" class="drawer-screenshot">
              <div class="detail-label">{{ t('history.drawer.screenshotPreview') }}</div>
              <div v-if="drawerScreenshotLoading" class="screenshot-loading">
                <NSpin size="small" />
              </div>
              <NImage
                v-else-if="drawerScreenshotUrl"
                :src="drawerScreenshotUrl"
                :alt="selectedRecord.summary"
                object-fit="contain"
                width="100%"
                :img-props="{ style: { maxHeight: '400px', borderRadius: '8px' } }"
              />
              <div v-else class="screenshot-empty">
                {{ t('history.drawer.screenshotNotFound') }}
              </div>
            </div>

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
  flex-wrap: wrap;
  gap: 12px;
}

.history-header h2 {
  color: #63e2b7;
  margin: 0;
}

.timeline-container {
  padding: 16px 0;
}

/* 记录卡片样式 */
.record-card {
  background: rgba(255, 255, 255, 0.02);
}

.record-content {
  display: flex;
  gap: 16px;
}

.record-thumbnail {
  flex-shrink: 0;
  width: 100px;
  height: 70px;
  border-radius: 8px;
  overflow: hidden;
  cursor: pointer;
  border: 1px solid rgba(255, 255, 255, 0.1);
  transition: border-color 0.2s, transform 0.2s;
}

.record-thumbnail:hover {
  border-color: rgba(99, 226, 183, 0.5);
  transform: scale(1.02);
}

.record-thumbnail img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.record-info {
  flex: 1;
  min-width: 0;
}

/* 抽屉截图样式 */
.drawer-screenshot {
  margin-top: 8px;
}

.screenshot-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 200px;
  background: rgba(255, 255, 255, 0.03);
  border-radius: 8px;
}

.screenshot-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100px;
  background: rgba(255, 255, 255, 0.03);
  border-radius: 8px;
  color: rgba(255, 255, 255, 0.4);
  font-size: 13px;
}

.detail-content {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.detail-label {
  color: #9aa4b2;
  font-size: 12px;
  margin-bottom: 8px;
}

.detail-text {
  white-space: pre-wrap;
  word-break: break-word;
  line-height: 1.6;
}
</style>
