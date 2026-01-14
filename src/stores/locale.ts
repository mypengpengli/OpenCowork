import { defineStore } from 'pinia'
import { useStorage } from '@vueuse/core'

export type Locale = 'zh' | 'en'

export const useLocaleStore = defineStore('locale', () => {
  const locale = useStorage<Locale>('opencowork-locale', 'zh')

  if (locale.value !== 'zh' && locale.value !== 'en') {
    locale.value = 'zh'
  }

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
