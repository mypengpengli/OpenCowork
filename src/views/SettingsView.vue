<script setup lang="ts">
import { ref, onMounted } from 'vue'
import {
  NLayout, NLayoutContent, NCard, NForm, NFormItem,
  NInput, NInputNumber, NSelect, NButton, NSwitch,
  NDivider, NSpace, useMessage, NTooltip
} from 'naive-ui'
import { useSettingsStore } from '../stores/settings'

const message = useMessage()
const settingsStore = useSettingsStore()

const formValue = ref({
  // 模型配置
  provider: 'api',
  apiType: 'openai',
  apiEndpoint: 'https://api.openai.com/v1',
  apiKey: '',
  apiModel: 'gpt-4-vision-preview',
  ollamaEndpoint: 'http://localhost:11434',
  ollamaModel: 'llava',

  // 截屏配置
  captureEnabled: true,
  captureInterval: 1000,
  compressQuality: 80,
  skipUnchanged: true,      // 跳过无变化画面
  changeThreshold: 0.95,    // 变化阈值

  // 存储配置
  retentionDays: 7,
  maxScreenshots: 10000,
  maxContextChars: 10000,
})

const providerOptions = [
  { label: 'API (云端)', value: 'api' },
  { label: 'Ollama (本地)', value: 'ollama' },
]

const apiTypeOptions = [
  { label: 'OpenAI', value: 'openai' },
  { label: 'Claude', value: 'claude' },
  { label: '自定义', value: 'custom' },
]

async function loadSettings() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = await invoke<any>('get_config')
    if (config) {
      formValue.value = {
        provider: config.model?.provider || 'api',
        apiType: config.model?.api?.type || 'openai',
        apiEndpoint: config.model?.api?.endpoint || 'https://api.openai.com/v1',
        apiKey: config.model?.api?.api_key || '',
        apiModel: config.model?.api?.model || 'gpt-4-vision-preview',
        ollamaEndpoint: config.model?.ollama?.endpoint || 'http://localhost:11434',
        ollamaModel: config.model?.ollama?.model || 'llava',
        captureEnabled: config.capture?.enabled ?? true,
        captureInterval: config.capture?.interval_ms || 1000,
        compressQuality: config.capture?.compress_quality || 80,
        skipUnchanged: config.capture?.skip_unchanged ?? true,
        changeThreshold: config.capture?.change_threshold ?? 0.95,
        retentionDays: config.storage?.retention_days || 7,
        maxScreenshots: config.storage?.max_screenshots || 10000,
        maxContextChars: config.storage?.max_context_chars || 10000,
      }
    }
  } catch (error) {
    console.error('加载配置失败:', error)
  }
}

async function saveSettings() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = {
      model: {
        provider: formValue.value.provider,
        api: {
          type: formValue.value.apiType,
          endpoint: formValue.value.apiEndpoint,
          api_key: formValue.value.apiKey,
          model: formValue.value.apiModel,
        },
        ollama: {
          endpoint: formValue.value.ollamaEndpoint,
          model: formValue.value.ollamaModel,
        },
      },
      capture: {
        enabled: formValue.value.captureEnabled,
        interval_ms: formValue.value.captureInterval,
        compress_quality: formValue.value.compressQuality,
        skip_unchanged: formValue.value.skipUnchanged,
        change_threshold: formValue.value.changeThreshold,
      },
      storage: {
        retention_days: formValue.value.retentionDays,
        max_screenshots: formValue.value.maxScreenshots,
        max_context_chars: formValue.value.maxContextChars,
      },
    }

    await invoke('save_config', { config })
    message.success('设置已保存')
  } catch (error) {
    message.error(`保存失败: ${error}`)
  }
}

async function testConnection() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('test_model_connection')
    message.success('连接成功')
  } catch (error) {
    message.error(`连接失败: ${error}`)
  }
}

onMounted(() => {
  loadSettings()
})
</script>

<template>
  <NLayout class="settings-layout">
    <NLayoutContent class="settings-content">
      <h2>设置</h2>

      <NForm :model="formValue" label-placement="left" label-width="120">
        <!-- 模型配置 -->
        <NCard title="模型配置" size="small">
          <NFormItem label="模型来源">
            <NSelect v-model:value="formValue.provider" :options="providerOptions" />
          </NFormItem>

          <template v-if="formValue.provider === 'api'">
            <NFormItem label="API 类型">
              <NSelect v-model:value="formValue.apiType" :options="apiTypeOptions" />
            </NFormItem>
            <NFormItem label="API 地址">
              <NInput v-model:value="formValue.apiEndpoint" placeholder="https://api.openai.com/v1" />
            </NFormItem>
            <NFormItem label="API Key">
              <NInput v-model:value="formValue.apiKey" type="password" show-password-on="click" placeholder="sk-xxx" />
            </NFormItem>
            <NFormItem label="模型名称">
              <NInput v-model:value="formValue.apiModel" placeholder="gpt-4-vision-preview" />
            </NFormItem>
          </template>

          <template v-else>
            <NFormItem label="Ollama 地址">
              <NInput v-model:value="formValue.ollamaEndpoint" placeholder="http://localhost:11434" />
            </NFormItem>
            <NFormItem label="模型名称">
              <NInput v-model:value="formValue.ollamaModel" placeholder="llava" />
            </NFormItem>
          </template>

          <NFormItem>
            <NButton @click="testConnection">测试连接</NButton>
          </NFormItem>
        </NCard>

        <NDivider />

        <!-- 截屏配置 -->
        <NCard title="截屏配置" size="small">
          <NFormItem label="启用监控">
            <NSwitch v-model:value="formValue.captureEnabled" />
          </NFormItem>
          <NFormItem label="截屏间隔">
            <NInputNumber v-model:value="formValue.captureInterval" :min="500" :max="10000" :step="100">
              <template #suffix>毫秒</template>
            </NInputNumber>
          </NFormItem>
          <NFormItem label="压缩质量">
            <NInputNumber v-model:value="formValue.compressQuality" :min="10" :max="100" :step="10">
              <template #suffix>%</template>
            </NInputNumber>
          </NFormItem>
          <NFormItem label="跳过无变化">
            <NTooltip trigger="hover">
              <template #trigger>
                <NSwitch v-model:value="formValue.skipUnchanged" />
              </template>
              启用后，当画面无明显变化时跳过识别，节省Token消耗
            </NTooltip>
          </NFormItem>
          <NFormItem v-if="formValue.skipUnchanged" label="变化敏感度">
            <NTooltip trigger="hover">
              <template #trigger>
                <NInputNumber
                  v-model:value="formValue.changeThreshold"
                  :min="0.5"
                  :max="0.99"
                  :step="0.01"
                  :precision="2"
                >
                  <template #suffix>相似度</template>
                </NInputNumber>
              </template>
              相似度阈值，越高越容易跳过（0.95表示95%相似就跳过）
            </NTooltip>
          </NFormItem>
        </NCard>

        <NDivider />

        <!-- 存储配置 -->
        <NCard title="存储配置" size="small">
          <NFormItem label="保留天数">
            <NInputNumber v-model:value="formValue.retentionDays" :min="1" :max="30">
              <template #suffix>天</template>
            </NInputNumber>
          </NFormItem>
          <NFormItem label="上下文大小">
            <NTooltip trigger="hover">
              <template #trigger>
                <NInputNumber
                  v-model:value="formValue.maxContextChars"
                  :min="1000"
                  :max="100000"
                  :step="1000"
                >
                  <template #suffix>字符</template>
                </NInputNumber>
              </template>
              对话时加载的历史记录最大字符数，越大越详细但消耗更多Token
            </NTooltip>
          </NFormItem>
        </NCard>

        <NDivider />

        <NSpace justify="end">
          <NButton type="primary" @click="saveSettings">保存设置</NButton>
        </NSpace>
      </NForm>
    </NLayoutContent>
  </NLayout>
</template>

<style scoped>
.settings-layout {
  height: 100%;
}

.settings-content {
  padding: 24px;
  overflow-y: auto;
}

.settings-content h2 {
  margin-bottom: 24px;
  color: #63e2b7;
}

.n-card {
  margin-bottom: 16px;
}
</style>
