<script setup lang="ts">
import {
  NConfigProvider,
  NMessageProvider,
  NLayout,
  NLayoutSider,
  NMenu,
  NIcon,
  NButton,
  darkTheme,
  zhCN,
  enUS,
  dateZhCN,
  dateEnUS,
} from 'naive-ui'
import { h, ref, computed } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { ChatboxOutline, SettingsOutline, TimeOutline, LanguageOutline } from '@vicons/ionicons5'
import { useI18n } from './i18n'

const router = useRouter()
const route = useRoute()

const collapsed = ref(false)
const { locale, t, toggleLocale } = useI18n()

const menuOptions = computed(() => [
  {
    label: t('menu.chat'),
    key: '/',
    icon: () => h(NIcon, null, { default: () => h(ChatboxOutline) }),
  },
  {
    label: t('menu.history'),
    key: '/history',
    icon: () => h(NIcon, null, { default: () => h(TimeOutline) }),
  },
  {
    label: t('menu.settings'),
    key: '/settings',
    icon: () => h(NIcon, null, { default: () => h(SettingsOutline) }),
  },
])

const naiveLocale = computed(() => (locale.value === 'zh' ? zhCN : enUS))
const naiveDateLocale = computed(() => (locale.value === 'zh' ? dateZhCN : dateEnUS))
const localeKey = computed(() => `locale-${locale.value}`)

const activeKey = computed(() => route.path)

function handleMenuUpdate(key: string) {
  router.push(key)
}
</script>

<template>
  <NConfigProvider :theme="darkTheme" :locale="naiveLocale" :date-locale="naiveDateLocale" :key="localeKey">
    <NMessageProvider>
      <NLayout has-sider style="height: 100vh">
        <NLayoutSider
          bordered
          collapse-mode="width"
          :collapsed-width="64"
          :width="200"
          :collapsed="collapsed"
          show-trigger
          @collapse="collapsed = true"
          @expand="collapsed = false"
        >
          <div class="logo" v-if="!collapsed">
            {{ t('app.name') }}
          </div>
          <div class="logo-mini" v-else>
            {{ t('app.shortName') }}
          </div>
          <NMenu
            :collapsed="collapsed"
            :collapsed-width="64"
            :collapsed-icon-size="22"
            :options="menuOptions"
            :value="activeKey"
            @update:value="handleMenuUpdate"
          />
          <div class="sidebar-footer">
            <NButton
              quaternary
              :circle="collapsed"
              size="small"
              @click="toggleLocale"
            >
              <template #icon>
                <NIcon><LanguageOutline /></NIcon>
              </template>
              <span v-if="!collapsed">{{ locale.value === 'zh' ? 'EN' : '中文' }}</span>
            </NButton>
          </div>
        </NLayoutSider>
        <NLayout>
          <router-view :key="localeKey" />
        </NLayout>
      </NLayout>
    </NMessageProvider>
  </NConfigProvider>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
}

.logo {
  height: 64px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  font-weight: bold;
  color: #63e2b7;
  border-bottom: 1px solid rgba(255, 255, 255, 0.09);
}

.logo-mini {
  height: 64px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  font-weight: bold;
  color: #63e2b7;
  border-bottom: 1px solid rgba(255, 255, 255, 0.09);
}

.sidebar-footer {
  position: absolute;
  bottom: 16px;
  left: 0;
  right: 0;
  display: flex;
  justify-content: center;
  padding: 0 12px;
}
</style>
