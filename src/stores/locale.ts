import { defineStore } from 'pinia'
import { ref } from 'vue'

export type Locale = 'zh' | 'en'

const STORAGE_KEY = 'opencowork-locale'
// 版本标记：当需要重置语言设置时，增加此版本号
const LOCALE_VERSION_KEY = 'opencowork-locale-version'
const CURRENT_LOCALE_VERSION = '5'

export const useLocaleStore = defineStore('locale', () => {
  // 检查是否需要重置语言设置（版本升级时）
  const savedVersion = localStorage.getItem(LOCALE_VERSION_KEY)
  if (savedVersion !== CURRENT_LOCALE_VERSION) {
    // 版本不匹配，清除旧设置
    localStorage.removeItem(STORAGE_KEY)
    localStorage.setItem(LOCALE_VERSION_KEY, CURRENT_LOCALE_VERSION)
  }

  // 读取已保存的设置
  const stored = localStorage.getItem(STORAGE_KEY)

  // 优先使用已保存的设置，否则默认中文
  const locale = ref<Locale>(stored === 'zh' || stored === 'en' ? stored : 'zh')

  // 如果没有保存过，立即保存默认值
  if (stored !== 'zh' && stored !== 'en') {
    localStorage.setItem(STORAGE_KEY, 'zh')
  }

  function setLocale(next: Locale) {
    locale.value = next
    localStorage.setItem(STORAGE_KEY, next)
  }

  function toggleLocale() {
    const next = locale.value === 'zh' ? 'en' : 'zh'
    locale.value = next
    localStorage.setItem(STORAGE_KEY, next)
  }

  return {
    locale,
    setLocale,
    toggleLocale,
  }
})
