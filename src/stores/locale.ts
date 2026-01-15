import { defineStore } from 'pinia'
import { useStorage } from '@vueuse/core'

export type Locale = 'zh' | 'en'

export const useLocaleStore = defineStore('locale', () => {
  const detectSystemLocale = (): Locale => {
    if (typeof navigator !== 'undefined') {
      const lang = (navigator.languages && navigator.languages[0]) || navigator.language || ''
      if (lang.toLowerCase().startsWith('zh')) {
        return 'zh'
      }
    }
    return 'en'
  }

  const systemLocale = detectSystemLocale()
  const locale = useStorage<Locale>('opencowork-locale', systemLocale)

  if (locale.value !== 'zh' && locale.value !== 'en') {
    locale.value = systemLocale
  }
  locale.value = systemLocale

  function setLocale(next: Locale) {
    locale.value = next
  }

  function toggleLocale() {
    locale.value = locale.value === 'zh' ? 'en' : 'zh'
  }

  return {
    locale,
    setLocale,
    toggleLocale,
  }
})
