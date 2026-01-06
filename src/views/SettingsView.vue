<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  NLayout,
  NLayoutContent,
  NCard,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NSelect,
  NButton,
  NSwitch,
  NDivider,
  NSpace,
  NTooltip,
  NDrawer,
  NDrawerContent,
  NTag,
  NSpin,
  useMessage,
} from 'naive-ui'

interface ProfileEntry {
  name: string
  subtitle: string
  detail: string
  serialized: string
  isActive: boolean
}

type DrawerMode = 'new' | 'edit' | 'copy'

const message = useMessage()

const profiles = ref<ProfileEntry[]>([])
const isLoading = ref(false)
const drawerVisible = ref(false)
const drawerMode = ref<DrawerMode>('new')
const profileName = ref('')
const currentConfigSerialized = ref('')
const currentConfig = ref<any | null>(null)

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
  skipUnchanged: true,
  changeThreshold: 0.95,
  recentSummaryLimit: 8,
  recentDetailLimit: 3,
  alertConfidenceThreshold: 0.7,
  alertCooldownSeconds: 120,

  // 存储配置
  retentionDays: 7,
  maxScreenshots: 10000,
  maxContextChars: 10000,
  autoClearOnStart: false,
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

const drawerTitle = computed(() => {
  if (drawerMode.value === 'edit') return '编辑方案'
  if (drawerMode.value === 'copy') return '复制方案'
  return '新建方案'
})

function normalizeConfig(raw: any) {
  return {
    model: {
      provider: raw?.model?.provider || 'api',
      api: {
        type: raw?.model?.api?.type || 'openai',
        endpoint: raw?.model?.api?.endpoint || 'https://api.openai.com/v1',
        api_key: raw?.model?.api?.api_key || '',
        model: raw?.model?.api?.model || 'gpt-4-vision-preview',
      },
      ollama: {
        endpoint: raw?.model?.ollama?.endpoint || 'http://localhost:11434',
        model: raw?.model?.ollama?.model || 'llava',
      },
    },
    capture: {
      enabled: raw?.capture?.enabled ?? true,
      interval_ms: raw?.capture?.interval_ms || 1000,
      compress_quality: raw?.capture?.compress_quality || 80,
      skip_unchanged: raw?.capture?.skip_unchanged ?? true,
      change_threshold: raw?.capture?.change_threshold ?? 0.95,
      recent_summary_limit: raw?.capture?.recent_summary_limit ?? 8,
      recent_detail_limit: raw?.capture?.recent_detail_limit ?? 3,
      alert_confidence_threshold: raw?.capture?.alert_confidence_threshold ?? 0.7,
      alert_cooldown_seconds: raw?.capture?.alert_cooldown_seconds ?? 120,
    },
    storage: {
      retention_days: raw?.storage?.retention_days || 7,
      max_screenshots: raw?.storage?.max_screenshots || 10000,
      max_context_chars: raw?.storage?.max_context_chars || 10000,
      auto_clear_on_start: raw?.storage?.auto_clear_on_start ?? false,
    },
  }
}

function serializeConfig(raw: any) {
  return JSON.stringify(normalizeConfig(raw))
}

function applyConfigToForm(config: any) {
  const normalized = normalizeConfig(config)
  formValue.value = {
    provider: normalized.model.provider,
    apiType: normalized.model.api.type,
    apiEndpoint: normalized.model.api.endpoint,
    apiKey: normalized.model.api.api_key,
    apiModel: normalized.model.api.model,
    ollamaEndpoint: normalized.model.ollama.endpoint,
    ollamaModel: normalized.model.ollama.model,
    captureEnabled: normalized.capture.enabled,
    captureInterval: normalized.capture.interval_ms,
    compressQuality: normalized.capture.compress_quality,
    skipUnchanged: normalized.capture.skip_unchanged,
    changeThreshold: normalized.capture.change_threshold,
    recentSummaryLimit: normalized.capture.recent_summary_limit ?? 8,
    recentDetailLimit: normalized.capture.recent_detail_limit ?? 3,
    alertConfidenceThreshold: normalized.capture.alert_confidence_threshold ?? 0.7,
    alertCooldownSeconds: normalized.capture.alert_cooldown_seconds ?? 120,
    retentionDays: normalized.storage.retention_days,
    maxScreenshots: normalized.storage.max_screenshots,
    maxContextChars: normalized.storage.max_context_chars,
    autoClearOnStart: normalized.storage.auto_clear_on_start ?? false,
  }
}

function buildConfigFromForm() {
  return normalizeConfig({
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
      recent_summary_limit: formValue.value.recentSummaryLimit,
      recent_detail_limit: formValue.value.recentDetailLimit,
      alert_confidence_threshold: formValue.value.alertConfidenceThreshold,
      alert_cooldown_seconds: formValue.value.alertCooldownSeconds,
    },
    storage: {
      retention_days: formValue.value.retentionDays,
      max_screenshots: formValue.value.maxScreenshots,
      max_context_chars: formValue.value.maxContextChars,
      auto_clear_on_start: formValue.value.autoClearOnStart,
    },
  })
}

function buildProfileSummary(config: any) {
  const normalized = normalizeConfig(config)
  if (normalized.model.provider === 'api') {
    return {
      subtitle: `API/${normalized.model.api.type} · ${normalized.model.api.model}`,
      detail: normalized.model.api.endpoint,
    }
  }
  return {
    subtitle: `Ollama · ${normalized.model.ollama.model}`,
    detail: normalized.model.ollama.endpoint,
  }
}

async function loadCurrentConfig() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = await invoke<any>('get_config')
    const normalized = normalizeConfig(config || {})
    currentConfig.value = normalized
    currentConfigSerialized.value = serializeConfig(normalized)
  } catch (error) {
    message.error(`加载当前配置失败: ${error}`)
  }
}

async function refreshProfiles() {
  isLoading.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const names = await invoke<string[]>('list_profiles')
    const entries: ProfileEntry[] = []

    for (const name of names || []) {
      try {
        const config = await invoke<any>('load_profile', { name })
        const normalized = normalizeConfig(config || {})
        const summary = buildProfileSummary(normalized)
        entries.push({
          name,
          subtitle: summary.subtitle,
          detail: summary.detail,
          serialized: serializeConfig(normalized),
          isActive: false,
        })
      } catch (error) {
        entries.push({
          name,
          subtitle: '读取失败',
          detail: String(error),
          serialized: '',
          isActive: false,
        })
      }
    }

    const activeName = entries.find((entry) => entry.serialized === currentConfigSerialized.value)?.name || null
    profiles.value = entries.map((entry) => ({
      ...entry,
      isActive: activeName === entry.name,
    }))
  } catch (error) {
    profiles.value = []
    message.error(`读取方案失败: ${error}`)
  } finally {
    isLoading.value = false
  }
}

async function openNewProfile() {
  drawerMode.value = 'new'
  profileName.value = ''
  applyConfigToForm(currentConfig.value || {})
  drawerVisible.value = true
}

async function editProfile(name: string) {
  drawerMode.value = 'edit'
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = await invoke<any>('load_profile', { name })
    applyConfigToForm(config || {})
    profileName.value = name
    drawerVisible.value = true
  } catch (error) {
    message.error(`读取方案失败: ${error}`)
  }
}

async function copyProfile(name: string) {
  drawerMode.value = 'copy'
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = await invoke<any>('load_profile', { name })
    applyConfigToForm(config || {})
    profileName.value = `${name}-copy`
    drawerVisible.value = true
  } catch (error) {
    message.error(`读取方案失败: ${error}`)
  }
}

async function saveProfileFromDrawer() {
  const name = profileName.value.trim()
  if (!name) {
    message.warning('请输入方案名称')
    return
  }
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = buildConfigFromForm()
    await invoke('save_profile', { name, config })
    drawerVisible.value = false
    message.success('方案已保存')
    await refreshProfiles()
  } catch (error) {
    message.error(`保存方案失败: ${error}`)
  }
}

async function applyConfig(config: any) {
  const normalized = normalizeConfig(config)
  const { invoke } = await import('@tauri-apps/api/core')
  await invoke('save_config', { config: normalized })
  currentConfig.value = normalized
  currentConfigSerialized.value = serializeConfig(normalized)
}

async function enableProfile(name: string) {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = await invoke<any>('load_profile', { name })
    await applyConfig(config || {})
    message.success('方案已启用')
    await refreshProfiles()
  } catch (error) {
    message.error(`启用方案失败: ${error}`)
  }
}

async function deleteProfile(name: string) {
  const confirmed = window.confirm(`确定删除方案 "${name}" 吗？`)
  if (!confirmed) return

  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('delete_profile', { name })
    message.success('方案已删除')
    await refreshProfiles()
  } catch (error) {
    message.error(`删除方案失败: ${error}`)
  }
}

async function testConnection() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = buildConfigFromForm()
    await invoke('test_model_connection', { config })
    message.success('连接成功')
  } catch (error) {
    message.error(`连接失败: ${error}`)
  }
}

onMounted(async () => {
  await loadCurrentConfig()
  await refreshProfiles()
})
</script>

<template>
  <NLayout class="settings-layout">
    <NLayoutContent class="settings-content">
      <div class="settings-header">
        <h2>配置方案</h2>
        <NButton type="primary" @click="openNewProfile">新建方案</NButton>
      </div>

      <div v-if="isLoading" class="loading-state">
        <NSpin size="small" />
        <span>正在加载方案...</span>
      </div>

      <div v-else>
        <div v-if="profiles.length === 0" class="empty-state">
          <p>暂无配置方案</p>
          <p class="muted">点击“新建方案”创建一个</p>
        </div>

        <div v-else class="profiles-list">
          <NCard
            v-for="profile in profiles"
            :key="profile.name"
            class="profile-card"
            :class="{ active: profile.isActive }"
          >
            <div class="profile-row">
              <div class="profile-info">
                <div class="profile-title">
                  <span>{{ profile.name }}</span>
                  <NTag v-if="profile.isActive" type="success" size="small">当前使用</NTag>
                </div>
                <div class="profile-sub">{{ profile.subtitle }}</div>
                <div class="profile-desc">{{ profile.detail }}</div>
              </div>
              <div class="profile-actions">
                <NButton size="small" type="primary" @click="enableProfile(profile.name)">启用</NButton>
                <NButton size="small" @click="editProfile(profile.name)">编辑</NButton>
                <NButton size="small" @click="copyProfile(profile.name)">复制</NButton>
                <NButton size="small" type="error" secondary @click="deleteProfile(profile.name)">
                  删除
                </NButton>
              </div>
            </div>
          </NCard>
        </div>
      </div>

      <NDrawer v-model:show="drawerVisible" placement="right" width="520">
        <NDrawerContent :title="drawerTitle" closable>
          <NForm :model="formValue" label-placement="left" label-width="120">
            <NCard title="方案信息" size="small">
              <NFormItem label="方案名称">
                <NInput v-model:value="profileName" placeholder="例如：工作/本地模型/写代码" />
              </NFormItem>
            </NCard>

            <NDivider />

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
                  <NInput
                    v-model:value="formValue.apiKey"
                    type="password"
                    show-password-on="click"
                    placeholder="sk-xxx"
                  />
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
              <NFormItem label="近期摘要条数">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.recentSummaryLimit"
                      :min="1"
                      :max="100"
                      :step="1"
                    >
                      <template #suffix>条</template>
                    </NInputNumber>
                  </template>
                  截图分析时带入最近的摘要条数（1-100）
                </NTooltip>
              </NFormItem>
              <NFormItem label="近期 detail 条数">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.recentDetailLimit"
                      :min="0"
                      :max="20"
                      :step="1"
                    >
                      <template #suffix>条</template>
                    </NInputNumber>
                  </template>
                  截图分析时带入最近的 detail 条数（0 表示不带）
                </NTooltip>
              </NFormItem>
              <NFormItem label="提醒置信度阈值">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.alertConfidenceThreshold"
                      :min="0"
                      :max="1"
                      :step="0.05"
                      :precision="2"
                    >
                      <template #suffix>置信度</template>
                    </NInputNumber>
                  </template>
                  有问题且置信度高于阈值时，自动在对话框提示建议
                </NTooltip>
              </NFormItem>
              <NFormItem label="提醒冷却时间">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.alertCooldownSeconds"
                      :min="10"
                      :max="3600"
                      :step="10"
                    >
                      <template #suffix>秒</template>
                    </NInputNumber>
                  </template>
                  相同问题在冷却时间内不重复提示，避免刷屏
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
              <NFormItem label="启动时清空历史">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NSwitch v-model:value="formValue.autoClearOnStart" />
                  </template>
                  开启后每次启动自动清空历史记录
                </NTooltip>
              </NFormItem>
            </NCard>

            <NDivider />

            <NSpace justify="end">
              <NButton @click="testConnection">测试连接</NButton>
              <NButton type="primary" @click="saveProfileFromDrawer">保存方案</NButton>
            </NSpace>
          </NForm>
        </NDrawerContent>
      </NDrawer>
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

.settings-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.settings-header h2 {
  margin: 0;
  color: #63e2b7;
}

.loading-state {
  display: flex;
  align-items: center;
  gap: 8px;
  color: rgba(255, 255, 255, 0.6);
  padding: 16px 0;
}

.empty-state {
  text-align: center;
  padding: 48px 16px;
  border: 1px dashed rgba(255, 255, 255, 0.12);
  border-radius: 12px;
  color: rgba(255, 255, 255, 0.6);
}

.empty-state .muted {
  margin-top: 8px;
  color: rgba(255, 255, 255, 0.4);
}

.profiles-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.profile-card {
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.profile-card.active {
  border-color: rgba(99, 226, 183, 0.6);
  background: rgba(99, 226, 183, 0.08);
}

.profile-row {
  display: flex;
  gap: 16px;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
}

.profile-info {
  flex: 1;
  min-width: 240px;
}

.profile-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 16px;
  font-weight: 600;
}

.profile-sub {
  margin-top: 6px;
  color: rgba(255, 255, 255, 0.7);
}

.profile-desc {
  margin-top: 4px;
  color: rgba(255, 255, 255, 0.4);
  font-size: 12px;
}

.profile-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  justify-content: flex-end;
}
</style>
