import { defineStore } from 'pinia'
import { ref } from 'vue'

export interface SkillMetadata {
  name: string
  description: string
  allowed_tools?: string[]
  model?: string
  context?: string
  user_invocable?: boolean
  metadata?: Record<string, string>
}

export interface Skill extends SkillMetadata {
  instructions: string
  path: string
}

export const useSkillsStore = defineStore('skills', () => {
  const availableSkills = ref<SkillMetadata[]>([])
  const isLoading = ref(false)
  const skillsDir = ref<string>('')
  let isWatching = false
  let unlisten: (() => void) | null = null
  let reloadTimer: ReturnType<typeof setTimeout> | null = null

  async function loadSkills() {
    isLoading.value = true
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      availableSkills.value = await invoke<SkillMetadata[]>('list_skills')
    } catch (error) {
      console.error('Failed to load skills:', error)
      availableSkills.value = []
    } finally {
      isLoading.value = false
    }
  }

  async function getSkill(name: string): Promise<Skill | null> {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      return await invoke<Skill>('get_skill', { name })
    } catch (error) {
      console.error(`Failed to get skill ${name}:`, error)
      return null
    }
  }

  async function createSkill(name: string, description: string, instructions: string): Promise<boolean> {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('create_skill', { name, description, instructions })
      await loadSkills() // Refresh the list
      return true
    } catch (error) {
      console.error('Failed to create skill:', error)
      return false
    }
  }

  async function deleteSkill(name: string): Promise<boolean> {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('delete_skill', { name })
      await loadSkills() // Refresh the list
      return true
    } catch (error) {
      console.error(`Failed to delete skill ${name}:`, error)
      return false
    }
  }

  async function getSkillsDir(): Promise<string> {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      skillsDir.value = await invoke<string>('get_skills_dir')
      return skillsDir.value
    } catch (error) {
      console.error('Failed to get skills dir:', error)
      return ''
    }
  }

  async function startSkillsWatcher() {
    if (isWatching) return
    isWatching = true
    try {
      const { listen } = await import('@tauri-apps/api/event')
      unlisten = await listen('skills-changed', () => {
        if (reloadTimer) {
          clearTimeout(reloadTimer)
        }
        reloadTimer = setTimeout(() => {
          loadSkills()
        }, 250)
      })
    } catch (error) {
      console.error('Failed to watch skills changes:', error)
      isWatching = false
    }
  }

  function stopSkillsWatcher() {
    if (unlisten) {
      unlisten()
      unlisten = null
    }
    if (reloadTimer) {
      clearTimeout(reloadTimer)
      reloadTimer = null
    }
    isWatching = false
  }

  return {
    availableSkills,
    isLoading,
    skillsDir,
    loadSkills,
    getSkill,
    createSkill,
    deleteSkill,
    getSkillsDir,
    startSkillsWatcher,
    stopSkillsWatcher,
  }
})
