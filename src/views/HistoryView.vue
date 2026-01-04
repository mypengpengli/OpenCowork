<script setup lang="ts">
import { ref, onMounted } from 'vue'
import {
  NLayout, NLayoutContent, NTimeline, NTimelineItem,
  NCard, NEmpty, NDatePicker, NSpace, NButton, NTag
} from 'naive-ui'

interface SummaryRecord {
  timestamp: string
  summary: string
  app: string
  action: string
  keywords: string[]
}

const records = ref<SummaryRecord[]>([])
const selectedDate = ref<number>(Date.now())
const isLoading = ref(false)

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
  }
  return typeMap[action] || 'info'
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
              <NSpace>
                <NTag size="small" type="info">{{ record.app }}</NTag>
                <NTag
                  v-for="keyword in record.keywords"
                  :key="keyword"
                  size="small"
                >
                  {{ keyword }}
                </NTag>
              </NSpace>
            </NCard>
          </NTimelineItem>
        </NTimeline>
      </div>
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
</style>
