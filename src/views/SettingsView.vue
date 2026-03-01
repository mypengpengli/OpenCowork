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
  NTabs,
  NTabPane,
  NEmpty,
  NModal,
  useMessage,
} from 'naive-ui'
import { useSkillsStore } from '../stores/skills'
import { useLocaleStore } from '../stores/locale'
import { useI18n } from '../i18n'
import { open } from '@tauri-apps/plugin-dialog'

interface ProfileEntry {
  name: string
  subtitle: string
  detail: string
  serialized: string
  isActive: boolean
}

type DrawerMode = 'new' | 'edit' | 'copy'

const message = useMessage()
const skillsStore = useSkillsStore()
const { t } = useI18n()

// Skills 相关状态
const activeTab = ref('profiles')
const skillModalVisible = ref(false)
const newSkillName = ref('')
const newSkillDescription = ref('')
const newSkillInstructions = ref('')
const skillTemplate = ref('basic')
const lastSkillTemplate = ref('basic')
const skillsDir = ref('')
const systemLocale = ref('')
const systemLocaleError = ref('')
const localeStore = useLocaleStore()
const updateChecking = ref(false)
const updateInstalling = ref(false)
const updateAvailable = ref(false)
const updateVersion = ref('')
let pendingUpdate: any = null

// 全局提示词相关状态
interface GlobalPromptItem {
  id: string
  name: string
  content: string
  enabled: boolean
}
const globalPrompts = ref<GlobalPromptItem[]>([])
const promptModalVisible = ref(false)
const promptModalMode = ref<'new' | 'edit'>('new')
const editingPromptId = ref('')
const newPromptName = ref('')
const newPromptContent = ref('')

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
  apiRequestFormat: 'chat_completions',
  apiResponsesQueryParams: '',
  apiResponsesHeaders: '',
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
  maxContextChars: 1000000,
  maxContextTokens: 128000,
  contextCompressTriggerRatio: 0.92,
  autoClearOnStart: false,
  contextMode: 'auto',
  contextDetailHours: 24,

  // 工具权限
  toolMode: 'unset',
  toolAllowedCommands: '',
  toolAllowedDirs: '',
  showProcessStatus: true,
})

const providerOptions = computed(() => [
  { label: t('settings.form.provider.api'), value: 'api' },
  { label: t('settings.form.provider.ollama'), value: 'ollama' },
])

const apiTypeOptions = computed(() => [
  { label: 'OpenAI', value: 'openai' },
  { label: 'Claude', value: 'claude' },
  { label: t('settings.form.api.custom'), value: 'custom' },
])

const apiRequestFormatOptions = computed(() => [
  { label: t('settings.form.apiRequestFormat.chatCompletions'), value: 'chat_completions' },
  { label: t('settings.form.apiRequestFormat.responses'), value: 'responses' },
])

const skillTemplateOptions = computed(() => [
  { label: t('settings.skills.template.basic'), value: 'basic' },
  { label: t('settings.skills.template.file'), value: 'file' },
  { label: t('settings.skills.template.web'), value: 'web' },
  { label: t('settings.skills.template.doc'), value: 'doc' },
])

const toolModeOptions = computed(() => [
  { label: t('settings.tools.mode.unset'), value: 'unset' },
  { label: t('settings.tools.mode.whitelist'), value: 'whitelist' },
  { label: t('settings.tools.mode.allowAll'), value: 'allow_all' },
])

const contextModeOptions = computed(() => [
  { label: t('settings.form.contextMode.auto'), value: 'auto' },
  { label: t('settings.form.contextMode.always'), value: 'always' },
  { label: t('settings.form.contextMode.off'), value: 'off' },
])

const drawerTitle = computed(() => {
  if (drawerMode.value === 'edit') return t('settings.profile.drawer.edit')
  if (drawerMode.value === 'copy') return t('settings.profile.drawer.copy')
  return t('settings.profile.drawer.new')
})

function listToText(values?: string[]) {
  if (!values || values.length === 0) return ''
  return values.join('\n')
}

function mapToText(values?: Record<string, string>) {
  if (!values) return ''
  return Object.entries(values)
    .filter(([key]) => key.trim().length > 0)
    .map(([key, value]) => `${key}=${value}`)
    .join('\n')
}

function textToMap(value: string) {
  const result: Record<string, string> = {}
  for (const rawLine of value.split('\n')) {
    const line = rawLine.trim()
    if (!line) continue
    const equalsIndex = line.indexOf('=')
    if (equalsIndex <= 0) continue
    const key = line.slice(0, equalsIndex).trim()
    const val = line.slice(equalsIndex + 1).trim()
    if (!key) continue
    result[key] = val
  }
  return result
}


async function selectWorkspaceDir() {
  try {
    const selection = await open({
      directory: true,
      multiple: false,
    })
    if (!selection) return

    const path = Array.isArray(selection) ? selection[0] : selection
    const existing = textToList(formValue.value.toolAllowedDirs)
    const next = [path, ...existing.filter(item => item !== path)]
    formValue.value.toolAllowedDirs = next.join('\n')
  } catch (error) {
    message.error(String(error))
  }
}

function textToList(value: string) {
  return value
    .split(/[\n,]/)
    .map(item => item.trim())
    .filter(Boolean)
}

function normalizeConfig(raw: any) {
  return {
    model: {
      provider: raw?.model?.provider || 'api',
      api: {
        type: raw?.model?.api?.type || 'openai',
        request_format: raw?.model?.api?.request_format || 'chat_completions',
        responses_query_params: raw?.model?.api?.responses_query_params || {},
        responses_headers: raw?.model?.api?.responses_headers || {},
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
      max_context_chars: raw?.storage?.max_context_chars || 1000000,
      max_context_tokens: raw?.storage?.max_context_tokens || 128000,
      context_compress_trigger_ratio: raw?.storage?.context_compress_trigger_ratio ?? 0.92,
      auto_clear_on_start: raw?.storage?.auto_clear_on_start ?? false,
      context_mode: raw?.storage?.context_mode || 'auto',
      context_detail_hours: raw?.storage?.context_detail_hours ?? 24,
    },
    tools: {
      mode: raw?.tools?.mode || 'unset',
      allowed_commands: raw?.tools?.allowed_commands || [],
      allowed_dirs: raw?.tools?.allowed_dirs || [],
    },
    ui: {
      show_progress: raw?.ui?.show_progress ?? true,
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
    apiRequestFormat: normalized.model.api.request_format,
    apiResponsesQueryParams: mapToText(normalized.model.api.responses_query_params),
    apiResponsesHeaders: mapToText(normalized.model.api.responses_headers),
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
    maxContextTokens: normalized.storage.max_context_tokens ?? 128000,
    contextCompressTriggerRatio: normalized.storage.context_compress_trigger_ratio ?? 0.92,
    autoClearOnStart: normalized.storage.auto_clear_on_start ?? false,
    contextMode: normalized.storage.context_mode ?? 'auto',
    contextDetailHours: normalized.storage.context_detail_hours ?? 24,
    toolMode: normalized.tools?.mode || 'unset',
    toolAllowedCommands: listToText(normalized.tools?.allowed_commands),
    toolAllowedDirs: listToText(normalized.tools?.allowed_dirs),
    showProcessStatus: normalized.ui?.show_progress ?? true,
  }
}

function buildConfigFromForm() {
  return normalizeConfig({
    model: {
      provider: formValue.value.provider,
      api: {
        type: formValue.value.apiType,
        request_format: formValue.value.apiRequestFormat,
        responses_query_params: textToMap(formValue.value.apiResponsesQueryParams),
        responses_headers: textToMap(formValue.value.apiResponsesHeaders),
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
      max_context_tokens: formValue.value.maxContextTokens,
      context_compress_trigger_ratio: formValue.value.contextCompressTriggerRatio,
      auto_clear_on_start: formValue.value.autoClearOnStart,
      context_mode: formValue.value.contextMode,
      context_detail_hours: formValue.value.contextDetailHours,
    },
    tools: {
      mode: formValue.value.toolMode,
      allowed_commands: textToList(formValue.value.toolAllowedCommands),
      allowed_dirs: textToList(formValue.value.toolAllowedDirs),
    },
    ui: {
      show_progress: formValue.value.showProcessStatus,
    },
  })
}

function buildProfileSummary(config: any) {
  const normalized = normalizeConfig(config)
  if (normalized.model.provider === 'api') {
    const formatLabel =
      normalized.model.api.request_format === 'responses' ? 'responses' : 'chat'
    return {
      subtitle: `API/${normalized.model.api.type}/${formatLabel} · ${normalized.model.api.model}`,
      detail: normalized.model.api.endpoint,
    }
  }
  return {
    subtitle: `Ollama · ${normalized.model.ollama.model}`,
    detail: normalized.model.ollama.endpoint,
  }
}

async function loadSystemLocale() {
  systemLocaleError.value = ''
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    let storedLocale = ''
    let storedVersion = ''
    try {
      storedLocale = localStorage.getItem('opencowork-locale') || ''
      storedVersion = localStorage.getItem('opencowork-locale-version') || ''
    } catch {
      // localStorage unavailable
    }
    const locale = await invoke<string>('get_system_locale', {
      ui_locale: localeStore.locale,
      stored_locale: storedLocale || undefined,
      stored_version: storedVersion || undefined,
    })
    systemLocale.value = locale ? String(locale) : ''
    // 不再自动覆盖语言设置，仅用于显示系统语言信息
    // 语言设置由 locale store 管理，用户可以手动切换
  } catch (error) {
    systemLocaleError.value = String(error)
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
    message.error(t('settings.profile.loadConfigFailed', { error: String(error) }))
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
          subtitle: t('settings.profile.readFailed'),
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
    message.error(t('settings.profile.loadFailed', { error: String(error) }))
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
    message.error(t('settings.profile.loadFailed', { error: String(error) }))
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
    message.error(t('settings.profile.loadFailed', { error: String(error) }))
  }
}

async function saveProfileFromDrawer() {
  const name = profileName.value.trim()
  if (!name) {
    message.warning(t('settings.profile.nameRequired'))
    return
  }
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = buildConfigFromForm()
    await invoke('save_profile', { name, config })
    drawerVisible.value = false
    message.success(t('settings.profile.saveSuccess'))
    await refreshProfiles()
  } catch (error) {
    message.error(t('settings.profile.saveFailed', { error: String(error) }))
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
    message.success(t('settings.profile.enableSuccess'))
    await refreshProfiles()
  } catch (error) {
    message.error(t('settings.profile.enableFailed', { error: String(error) }))
  }
}

async function deleteProfile(name: string) {
  const confirmed = window.confirm(t('settings.profile.deleteConfirm', { name }))
  if (!confirmed) return

  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('delete_profile', { name })
    message.success(t('settings.profile.deleteSuccess'))
    await refreshProfiles()
  } catch (error) {
    message.error(t('settings.profile.deleteFailed', { error: String(error) }))
  }
}

async function testConnection() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = buildConfigFromForm()
    await invoke('test_model_connection', { config })
    message.success(t('settings.connection.success'))
  } catch (error) {
    message.error(t('settings.connection.failed', { error: String(error) }))
  }
}

onMounted(async () => {
  await loadSystemLocale()
  await loadCurrentConfig()
  await refreshProfiles()
  // 加载 Skills
  await skillsStore.loadSkills()
  skillsDir.value = await skillsStore.getSkillsDir()
  // 加载全局提示词
  await loadGlobalPrompts()
})

// 全局提示词相关函数
async function loadGlobalPrompts() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = await invoke<any>('get_config')
    globalPrompts.value = config?.global_prompt?.items || []
  } catch (error) {
    console.error('加载全局提示词失败:', error)
    globalPrompts.value = []
  }
}

async function saveGlobalPrompts() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const config = await invoke<any>('get_config')
    const updatedConfig = {
      ...config,
      global_prompt: {
        items: globalPrompts.value
      }
    }
    await invoke('save_config', { config: updatedConfig })
    message.success(t('settings.prompt.saveSuccess'))
  } catch (error) {
    message.error(t('settings.prompt.saveFailed', { error: String(error) }))
  }
}

function openCreatePromptModal() {
  promptModalMode.value = 'new'
  editingPromptId.value = ''
  newPromptName.value = ''
  newPromptContent.value = ''
  promptModalVisible.value = true
}

function openEditPromptModal(prompt: GlobalPromptItem) {
  promptModalMode.value = 'edit'
  editingPromptId.value = prompt.id
  newPromptName.value = prompt.name
  newPromptContent.value = prompt.content
  promptModalVisible.value = true
}

async function savePrompt() {
  if (!newPromptName.value.trim()) {
    message.warning(t('settings.prompt.modal.name'))
    return
  }
  if (!newPromptContent.value.trim()) {
    message.warning(t('settings.prompt.modal.content'))
    return
  }

  if (promptModalMode.value === 'new') {
    // 创建新提示词
    const newPrompt: GlobalPromptItem = {
      id: crypto.randomUUID(),
      name: newPromptName.value.trim(),
      content: newPromptContent.value.trim(),
      enabled: true
    }
    globalPrompts.value.push(newPrompt)
  } else {
    // 编辑现有提示词
    const index = globalPrompts.value.findIndex(p => p.id === editingPromptId.value)
    if (index !== -1) {
      globalPrompts.value[index].name = newPromptName.value.trim()
      globalPrompts.value[index].content = newPromptContent.value.trim()
    }
  }

  await saveGlobalPrompts()
  promptModalVisible.value = false
}

async function deletePrompt(prompt: GlobalPromptItem) {
  const confirmed = window.confirm(t('settings.prompt.deleteConfirm', { name: prompt.name }))
  if (!confirmed) return

  globalPrompts.value = globalPrompts.value.filter(p => p.id !== prompt.id)
  await saveGlobalPrompts()
  message.success(t('settings.prompt.deleteSuccess'))
}

async function togglePromptEnabled(prompt: GlobalPromptItem) {
  prompt.enabled = !prompt.enabled
  await saveGlobalPrompts()
}

// Skills 相关函数
function openCreateSkillModal() {
  newSkillName.value = ''
  newSkillDescription.value = ''
  skillTemplate.value = 'basic'
  lastSkillTemplate.value = 'basic'
  applySkillTemplate('basic', true)
  skillModalVisible.value = true
}

function applySkillTemplate(value: string, force = false) {
  const nextContent = t(`settings.skills.templateContent.${value}`)
  const currentContent = newSkillInstructions.value.trim()
  const lastContent = t(`settings.skills.templateContent.${lastSkillTemplate.value}`).trim()

  if (!force && currentContent && currentContent !== lastContent) {
    const confirmed = window.confirm(t('settings.skills.templateChangeConfirm'))
    if (!confirmed) {
      skillTemplate.value = lastSkillTemplate.value
      return
    }
  }

  newSkillInstructions.value = nextContent
  lastSkillTemplate.value = value
}

async function createNewSkill() {
  if (!newSkillName.value.trim()) {
    message.warning(t('settings.skills.nameRequired'))
    return
  }
  if (!newSkillDescription.value.trim()) {
    message.warning(t('settings.skills.descRequired'))
    return
  }

  const success = await skillsStore.createSkill(
    newSkillName.value.trim(),
    newSkillDescription.value.trim(),
    newSkillInstructions.value.trim()
  )

  if (success) {
    message.success(t('settings.skills.createSuccess'))
    skillModalVisible.value = false
  } else {
    message.error(t('settings.skills.createFailed'))
  }
}

async function handleDeleteSkill(name: string) {
  const confirmed = window.confirm(t('settings.skills.deleteConfirm', { name }))
  if (!confirmed) return

  const success = await skillsStore.deleteSkill(name)
  if (success) {
    message.success(t('settings.skills.deleteSuccess'))
  } else {
    message.error(t('settings.skills.deleteFailed'))
  }
}

async function openSkillsFolder() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('open_skills_dir')
  } catch (error) {
    // 如果打开失败，回退到复制路径
    try {
      await navigator.clipboard.writeText(skillsDir.value)
      message.success(t('settings.skills.openDirCopied', { dir: skillsDir.value }))
    } catch {
      message.info(t('settings.skills.openDirInfo', { dir: skillsDir.value }))
    }
  }
}

async function openReleasePage() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('open_release_page')
  } catch (error) {
    message.error(t('settings.update.openFailed', { error: String(error) }))
  }
}

function resetUpdateState() {
  pendingUpdate = null
  updateAvailable.value = false
  updateVersion.value = ''
}

async function checkForUpdates() {
  if (updateChecking.value || updateInstalling.value) return
  updateChecking.value = true
  try {
    const { check } = await import('@tauri-apps/plugin-updater')
    const update = await check()
    if (update?.available) {
      pendingUpdate = update
      updateAvailable.value = true
      updateVersion.value = update.version || ''
      message.info(
        t('settings.update.available', {
          version: updateVersion.value || t('common.unknown'),
        }),
      )
      return
    }
    resetUpdateState()
    message.success(t('settings.update.upToDate'))
  } catch (error) {
    message.error(t('settings.update.failed', { error: String(error) }))
  } finally {
    updateChecking.value = false
  }
}

async function installUpdate() {
  if (updateInstalling.value) return
  if (!pendingUpdate) {
    await checkForUpdates()
  }
  if (!pendingUpdate) return
  updateInstalling.value = true
  try {
    message.info(t('settings.update.downloading'))
    await pendingUpdate.downloadAndInstall()
    message.success(t('settings.update.installing'))
  } catch (error) {
    message.error(t('settings.update.installFailed', { error: String(error) }))
  } finally {
    updateInstalling.value = false
  }
}
</script>

<template>
  <NLayout class="settings-layout">
    <NLayoutContent class="settings-content">
      <NTabs v-model:value="activeTab" type="line">
        <!-- 配置方案 Tab -->
        <NTabPane name="profiles" :tab="t('settings.tabs.profiles')">
          <div class="settings-header">
            <div class="settings-title">
              <h2>{{ t('settings.header.profiles') }}</h2>
            </div>
            <NSpace>
              <NTag v-if="updateAvailable" type="warning" size="small">
                {{ t('settings.update.availableTag', { version: updateVersion || t('common.unknown') }) }}
              </NTag>
              <NButton
                :loading="updateChecking || updateInstalling"
                :type="updateAvailable ? 'primary' : 'default'"
                @click="updateAvailable ? installUpdate() : checkForUpdates()"
              >
                {{ updateAvailable ? t('settings.buttons.startUpdate') : t('settings.buttons.checkUpdate') }}
              </NButton>
              <NButton type="primary" @click="openNewProfile">{{ t('settings.buttons.newProfile') }}</NButton>
            </NSpace>
          </div>

          <div v-if="isLoading" class="loading-state">
            <NSpin size="small" />
            <span>{{ t('settings.loading.profiles') }}</span>
          </div>

          <div v-else>
            <div v-if="profiles.length === 0" class="empty-state">
              <p>{{ t('settings.empty.profiles') }}</p>
              <p class="muted">{{ t('settings.empty.profilesHint') }}</p>
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
                      <NTag v-if="profile.isActive" type="success" size="small">
                        {{ t('settings.profile.active') }}
                      </NTag>
                    </div>
                    <div class="profile-sub">{{ profile.subtitle }}</div>
                    <div class="profile-desc">{{ profile.detail }}</div>
                  </div>
                  <div class="profile-actions">
                    <NButton size="small" type="primary" @click="enableProfile(profile.name)">
                      {{ t('common.enable') }}
                    </NButton>
                    <NButton size="small" @click="editProfile(profile.name)">
                      {{ t('common.edit') }}
                    </NButton>
                    <NButton size="small" @click="copyProfile(profile.name)">
                      {{ t('common.copy') }}
                    </NButton>
                    <NButton size="small" type="error" secondary @click="deleteProfile(profile.name)">
                      {{ t('common.delete') }}
                    </NButton>
                  </div>
                </div>
              </NCard>
            </div>
          </div>
        </NTabPane>

        <!-- Skills Tab -->
        <NTabPane name="skills" :tab="t('settings.tabs.skills')">
          <div class="settings-header">
            <h2>{{ t('settings.header.skills') }}</h2>
            <NSpace>
              <NButton @click="openSkillsFolder">{{ t('settings.buttons.openSkillsFolder') }}</NButton>
              <NButton type="primary" @click="openCreateSkillModal">{{ t('settings.buttons.newSkill') }}</NButton>
            </NSpace>
          </div>

          <div v-if="skillsStore.isLoading" class="loading-state">
            <NSpin size="small" />
            <span>{{ t('settings.loading.skills') }}</span>
          </div>

          <div v-else>
            <div v-if="skillsStore.availableSkills.length === 0" class="empty-state">
              <p>{{ t('settings.empty.skills') }}</p>
              <p class="muted">{{ t('settings.empty.skillsHint') }}</p>
              <p class="muted" style="margin-top: 8px;">
                {{ t('settings.empty.skillsDir', { dir: skillsDir }) }}
              </p>
            </div>

            <div v-else class="skills-list">
              <NCard
                v-for="skill in skillsStore.availableSkills"
                :key="skill.name"
                class="skill-card"
              >
                <div class="skill-row">
                  <div class="skill-info">
                    <div class="skill-title">
                      <span>/{{ skill.name }}</span>
                    </div>
                    <div class="skill-desc">{{ skill.description }}</div>
                  </div>
                  <div class="skill-actions">
                    <NButton size="small" type="error" secondary @click="handleDeleteSkill(skill.name)">
                      {{ t('common.delete') }}
                    </NButton>
                  </div>
                </div>
              </NCard>
            </div>
          </div>

          <div class="skills-help">
            <NDivider />
            <h3>{{ t('settings.skills.help.title') }}</h3>
            <ul>
              <li v-html="t('settings.skills.help.item1')"></li>
              <li v-html="t('settings.skills.help.item2')"></li>
              <li>{{ t('settings.skills.help.item3') }}</li>
            </ul>
          </div>
        </NTabPane>

        <!-- 全局提示词 Tab -->
        <NTabPane name="prompts" :tab="t('settings.tabs.prompts')">
          <div class="settings-header">
            <h2>{{ t('settings.header.prompts') }}</h2>
            <NSpace>
              <NButton type="primary" @click="openCreatePromptModal">{{ t('settings.buttons.newPrompt') }}</NButton>
            </NSpace>
          </div>

          <div v-if="globalPrompts.length === 0" class="empty-state">
            <p>{{ t('settings.empty.prompts') }}</p>
            <p class="muted">{{ t('settings.empty.promptsHint') }}</p>
          </div>

          <div v-else class="prompts-list">
            <NCard
              v-for="prompt in globalPrompts"
              :key="prompt.id"
              class="prompt-card"
              :class="{ active: prompt.enabled }"
            >
              <div class="prompt-row">
                <div class="prompt-info">
                  <div class="prompt-title">
                    <span>{{ prompt.name }}</span>
                    <NTag v-if="prompt.enabled" type="success" size="small">启用</NTag>
                  </div>
                  <div class="prompt-content">{{ prompt.content.length > 100 ? prompt.content.slice(0, 100) + '...' : prompt.content }}</div>
                </div>
                <div class="prompt-actions">
                  <NSwitch :value="prompt.enabled" @update:value="togglePromptEnabled(prompt)" />
                  <NButton size="small" @click="openEditPromptModal(prompt)">
                    {{ t('common.edit') }}
                  </NButton>
                  <NButton size="small" type="error" secondary @click="deletePrompt(prompt)">
                    {{ t('common.delete') }}
                  </NButton>
                </div>
              </div>
            </NCard>
          </div>

          <div class="prompts-help">
            <NDivider />
            <h3>{{ t('settings.prompt.help.title') }}</h3>
            <ul>
              <li>{{ t('settings.prompt.help.item1') }}</li>
              <li>{{ t('settings.prompt.help.item2') }}</li>
              <li>{{ t('settings.prompt.help.item3') }}</li>
            </ul>
          </div>
        </NTabPane>
      </NTabs>

      <NDrawer v-model:show="drawerVisible" placement="right" width="520">
        <NDrawerContent :title="drawerTitle" closable>
          <NForm :model="formValue" label-placement="left" label-width="120">
            <NCard :title="t('settings.form.profileInfo')" size="small">
              <NFormItem :label="t('settings.form.profileName')">
                <NInput v-model:value="profileName" :placeholder="t('settings.form.profileNamePlaceholder')" />
              </NFormItem>
            </NCard>

            <NDivider />

            <!-- 模型配置 -->
            <NCard :title="t('settings.form.modelConfig')" size="small">
              <NFormItem :label="t('settings.form.modelProvider')">
                <NSelect v-model:value="formValue.provider" :options="providerOptions" />
              </NFormItem>

              <template v-if="formValue.provider === 'api'">
                <NFormItem :label="t('settings.form.apiType')">
                  <NSelect v-model:value="formValue.apiType" :options="apiTypeOptions" />
                </NFormItem>
                <NFormItem :label="t('settings.form.apiRequestFormat')">
                  <NSelect v-model:value="formValue.apiRequestFormat" :options="apiRequestFormatOptions" />
                </NFormItem>
                <template v-if="formValue.apiRequestFormat === 'responses'">
                  <NFormItem :label="t('settings.form.responsesQueryParams')">
                    <NInput
                      v-model:value="formValue.apiResponsesQueryParams"
                      type="textarea"
                      :autosize="{ minRows: 2, maxRows: 6 }"
                      :placeholder="t('settings.form.responsesQueryParamsPlaceholder')"
                    />
                  </NFormItem>
                  <NFormItem :label="t('settings.form.responsesHeaders')">
                    <NInput
                      v-model:value="formValue.apiResponsesHeaders"
                      type="textarea"
                      :autosize="{ minRows: 2, maxRows: 6 }"
                      :placeholder="t('settings.form.responsesHeadersPlaceholder')"
                    />
                  </NFormItem>
                </template>
                <NFormItem :label="t('settings.form.apiEndpoint')">
                  <NInput v-model:value="formValue.apiEndpoint" placeholder="https://api.openai.com/v1" />
                </NFormItem>
                <NFormItem :label="t('settings.form.apiKey')">
                  <NInput
                    v-model:value="formValue.apiKey"
                    type="password"
                    show-password-on="click"
                    placeholder="sk-xxx"
                  />
                </NFormItem>
                <NFormItem :label="t('settings.form.modelName')">
                  <NInput v-model:value="formValue.apiModel" placeholder="gpt-4-vision-preview" />
                </NFormItem>
              </template>

              <template v-else>
                <NFormItem :label="t('settings.form.ollamaEndpoint')">
                  <NInput v-model:value="formValue.ollamaEndpoint" placeholder="http://localhost:11434" />
                </NFormItem>
                <NFormItem :label="t('settings.form.modelName')">
                  <NInput v-model:value="formValue.ollamaModel" placeholder="llava" />
                </NFormItem>
              </template>
            </NCard>

            <NDivider />

            <!-- 截屏配置 -->
            <NCard :title="t('settings.form.captureConfig')" size="small">
              <NFormItem :label="t('settings.form.captureEnable')">
                <NSwitch v-model:value="formValue.captureEnabled" />
              </NFormItem>
              <NFormItem :label="t('settings.form.captureInterval')">
                <NInputNumber v-model:value="formValue.captureInterval" :min="500" :max="10000" :step="100">
                  <template #suffix>{{ t('settings.form.captureIntervalUnit') }}</template>
                </NInputNumber>
              </NFormItem>
              <NFormItem :label="t('settings.form.compressQuality')">
                <NInputNumber v-model:value="formValue.compressQuality" :min="10" :max="100" :step="10">
                  <template #suffix>%</template>
                </NInputNumber>
              </NFormItem>
              <NFormItem :label="t('settings.form.skipUnchanged')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NSwitch v-model:value="formValue.skipUnchanged" />
                  </template>
                  {{ t('settings.form.skipUnchangedTip') }}
                </NTooltip>
              </NFormItem>
              <NFormItem v-if="formValue.skipUnchanged" :label="t('settings.form.changeThreshold')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.changeThreshold"
                      :min="0.5"
                      :max="0.99"
                      :step="0.01"
                      :precision="2"
                    >
                      <template #suffix>{{ t('settings.form.changeThresholdUnit') }}</template>
                    </NInputNumber>
                  </template>
                  {{ t('settings.form.changeThresholdTip') }}
                </NTooltip>
              </NFormItem>
              <NFormItem :label="t('settings.form.recentSummaryLimit')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.recentSummaryLimit"
                      :min="1"
                      :max="100"
                      :step="1"
                    >
                      <template #suffix>{{ t('settings.form.countUnit') }}</template>
                    </NInputNumber>
                  </template>
                  {{ t('settings.form.recentSummaryTip') }}
                </NTooltip>
              </NFormItem>
              <NFormItem :label="t('settings.form.recentDetailLimit')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.recentDetailLimit"
                      :min="0"
                      :max="20"
                      :step="1"
                    >
                      <template #suffix>{{ t('settings.form.countUnit') }}</template>
                    </NInputNumber>
                  </template>
                  {{ t('settings.form.recentDetailTip') }}
                </NTooltip>
              </NFormItem>
              <NFormItem :label="t('settings.form.alertConfidence')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.alertConfidenceThreshold"
                      :min="0"
                      :max="1"
                      :step="0.05"
                      :precision="2"
                    >
                      <template #suffix>{{ t('settings.form.confidenceUnit') }}</template>
                    </NInputNumber>
                  </template>
                  {{ t('settings.form.alertConfidenceTip') }}
                </NTooltip>
              </NFormItem>
              <NFormItem :label="t('settings.form.alertCooldown')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.alertCooldownSeconds"
                      :min="10"
                      :max="3600"
                      :step="10"
                    >
                      <template #suffix>{{ t('settings.form.secondsUnit') }}</template>
                    </NInputNumber>
                  </template>
                  {{ t('settings.form.alertCooldownTip') }}
                </NTooltip>
              </NFormItem>
            </NCard>

            <NDivider />

            <!-- 存储配置 -->
            <NCard :title="t('settings.form.storageConfig')" size="small">
              <NFormItem :label="t('settings.form.retentionDays')">
                <NInputNumber v-model:value="formValue.retentionDays" :min="1" :max="30">
                  <template #suffix>{{ t('settings.form.daysUnit') }}</template>
                </NInputNumber>
              </NFormItem>
              <NFormItem :label="t('settings.form.contextSize')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.maxContextChars"
                      :min="1000"
                      :step="1000"
                    >
                      <template #suffix>{{ t('settings.form.charsUnit') }}</template>
                    </NInputNumber>
                  </template>
                  {{ t('settings.form.contextSizeTip') }}
                </NTooltip>
              </NFormItem>
              <NFormItem :label="t('settings.form.maxContextTokens')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.maxContextTokens"
                      :min="4096"
                      :step="1024"
                    >
                      <template #suffix>{{ t('settings.form.tokensUnit') }}</template>
                    </NInputNumber>
                  </template>
                  {{ t('settings.form.maxContextTokensTip') }}
                </NTooltip>
              </NFormItem>
              <NFormItem :label="t('settings.form.compressTriggerRatio')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.contextCompressTriggerRatio"
                      :min="0.7"
                      :max="0.99"
                      :step="0.01"
                      :precision="2"
                    >
                      <template #suffix>{{ t('settings.form.ratioUnit') }}</template>
                    </NInputNumber>
                  </template>
                  {{ t('settings.form.compressTriggerRatioTip') }}
                </NTooltip>
              </NFormItem>
              <NFormItem :label="t('settings.form.contextMode')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NSelect v-model:value="formValue.contextMode" :options="contextModeOptions" />
                  </template>
                  {{ t('settings.form.contextModeTip') }}
                </NTooltip>
              </NFormItem>
              <NFormItem :label="t('settings.form.contextDetailHours')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NInputNumber
                      v-model:value="formValue.contextDetailHours"
                      :min="0"
                      :max="168"
                      :step="1"
                    >
                      <template #suffix>{{ t('settings.form.hoursUnit') }}</template>
                    </NInputNumber>
                  </template>
                  {{ t('settings.form.contextDetailHoursTip') }}
                </NTooltip>
              </NFormItem>
              <NFormItem :label="t('settings.form.autoClear')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NSwitch v-model:value="formValue.autoClearOnStart" />
                  </template>
                  {{ t('settings.form.autoClearTip') }}
                </NTooltip>
              </NFormItem>
            </NCard>

            <NDivider />

            <!-- 工具权限 -->
            <NCard :title="t('settings.form.toolsConfig')" size="small">
              <NFormItem :label="t('settings.form.toolsMode')">
                <NSelect v-model:value="formValue.toolMode" :options="toolModeOptions" />
              </NFormItem>
              <NFormItem :label="t('settings.form.toolsAllowedCommands')">
                <NInput
                  v-model:value="formValue.toolAllowedCommands"
                  type="textarea"
                  :autosize="{ minRows: 2, maxRows: 6 }"
                  :placeholder="t('settings.form.toolsAllowedCommandsPlaceholder')"
                />
              </NFormItem>
              <NFormItem :label="t('settings.form.toolsAllowedDirs')">
                <NInput
                  v-model:value="formValue.toolAllowedDirs"
                  type="textarea"
                  :autosize="{ minRows: 2, maxRows: 6 }"
                  :placeholder="t('settings.form.toolsAllowedDirsPlaceholder')"
                />
                <NSpace align="center" size="small" class="tools-dir-actions">
                  <NButton size="small" @click="selectWorkspaceDir">{{ t('settings.form.toolsPickWorkspace') }}</NButton>
                  <span class="tools-dir-hint">{{ t('settings.form.toolsAllowedDirsHint') }}</span>
                </NSpace>
              </NFormItem>
            </NCard>

            <NDivider />

            <!-- 界面配置 -->
            <NCard :title="t('settings.form.uiConfig')" size="small">
              <NFormItem :label="t('settings.form.showProcess')">
                <NTooltip trigger="hover">
                  <template #trigger>
                    <NSwitch v-model:value="formValue.showProcessStatus" />
                  </template>
                  {{ t('settings.form.showProcessTip') }}
                </NTooltip>
              </NFormItem>
            </NCard>

            <NDivider />

            <NSpace justify="end">
              <NButton @click="testConnection">{{ t('settings.form.testConnection') }}</NButton>
              <NButton type="primary" @click="saveProfileFromDrawer">{{ t('settings.form.saveProfile') }}</NButton>
            </NSpace>
          </NForm>
        </NDrawerContent>
      </NDrawer>

      <!-- 创建技能模态框 -->
      <NModal v-model:show="skillModalVisible" preset="card" :title="t('settings.skills.modal.title')" style="width: 600px;">
        <NForm label-placement="left" label-width="100">
          <NFormItem :label="t('settings.skills.modal.name')">
            <NInput
              v-model:value="newSkillName"
              :placeholder="t('settings.skills.modal.namePlaceholder')"
            />
          </NFormItem>
          <NFormItem :label="t('settings.skills.modal.description')">
            <NInput
              v-model:value="newSkillDescription"
              type="textarea"
              :autosize="{ minRows: 2, maxRows: 4 }"
              :placeholder="t('settings.skills.modal.descriptionPlaceholder')"
            />
          </NFormItem>
          <NFormItem :label="t('settings.skills.templateLabel')">
            <NSelect
              v-model:value="skillTemplate"
              :options="skillTemplateOptions"
              @update:value="applySkillTemplate"
            />
          </NFormItem>
          <NFormItem :label="t('settings.skills.modal.instructions')">
            <NInput
              v-model:value="newSkillInstructions"
              type="textarea"
              :autosize="{ minRows: 8, maxRows: 16 }"
              :placeholder="t('settings.skills.modal.instructionsPlaceholder')"
            />
          </NFormItem>
        </NForm>
        <template #footer>
          <NSpace justify="end">
            <NButton @click="skillModalVisible = false">{{ t('settings.skills.modal.cancel') }}</NButton>
            <NButton type="primary" @click="createNewSkill">{{ t('settings.skills.modal.create') }}</NButton>
          </NSpace>
        </template>
      </NModal>

      <!-- 创建/编辑提示词模态框 -->
      <NModal v-model:show="promptModalVisible" preset="card" :title="promptModalMode === 'new' ? t('settings.prompt.modal.titleNew') : t('settings.prompt.modal.titleEdit')" style="width: 600px;">
        <NForm label-placement="left" label-width="100">
          <NFormItem :label="t('settings.prompt.modal.name')">
            <NInput
              v-model:value="newPromptName"
              :placeholder="t('settings.prompt.modal.namePlaceholder')"
            />
          </NFormItem>
          <NFormItem :label="t('settings.prompt.modal.content')">
            <NInput
              v-model:value="newPromptContent"
              type="textarea"
              :autosize="{ minRows: 4, maxRows: 12 }"
              :placeholder="t('settings.prompt.modal.contentPlaceholder')"
            />
          </NFormItem>
        </NForm>
        <template #footer>
          <NSpace justify="end">
            <NButton @click="promptModalVisible = false">{{ t('common.cancel') }}</NButton>
            <NButton type="primary" @click="savePrompt">{{ t('common.save') }}</NButton>
          </NSpace>
        </template>
      </NModal>
    </NLayoutContent>
  </NLayout>
</template>

<style scoped>
.tools-dir-actions {
  margin-top: 6px;
}

.tools-dir-hint {
  font-size: 12px;
  color: rgba(255, 255, 255, 0.45);
}

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

.settings-title {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.settings-header h2 {
  margin: 0;
  color: #63e2b7;
}

.settings-locale {
  margin: 0;
  font-size: 12px;
  color: rgba(255, 255, 255, 0.45);
}

.settings-locale.error {
  color: rgba(255, 107, 107, 0.9);
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

/* Skills 样式 */
.skills-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.skill-card {
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.skill-row {
  display: flex;
  gap: 16px;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
}

.skill-info {
  flex: 1;
  min-width: 240px;
}

.skill-title {
  font-size: 16px;
  font-weight: 600;
  color: #63e2b7;
}

.skill-desc {
  margin-top: 6px;
  color: rgba(255, 255, 255, 0.7);
  font-size: 14px;
}

.skill-actions {
  display: flex;
  gap: 8px;
}

.skills-help {
  margin-top: 24px;
}

.skills-help h3 {
  color: rgba(255, 255, 255, 0.8);
  margin-bottom: 12px;
}

.skills-help ul {
  color: rgba(255, 255, 255, 0.6);
  padding-left: 20px;
}

.skills-help li {
  margin: 8px 0;
}

.skills-help code {
  background: rgba(99, 226, 183, 0.1);
  color: #63e2b7;
  padding: 2px 6px;
  border-radius: 4px;
}

/* 全局提示词样式 */
.prompts-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.prompt-card {
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.prompt-card.active {
  border-color: rgba(99, 226, 183, 0.6);
  background: rgba(99, 226, 183, 0.08);
}

.prompt-row {
  display: flex;
  gap: 16px;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
}

.prompt-info {
  flex: 1;
  min-width: 240px;
}

.prompt-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 16px;
  font-weight: 600;
}

.prompt-content {
  margin-top: 6px;
  color: rgba(255, 255, 255, 0.6);
  font-size: 14px;
  white-space: pre-wrap;
  word-break: break-word;
}

.prompt-actions {
  display: flex;
  gap: 8px;
  align-items: center;
}

.prompts-help {
  margin-top: 24px;
}

.prompts-help h3 {
  color: rgba(255, 255, 255, 0.8);
  margin-bottom: 12px;
}

.prompts-help ul {
  color: rgba(255, 255, 255, 0.6);
  padding-left: 20px;
}

.prompts-help li {
  margin: 8px 0;
}
</style>
