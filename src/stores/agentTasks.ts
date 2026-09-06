import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useChatStore } from './chat'
import { translate } from '../i18n'
import { useLocaleStore } from './locale'

export interface AgentTask {
  id: string
  conversation_id: string
  objective: string
  status: 'running' | 'completed' | 'failed' | 'cancelled' | 'interrupted'
  created_at: string
  updated_at: string
  steps: { title: string; status: string }[]
  artifacts: string[]
  verification: string[]
  response: string | null
  error: string | null
}

export const useAgentTasksStore = defineStore('agentTasks', () => {
  const tasks = ref<AgentTask[]>([])
  const error = ref('')
  let timer: ReturnType<typeof setTimeout> | null = null
  let refreshing: Promise<void> | null = null
  let started = false
  const delivered = new Set<string>()
  try {
    const saved: unknown = JSON.parse(localStorage.getItem('opencowork-delivered-tasks') || '[]')
    if (Array.isArray(saved)) saved.filter((id): id is string => typeof id === 'string').forEach(id => delivered.add(id))
  } catch { /* Recover from invalid local cache; message taskId still prevents duplicates. */ }

  async function refresh() {
    if (refreshing) await refreshing
    refreshing = refreshOnce()
    try { await refreshing } finally { refreshing = null }
  }

  async function refreshOnce() {
    try {
      tasks.value = await invoke<AgentTask[]>('list_agent_tasks', { conversationId: null })
      const chat = useChatStore()
      const locale = useLocaleStore()
      for (const task of tasks.value) {
        if (task.status === 'running' || delivered.has(task.id)) continue
        let content = task.response || translate(locale.locale, 'agent.stopped', { error: task.error || task.status })
        let toolContext
        let activeSkill
        try {
          const parsed = JSON.parse(content)
          if (typeof parsed.response === 'string') {
            content = parsed.response
            toolContext = parsed.tool_context
            activeSkill = parsed.active_skill
          }
        } catch { /* A plain text response is also valid. */ }
        const added = chat.addMessageToConversation(task.conversation_id, {
          role: 'assistant', content, timestamp: task.updated_at, taskId: task.id,
          toolContext, activeSkill,
        })
        if (added) {
          delivered.add(task.id)
          localStorage.setItem('opencowork-delivered-tasks', JSON.stringify([...delivered]))
        }
      }
      error.value = ''
    } catch (err) { error.value = String(err) }
  }

  function startPolling() {
    if (started) return
    started = true
    const poll = async () => {
      await refresh()
      timer = setTimeout(poll, 1500)
    }
    void poll()
  }

  async function start(input: Record<string, unknown>) {
    const id = await invoke<string>('start_agent_task', { input })
    await refresh()
    return id
  }

  async function stop(id: string) {
    await invoke('cancel_request', { requestId: id })
    await refresh()
  }

  async function resume(task: AgentTask) {
    return start({
      conversation_id: task.conversation_id, message: '', history: [], attachments: [],
      skill_name: null, skill_args: null, resume_from: task.id,
    })
  }

  function dispose() {
    if (timer) clearTimeout(timer)
    timer = null
    started = false
  }

  return { tasks, error, refresh, startPolling, start, stop, resume, dispose }
})
