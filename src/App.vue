<script setup lang="ts">
import { NConfigProvider, NMessageProvider, NLayout, NLayoutSider, NMenu, NIcon, darkTheme } from 'naive-ui'
import { h, ref, computed } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { ChatboxOutline, SettingsOutline, TimeOutline } from '@vicons/ionicons5'

const router = useRouter()
const route = useRoute()

const collapsed = ref(false)

const menuOptions = [
  {
    label: '对话',
    key: '/',
    icon: () => h(NIcon, null, { default: () => h(ChatboxOutline) }),
  },
  {
    label: '历史',
    key: '/history',
    icon: () => h(NIcon, null, { default: () => h(TimeOutline) }),
  },
  {
    label: '设置',
    key: '/settings',
    icon: () => h(NIcon, null, { default: () => h(SettingsOutline) }),
  },
]

const activeKey = computed(() => route.path)

function handleMenuUpdate(key: string) {
  router.push(key)
}
</script>

<template>
  <NConfigProvider :theme="darkTheme">
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
            Screen Assistant
          </div>
          <div class="logo-mini" v-else>
            SA
          </div>
          <NMenu
            :collapsed="collapsed"
            :collapsed-width="64"
            :collapsed-icon-size="22"
            :options="menuOptions"
            :value="activeKey"
            @update:value="handleMenuUpdate"
          />
        </NLayoutSider>
        <NLayout>
          <router-view />
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
</style>
