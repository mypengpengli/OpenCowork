<script setup lang="ts">
import { ref, onMounted } from 'vue'
import {
  NLayout, NLayoutContent, NTimeline, NTimelineItem,
  NCard, NEmpty, NDatePicker, NSpace, NButton, NTag,
  NDrawer, NDrawerContent, NDescriptions, NDescriptionsItem, NEllipsis, NDivider,
  useMessage
} from 'naive-ui'

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
  const confirmed = window.confirm('确定清空当前日期的历史记录吗？')
  if (!confirmed) return

  isClearing.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const date = new Date(selectedDate.value)
    const dateStr = date.toISOString().split('T')[0]
    const removed = await invoke<number>('clear_summaries', { date: dateStr })
    message.success(`已清空 ${removed} 条记录`)
    await loadHistory()
  } catch (error) {
    message.error(`清空失败: ${error}`)
  } finally {
    isClearing.value = false
  }
}

async function clearAllHistory() {
  const confirmed = window.confirm('确定清空所有历史记录吗？此操作不可恢复。')
  if (!confirmed) return

  isClearingAll.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const removed = await invoke<number>('clear_all_summaries')
    message.success(`已清空 ${removed} 条记录`)
    await loadHistory()
  } catch (error) {
    message.error(`清空失败: ${error}`)
  } finally {
    isClearingAll.value = false
  }
}

async function openScreenshotsDir() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('open_screenshots_dir')
  } catch (error) {
    message.error(`Open screenshots folder failed: ${error}`)
  }
}

function formatTime(timestamp: string): string {
  const date = new Date(timestamp)
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' })
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

onMounted(() => {
  loadHistory()
})
</script>

<template>
  <NLayout class="history-layout">
    <NLayoutContent class="history-content">
      <div class="history-header">
        <h2>历史记录</h2>
        <NSpace>
          <NDatePicker
            v-model:value="selectedDate"
            type="date"
            @update:value="loadHistory"
          />
          <NButton @click="loadHistory" :loading="isLoading">刷新</NButton>
          <NButton secondary @click="openScreenshotsDir">打开截图文件夹</NButton>
          <NButton
            type="error"
            secondary
            @click="clearHistory"
            :loading="isClearing"
          >
            清空当天
          </NButton>
          <NButton
            type="error"
            @click="clearAllHistory"
            :loading="isClearingAll"
          >
            清空全部
          </NButton>
        </NSpace>
      </div>

      <div class="timeline-container">
        <NEmpty v-if="records.length === 0 && !isLoading" description="暂无记录" />

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
                  <NTag size="small" type="info">{{ record.app || 'Unknown' }}</NTag>
                  <NTag size="small" :type="hasIssue(record) ? 'error' : 'success'">
                    {{ hasIssue(record) ? '有问题' : '正常' }}
                  </NTag>
                  <NTag v-if="record.issue_type" size="small" type="warning">{{ record.issue_type }}</NTag>
                  <NTag size="small">置信度 {{ formatConfidence(record.confidence) }}</NTag>
                  <NButton text size="tiny" @click="openDetail(record)">详情</NButton>
                </NSpace>

                <NDescriptions
                  v-if="record.issue_summary || record.suggestion"
                  size="small"
                  :column="1"
                  label-placement="left"
                >
                  <NDescriptionsItem v-if="record.issue_summary" label="问题摘要">
                    <NEllipsis :line-clamp="2">{{ record.issue_summary }}</NEllipsis>
                  </NDescriptionsItem>
                  <NDescriptionsItem v-if="record.suggestion" label="建议">
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
        <NDrawerContent title="详情">
          <div v-if="selectedRecord" class="detail-content">
            <NDescriptions size="small" :column="1" label-placement="left">
              <NDescriptionsItem label="时间">{{ formatTime(selectedRecord.timestamp) }}</NDescriptionsItem>
              <NDescriptionsItem label="应用">{{ selectedRecord.app || 'Unknown' }}</NDescriptionsItem>
              <NDescriptionsItem label="状态">{{ hasIssue(selectedRecord) ? '有问题' : '正常' }}</NDescriptionsItem>
              <NDescriptionsItem v-if="selectedRecord.issue_type" label="问题类型">
                {{ selectedRecord.issue_type }}
              </NDescriptionsItem>
              <NDescriptionsItem label="置信度">
                {{ formatConfidence(selectedRecord.confidence) }}
              </NDescriptionsItem>
              <NDescriptionsItem v-if="selectedRecord.issue_summary" label="问题摘要">
                {{ selectedRecord.issue_summary }}
              </NDescriptionsItem>
              <NDescriptionsItem v-if="selectedRecord.suggestion" label="建议">
                {{ selectedRecord.suggestion }}
              </NDescriptionsItem>
              <NDescriptionsItem v-if="selectedRecord.detail_ref" label="截图">
                {{ selectedRecord.detail_ref }}
              </NDescriptionsItem>
            </NDescriptions>

            <NDivider />
            <div class="detail-label">detail</div>
            <div class="detail-text">{{ selectedRecord.detail || '无 detail' }}</div>
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
