<script setup lang="ts">
import {
  NConfigProvider,
  NMessageProvider,
  NLayout,
  NLayoutSider,
  NIcon,
  NButton,
  darkTheme,
  zhCN,
  enUS,
  dateZhCN,
  dateEnUS,
} from 'naive-ui'
import { ref, computed } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { SettingsOutline, TimeOutline, LanguageOutline, AddOutline, TrashOutline } from '@vicons/ionicons5'
import { useI18n } from './i18n'
import { useChatStore } from './stores/chat'

const router = useRouter()
const route = useRoute()
const chatStore = useChatStore()

const collapsed = ref(false)
const { locale, t, toggleLocale } = useI18n()

const naiveLocale = computed(() => (locale.value === 'zh' ? zhCN : enUS))
const naiveDateLocale = computed(() => (locale.value === 'zh' ? dateZhCN : dateEnUS))
const localeKey = computed(() => `locale-${locale.value}`)

const activeRoute = computed(() => route.path)

function handleNewChat() {
  const hasMessages = chatStore.messages.some(message => !message.isAlert)
  if (hasMessages) {
    const confirmed = window.confirm(t('main.chat.newConfirm'))
    if (!confirmed) return
  }
  chatStore.newConversation()
  router.push('/')
}

function handleLoadConversation(id: string) {
  if (chatStore.loadConversation(id)) {
    router.push('/')
  }
}

function handleDeleteConversation(id: string) {
  const confirmed = window.confirm(t('sidebar.deleteConfirm'))
  if (!confirmed) return
  chatStore.deleteConversation(id)
}

function goHistory() {
  router.push('/history')
}

function goSettings() {
  router.push('/settings')
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
          :width="240"
          :collapsed="collapsed"
          show-trigger
          @collapse="collapsed = true"
          @expand="collapsed = false"
        >
          <div class="sidebar" :class="{ collapsed: collapsed }">
            <div class="sidebar-top">
              <div class="logo" v-if="!collapsed">
                {{ t('app.name') }}
              </div>
              <div class="logo-mini" v-else>
                {{ t('app.shortName') }}
              </div>
              <NButton
                class="new-chat-button"
                :class="{ collapsed: collapsed }"
                secondary
                :circle="collapsed"
                @click="handleNewChat"
              >
                <template #icon>
                  <NIcon><AddOutline /></NIcon>
                </template>
                <span v-if="!collapsed">{{ t('sidebar.newChat') }}</span>
              </NButton>
            </div>

            <div v-if="!collapsed" class="conversation-section">
              <div class="conversation-header">{{ t('sidebar.conversations') }}</div>
              <div class="conversation-list">
                <div
                  v-if="chatStore.savedConversations.length === 0"
                  class="conversation-empty"
                >
                  {{ t('sidebar.empty') }}
                </div>
                <div v-else class="conversation-items">
                  <div
                    v-for="conversation in chatStore.savedConversations"
                    :key="conversation.id"
                    class="conversation-item"
                    :class="{ active: conversation.id === chatStore.activeConversationId }"
                    @click="handleLoadConversation(conversation.id)"
                  >
                    <span class="conversation-title">{{ conversation.title }}</span>
                    <NButton
                      quaternary
                      size="tiny"
                      class="conversation-delete"
                      :title="t('common.delete')"
                      @click.stop="handleDeleteConversation(conversation.id)"
                    >
                      <template #icon>
                        <NIcon><TrashOutline /></NIcon>
                      </template>
                    </NButton>
                  </div>
                </div>
              </div>
            </div>

            <div class="sidebar-footer">
              <NButton
                quaternary
                :circle="collapsed"
                size="small"
                class="sidebar-action"
                :class="{ active: activeRoute === '/history' }"
                @click="goHistory"
              >
                <template #icon>
                  <NIcon><TimeOutline /></NIcon>
                </template>
                <span v-if="!collapsed">{{ t('menu.history') }}</span>
              </NButton>
              <NButton
                quaternary
                :circle="collapsed"
                size="small"
                class="sidebar-action"
                :class="{ active: activeRoute === '/settings' }"
                @click="goSettings"
              >
                <template #icon>
                  <NIcon><SettingsOutline /></NIcon>
                </template>
                <span v-if="!collapsed">{{ t('menu.settings') }}</span>
              </NButton>
              <NButton
                quaternary
                :circle="collapsed"
                size="small"
                class="sidebar-action"
                @click="toggleLocale"
              >
                <template #icon>
                  <NIcon><LanguageOutline /></NIcon>
                </template>
                <span v-if="!collapsed">{{ locale.value === 'zh' ? 'EN' : '中文' }}</span>
              </NButton>
            </div>
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
}

.logo-mini {
  height: 64px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  font-weight: bold;
  color: #63e2b7;
}

.sidebar {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.sidebar-top {
  padding-bottom: 4px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.09);
}

.new-chat-button {
  margin: 12px;
  width: calc(100% - 24px);
  justify-content: flex-start;
}

.new-chat-button.collapsed {
  width: auto;
  justify-content: center;
}

.conversation-section {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.conversation-header {
  padding: 8px 16px 4px;
  font-size: 12px;
  color: rgba(255, 255, 255, 0.5);
}

.conversation-list {
  flex: 1;
  overflow-y: auto;
  padding: 4px 8px 8px;
}

.conversation-empty {
  padding: 8px 12px;
  color: rgba(255, 255, 255, 0.45);
  font-size: 12px;
}

.conversation-items {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.conversation-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 8px;
  cursor: pointer;
  color: rgba(255, 255, 255, 0.78);
  transition: background 0.15s ease, color 0.15s ease;
}

.conversation-item:hover {
  background: rgba(255, 255, 255, 0.06);
}

.conversation-item.active {
  background: rgba(99, 226, 183, 0.16);
  color: #63e2b7;
}

.conversation-title {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.conversation-delete {
  opacity: 0.65;
}

.conversation-item:hover .conversation-delete {
  opacity: 1;
}

.sidebar-footer {
  margin-top: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px;
  border-top: 1px solid rgba(255, 255, 255, 0.09);
}

.sidebar-action {
  width: 100%;
  justify-content: flex-start;
}

.sidebar-action.active {
  background: rgba(99, 226, 183, 0.12);
  color: #63e2b7;
}

.sidebar.collapsed .sidebar-footer {
  align-items: center;
}

.sidebar.collapsed .sidebar-action {
  width: auto;
  justify-content: center;
}
</style>
