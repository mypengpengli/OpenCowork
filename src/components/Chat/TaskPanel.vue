<script setup lang="ts">
import { computed, ref } from 'vue'
import { NButton, NTag, NCollapse, NCollapseItem, NModal, useMessage } from 'naive-ui'
import { invoke } from '@tauri-apps/api/core'
import { useAgentTasksStore, type AgentTask } from '../../stores/agentTasks'
import { useChatStore } from '../../stores/chat'
import { useI18n } from '../../i18n'

const { t } = useI18n()
const store = useAgentTasksStore()
const chat = useChatStore()
const message = useMessage()
const tasks = computed(() => store.tasks.filter(task => task.conversation_id === chat.activeConversationId))
const running = computed(() => tasks.value.some(task => task.status === 'running'))
const preview = ref<{ kind: string; content: string; truncated?: boolean } | null>(null)
const previewTitle = ref('')
const previewVisible = ref(false)
async function act(task: AgentTask, action: 'stop' | 'resume') {
  try { await (action === 'stop' ? store.stop(task.id) : store.resume(task)) }
  catch (error) { message.error(String(error)) }
}
async function openArtifact(task: AgentTask, path: string) {
  try {
    preview.value = await invoke('preview_task_artifact', { taskId: task.id, path })
    previewTitle.value = path
    previewVisible.value = true
  }
  catch (error) { message.error(String(error)) }
}
</script>

<template>
  <section v-if="tasks.length || store.error" class="task-panel" :aria-label="t('agent.tasks')">
    <p v-if="store.error" role="alert">{{ store.error }}</p>
    <NCollapse>
      <NCollapseItem :title="`${t('agent.tasks')} (${tasks.length})`" name="tasks">
        <article v-for="task in tasks" :key="task.id" class="task-card">
          <div class="task-header">
            <strong>{{ task.objective }}</strong>
            <NTag size="small" :type="task.status === 'running' ? 'info' : task.status === 'completed' ? 'success' : 'warning'">
              {{ t(`agent.status.${task.status}`) }}
            </NTag>
          </div>
          <ol v-if="task.steps.length">
            <li v-for="(step, index) in task.steps" :key="index">
              {{ step.status === 'completed' ? '✓' : step.status === 'running' ? '◉' : '○' }} {{ step.title }}
            </li>
          </ol>
          <p v-if="task.error" role="alert">{{ task.error }}</p>
          <div v-if="task.artifacts.length" class="task-artifacts">
            <span>{{ t('agent.artifacts') }}</span>
            <NButton v-for="path in task.artifacts" :key="path" text @click="openArtifact(task, path)">{{ path }}</NButton>
          </div>
          <p>{{ t('agent.verification') }}</p>
          <ul v-if="task.verification.length"><li v-for="(evidence, i) in task.verification" :key="i">{{ evidence }}</li></ul>
          <p v-else class="task-muted">{{ t('agent.unverified') }}</p>
          <NButton v-if="task.status === 'running'" size="small" @click="act(task, 'stop')">{{ t('agent.stop') }}</NButton>
          <NButton v-else-if="task.status !== 'completed'" size="small" :disabled="running" @click="act(task, 'resume')">{{ t('agent.resume') }}</NButton>
        </article>
      </NCollapseItem>
    </NCollapse>
  </section>
  <NModal v-model:show="previewVisible" preset="card" :title="previewTitle" style="width: min(900px, 90vw)">
    <img v-if="preview?.kind === 'image'" :src="preview.content" :alt="previewTitle" style="max-width: 100%; max-height: 70vh">
    <pre v-else class="artifact-preview">{{ preview?.content }}</pre>
    <p v-if="preview?.truncated">{{ t('agent.previewTruncated') }}</p>
  </NModal>
</template>

<style scoped>
.task-panel { margin: 12px 20px; padding: 12px; border: 1px solid var(--n-border-color, #d8dce2); border-radius: 8px; }
.task-card { padding: 12px 0; border-bottom: 1px solid #8883; }
.task-header { display: flex; align-items: start; justify-content: space-between; gap: 12px; }
.task-header strong { overflow-wrap: anywhere; }
.task-artifacts { display: flex; align-items: start; flex-direction: column; gap: 6px; overflow-wrap: anywhere; }
.task-muted { opacity: .65; }
.artifact-preview { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 70vh; overflow: auto; }
</style>
