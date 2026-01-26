<template>
  <div class="notification-container" :class="urgencyClass" @click="handleClick">
    <div class="notification-header">
      <div class="notification-icon">
        <span v-if="helpType === 'error'">⚠️</span>
        <span v-else-if="helpType === 'reminder'">💡</span>
        <span v-else-if="helpType === 'suggestion'">✨</span>
        <span v-else>ℹ️</span>
      </div>
      <div class="notification-title">
        {{ intentLabel }}
      </div>
      <div class="notification-countdown">
        {{ countdown }}s
      </div>
      <button class="notification-close" @click.stop="handleClose">×</button>
    </div>
    <div class="notification-body">
      <div class="notification-summary">{{ summary }}</div>
      <div v-if="suggestion" class="notification-suggestion">
        {{ suggestion }}
      </div>
    </div>
    <div class="notification-footer">
      <span class="notification-scene">{{ sceneLabel }}</span>
      <span class="notification-hint">点击查看详情</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRoute } from 'vue-router'

const route = useRoute()

// 从 URL 参数获取数据
const intent = ref(decodeURIComponent((route.query.intent as string) || ''))
const scene = ref(decodeURIComponent((route.query.scene as string) || ''))
const helpType = ref(decodeURIComponent((route.query.help_type as string) || 'info'))
const summary = ref(decodeURIComponent((route.query.summary as string) || ''))
const suggestion = ref(decodeURIComponent((route.query.suggestion as string) || ''))
const urgency = ref(decodeURIComponent((route.query.urgency as string) || 'medium'))

// 倒计时
const countdown = ref(10)
let countdownTimer: ReturnType<typeof setInterval> | null = null

// 计算属性
const urgencyClass = computed(() => {
  return `urgency-${urgency.value}`
})

const intentLabel = computed(() => {
  const labels: Record<string, string> = {
    '安装软件': '安装提醒',
    '写作': '写作助手',
    '出行规划': '出行提醒',
    '代码开发': '开发助手',
    '浏览网页': '浏览提示',
    '文件管理': '文件操作',
    '通讯聊天': '通讯提醒',
    '学习研究': '学习助手',
  }
  return labels[intent.value] || intent.value || '智能提醒'
})

const sceneLabel = computed(() => {
  const labels: Record<string, string> = {
    'github-install': 'GitHub',
    'npm-install': 'NPM',
    'writing': '写作',
    'travel': '出行',
    'coding': '编程',
    'browsing': '浏览',
    'file-management': '文件',
    'communication': '通讯',
  }
  return labels[scene.value] || scene.value || ''
})

// 方法
async function handleClick() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('focus_main_window')
    await invoke('close_notification')
  } catch (error) {
    console.error('处理点击失败:', error)
  }
}

async function handleClose() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('close_notification')
  } catch (error) {
    console.error('关闭通知失败:', error)
  }
}

async function autoClose() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('close_notification')
  } catch (error) {
    console.error('自动关闭失败:', error)
  }
}

// 监听更新事件
async function setupUpdateListener() {
  try {
    const { listen } = await import('@tauri-apps/api/event')
    await listen<{
      intent: string
      scene: string
      help_type: string
      summary: string
      suggestion: string
      urgency: string
    }>('notification-update', (event) => {
      const data = event.payload
      intent.value = data.intent
      scene.value = data.scene
      helpType.value = data.help_type
      summary.value = data.summary
      suggestion.value = data.suggestion
      urgency.value = data.urgency
      // 重置倒计时
      countdown.value = 10
    })
  } catch (error) {
    console.error('设置更新监听失败:', error)
  }
}

onMounted(() => {
  setupUpdateListener()

  // 启动倒计时
  countdownTimer = setInterval(() => {
    countdown.value--
    if (countdown.value <= 0) {
      if (countdownTimer) {
        clearInterval(countdownTimer)
        countdownTimer = null
      }
      autoClose()
    }
  }, 1000)
})

onUnmounted(() => {
  if (countdownTimer) {
    clearInterval(countdownTimer)
    countdownTimer = null
  }
})
</script>

<style scoped>
.notification-container {
  width: 100%;
  height: 100%;
  background: rgba(30, 30, 30, 0.95);
  border-radius: 12px;
  padding: 12px 16px;
  box-sizing: border-box;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  color: #fff;
  border: 1px solid rgba(255, 255, 255, 0.1);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  transition: transform 0.2s, box-shadow 0.2s;
}

.notification-container:hover {
  transform: translateY(-2px);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
}

/* 紧急程度样式 */
.urgency-high {
  border-left: 4px solid #ff4d4f;
}

.urgency-medium {
  border-left: 4px solid #faad14;
}

.urgency-low {
  border-left: 4px solid #52c41a;
}

.notification-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.notification-icon {
  font-size: 18px;
  line-height: 1;
}

.notification-title {
  flex: 1;
  font-size: 14px;
  font-weight: 600;
  color: #fff;
}

.notification-countdown {
  font-size: 12px;
  color: rgba(255, 255, 255, 0.5);
  min-width: 24px;
  text-align: right;
}

.notification-close {
  background: none;
  border: none;
  color: rgba(255, 255, 255, 0.5);
  font-size: 18px;
  cursor: pointer;
  padding: 0 4px;
  line-height: 1;
  transition: color 0.2s;
}

.notification-close:hover {
  color: #fff;
}

.notification-body {
  flex: 1;
  overflow: hidden;
}

.notification-summary {
  font-size: 13px;
  color: rgba(255, 255, 255, 0.9);
  line-height: 1.4;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  text-overflow: ellipsis;
}

.notification-suggestion {
  font-size: 12px;
  color: rgba(255, 255, 255, 0.7);
  margin-top: 4px;
  display: -webkit-box;
  -webkit-line-clamp: 1;
  -webkit-box-orient: vertical;
  overflow: hidden;
  text-overflow: ellipsis;
}

.notification-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px solid rgba(255, 255, 255, 0.1);
}

.notification-scene {
  font-size: 11px;
  color: rgba(255, 255, 255, 0.5);
  background: rgba(255, 255, 255, 0.1);
  padding: 2px 8px;
  border-radius: 4px;
}

.notification-hint {
  font-size: 11px;
  color: rgba(255, 255, 255, 0.4);
}
</style>
