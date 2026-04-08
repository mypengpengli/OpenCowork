function loadPaneState() {
  try {
    const raw = JSON.parse(localStorage.getItem('opencowork-shell-pane-state') || '{}')
    return {
      overview: true,
      utility: true,
      workspace: raw.workspace !== undefined ? Boolean(raw.workspace) : true,
      activity: raw.activity !== undefined ? Boolean(raw.activity) : true,
    }
  } catch {
    return {
      overview: true,
      utility: true,
      workspace: true,
      activity: true,
    }
  }
}

const state = {
  bootstrap: null,
  locale: localStorage.getItem('opencowork-shell-locale') === 'en' ? 'en' : 'zh',
  currentView: 'chat',
  currentSessionId: null,
  currentSession: null,
  sending: false,
  slashCatalog: {
    loaded: false,
    loading: false,
    commands: [],
    tools: [],
  },
  slashMenu: {
    open: false,
    query: '',
    items: [],
    activeIndex: 0,
  },
  pendingTurn: null,
  pendingStreamToken: 0,
  sessionFilter: '',
  historyFilter: '',
  lastEvents: [],
  expandedBlocks: new Set(),
  sidebarCollapsed: false,
  settingsTab: 'provider',
  drawerMode: 'provider',
  providerApiKeyVisible: false,
  selectedProviderProfileId: null,
  selectedSkillSlug: null,
  selectedSkillDetail: null,
  selectedMcpName: null,
  skillFilter: '',
  skillProjectOnly: false,
  mcpFilter: '',
  paneState: loadPaneState(),
  forceMessageScroll: false,
  blockViewer: {
    title: '',
    content: '',
    meta: '',
  },
  turnStats: {
    iterations: null,
    estimatedPromptTokens: null,
    compacted: null,
  },
}

const els = {
  body: document.body,
  sidebar: document.querySelector('#sidebar'),
  sidebarToggleButton: document.querySelector('#sidebar-toggle-button'),
  newSessionButton: document.querySelector('#new-session-button'),
  reloadSessionsButton: document.querySelector('#reload-sessions-button'),
  localeToggleButton: document.querySelector('#locale-toggle-button'),
  localeToggleIcon: document.querySelector('#locale-toggle-icon'),
  localeToggleLabel: document.querySelector('#locale-toggle-label'),
  sessionSearch: document.querySelector('#session-search'),
  sessionList: document.querySelector('#session-list'),
  navButtons: Array.from(document.querySelectorAll('.sidebar-nav-button')),
  chatView: document.querySelector('#chat-view'),
  historyView: document.querySelector('#history-view'),
  settingsView: document.querySelector('#settings-view'),
  runtimeModel: document.querySelector('#runtime-model'),
  runtimePermission: document.querySelector('#runtime-permission'),
  providerPersisted: document.querySelector('#provider-persisted'),
  teamMemoryState: document.querySelector('#team-memory-state'),
  sessionUpdatedChip: document.querySelector('#session-updated-chip'),
  sessionOverviewLaunchButton: document.querySelector('#session-overview-launch-button'),
  utilityLaunchButton: document.querySelector('#utility-launch-button'),
  sessionBanner: document.querySelector('#session-banner'),
  sessionOverviewDrawer: document.querySelector('#session-overview-drawer'),
  sessionOverviewContent: document.querySelector('#session-overview-content'),
  sessionOverviewToggle: document.querySelector('#session-overview-toggle'),
  sessionOverviewMeta: document.querySelector('#session-overview-meta'),
  metricsGrid: document.querySelector('#metrics-grid'),
  messageToolbar: document.querySelector('#message-toolbar'),
  chatEmptyState: document.querySelector('#chat-empty-state'),
  chatTitle: document.querySelector('#chat-title'),
  chatSubtitle: document.querySelector('#chat-subtitle'),
  currentSessionChip: document.querySelector('#current-session-chip'),
  sessionCopyIdButton: document.querySelector('#session-copy-id-button'),
  sessionNewButton: document.querySelector('#session-new-button'),
  summaryMessages: document.querySelector('#summary-messages'),
  summaryTools: document.querySelector('#summary-tools'),
  summaryTokens: document.querySelector('#summary-tokens'),
  summaryMemory: document.querySelector('#summary-memory'),
  messageCount: document.querySelector('#message-count'),
  expandAllButton: document.querySelector('#expand-all-button'),
  messageList: document.querySelector('#message-list'),
  workspaceCwd: document.querySelector('#workspace-cwd'),
  workspaceSettings: document.querySelector('#workspace-settings'),
  teamMemoryDetail: document.querySelector('#team-memory-detail'),
  sessionMeta: document.querySelector('#session-meta'),
  workspacePaneCard: document.querySelector('#workspace-pane-card'),
  workspacePaneContent: document.querySelector('#workspace-pane-content'),
  workspacePaneToggle: document.querySelector('#workspace-pane-toggle'),
  eventCount: document.querySelector('#event-count'),
  turnIterations: document.querySelector('#turn-iterations'),
  turnPromptTokens: document.querySelector('#turn-prompt-tokens'),
  turnCompacted: document.querySelector('#turn-compacted'),
  turnEventTotal: document.querySelector('#turn-event-total'),
  eventList: document.querySelector('#event-list'),
  utilityDrawer: document.querySelector('#utility-drawer'),
  utilityDrawerContent: document.querySelector('#utility-drawer-content'),
  utilityDrawerToggle: document.querySelector('#utility-drawer-toggle'),
  utilityDrawerMeta: document.querySelector('#utility-drawer-meta'),
  activityPaneCard: document.querySelector('#activity-pane-card'),
  activityPaneContent: document.querySelector('#activity-pane-content'),
  activityPaneToggle: document.querySelector('#activity-pane-toggle'),
  composerForm: document.querySelector('#composer-form'),
  slashMenu: document.querySelector('#slash-menu'),
  slashMenuList: document.querySelector('#slash-menu-list'),
  slashMenuMeta: document.querySelector('#slash-menu-meta'),
  composerInput: document.querySelector('#composer-input'),
  clearInputButton: document.querySelector('#clear-input-button'),
  composerStatus: document.querySelector('#composer-status'),
  sendButton: document.querySelector('#send-button'),
  historyRefreshButton: document.querySelector('#history-refresh-button'),
  historyNewSessionButton: document.querySelector('#history-new-session-button'),
  historySearch: document.querySelector('#history-search'),
  historyCountChip: document.querySelector('#history-count-chip'),
  historyList: document.querySelector('#history-list'),
  settingsWorkspaceChip: document.querySelector('#settings-workspace-chip'),
  headerNewSkillButton: document.querySelector('#header-new-skill-button'),
  headerNewMcpButton: document.querySelector('#header-new-mcp-button'),
  settingsTabs: Array.from(document.querySelectorAll('.settings-tab')),
  settingsProviderPanel: document.querySelector('#settings-provider-panel'),
  settingsPermissionPanel: document.querySelector('#settings-permission-panel'),
  settingsEnvironmentPanel: document.querySelector('#settings-environment-panel'),
  permissionCard: document.querySelector('#permission-card'),
  runtimeCard: document.querySelector('#runtime-card'),
  localeCard: document.querySelector('#locale-card'),
  pathsCard: document.querySelector('#paths-card'),
  providerProfileHeader: document.querySelector('#provider-profile-header'),
  overviewProviderCard: document.querySelector('#overview-provider-card'),
  overviewProviderValue: document.querySelector('#overview-provider-value'),
  overviewProviderMeta: document.querySelector('#overview-provider-meta'),
  overviewSkillsCard: document.querySelector('#overview-skills-card'),
  overviewSkillsValue: document.querySelector('#overview-skills-value'),
  overviewSkillsMeta: document.querySelector('#overview-skills-meta'),
  overviewMcpCard: document.querySelector('#overview-mcp-card'),
  overviewMcpValue: document.querySelector('#overview-mcp-value'),
  overviewMcpMeta: document.querySelector('#overview-mcp-meta'),
  overviewLocaleCard: document.querySelector('#overview-locale-card'),
  overviewLocaleValue: document.querySelector('#overview-locale-value'),
  overviewLocaleMeta: document.querySelector('#overview-locale-meta'),
  permissionSummaryValue: document.querySelector('#permission-summary-value'),
  permissionModeSelect: document.querySelector('#permission-mode-select'),
  savePermissionButton: document.querySelector('#save-permission-button'),
  providerProfileCountChip: document.querySelector('#provider-profile-count-chip'),
  providerProfileList: document.querySelector('#provider-profile-list'),
  newProviderProfileButton: document.querySelector('#new-provider-profile-button'),
  settingsSkillsPanel: document.querySelector('#settings-skills-panel'),
  settingsMcpPanel: document.querySelector('#settings-mcp-panel'),
  openProviderDrawerButton: document.querySelector('#open-provider-drawer-button'),
  localeCardButton: document.querySelector('#locale-card-button'),
  localeCurrentValue: document.querySelector('#locale-current-value'),
  providerSummaryPermission: document.querySelector('#provider-summary-permission'),
  providerSummaryPersisted: document.querySelector('#provider-summary-persisted'),
  providerSummarySessionCount: document.querySelector('#provider-summary-session-count'),
  providerSummarySkillCount: document.querySelector('#provider-summary-skill-count'),
  settingsFilePath: document.querySelector('#settings-file-path'),
  skillsDirPath: document.querySelector('#skills-dir-path'),
  copySettingsPathButton: document.querySelector('#copy-settings-path-button'),
  copySkillsDirButton: document.querySelector('#copy-skills-dir-button'),
  settingsNewSkillButton: document.querySelector('#settings-new-skill-button'),
  skillSearch: document.querySelector('#skill-search'),
  skillProjectOnly: document.querySelector('#skill-project-only'),
  skillCountChip: document.querySelector('#skill-count-chip'),
  skillList: document.querySelector('#skill-list'),
  mcpSearch: document.querySelector('#mcp-search'),
  mcpCountChip: document.querySelector('#mcp-count-chip'),
  mcpList: document.querySelector('#mcp-list'),
  newSkillButton: document.querySelector('#new-skill-button'),
  newMcpButton: document.querySelector('#new-mcp-button'),
  drawerOverlay: document.querySelector('#drawer-overlay'),
  settingsDrawer: document.querySelector('#settings-drawer'),
  closeDrawerButton: document.querySelector('#close-drawer-button'),
  examplePills: Array.from(document.querySelectorAll('.example-pill')),
  drawerTitle: document.querySelector('#drawer-title'),
  drawerStatus: document.querySelector('#drawer-status'),
  drawerProviderPanel: document.querySelector('#drawer-provider-panel'),
  drawerSkillPanel: document.querySelector('#drawer-skill-panel'),
  drawerMcpPanel: document.querySelector('#drawer-mcp-panel'),
  providerProfileState: document.querySelector('#provider-profile-state'),
  providerFormNote: document.querySelector('#provider-form-note'),
  providerForm: document.querySelector('#provider-form'),
  providerProfileLabel: document.querySelector('#provider-profile-label'),
  providerModel: document.querySelector('#provider-model'),
  providerName: document.querySelector('#provider-name'),
  providerApiKey: document.querySelector('#provider-api-key'),
  providerApiKeyVisibilityButton: document.querySelector('#provider-api-key-visibility-button'),
  providerApiKeyStatus: document.querySelector('#provider-api-key-status'),
  providerClearApiKeyRow: document.querySelector('#provider-clear-api-key-row'),
  providerClearApiKey: document.querySelector('#provider-clear-api-key'),
  providerBaseUrl: document.querySelector('#provider-base-url'),
  providerBaseUrlEnv: document.querySelector('#provider-base-url-env'),
  providerTimeoutMs: document.querySelector('#provider-timeout-ms'),
  providerProfileActivate: document.querySelector('#provider-profile-activate'),
  resetProviderButton: document.querySelector('#reset-provider-button'),
  deleteProviderProfileButton: document.querySelector('#delete-provider-profile-button'),
  skillEditorState: document.querySelector('#skill-editor-state'),
  skillForm: document.querySelector('#skill-form'),
  skillFormNote: document.querySelector('#skill-form-note'),
  skillTemplate: document.querySelector('#skill-template'),
  applySkillTemplateButton: document.querySelector('#apply-skill-template-button'),
  skillName: document.querySelector('#skill-name'),
  skillDescription: document.querySelector('#skill-description'),
  skillWhen: document.querySelector('#skill-when'),
  skillArgumentHint: document.querySelector('#skill-argument-hint'),
  skillTools: document.querySelector('#skill-tools'),
  skillPaths: document.querySelector('#skill-paths'),
  skillContext: document.querySelector('#skill-context'),
  skillVersion: document.querySelector('#skill-version'),
  skillAgent: document.querySelector('#skill-agent'),
  skillModel: document.querySelector('#skill-model'),
  skillEffort: document.querySelector('#skill-effort'),
  skillContent: document.querySelector('#skill-content'),
  resetSkillButton: document.querySelector('#reset-skill-button'),
  deleteSkillButton: document.querySelector('#delete-skill-button'),
  mcpEditorState: document.querySelector('#mcp-editor-state'),
  mcpTransportHint: document.querySelector('#mcp-transport-hint'),
  mcpAuthHint: document.querySelector('#mcp-auth-hint'),
  mcpPreset: document.querySelector('#mcp-preset'),
  applyMcpPresetButton: document.querySelector('#apply-mcp-preset-button'),
  mcpForm: document.querySelector('#mcp-form'),
  mcpName: document.querySelector('#mcp-name'),
  mcpTransport: document.querySelector('#mcp-transport'),
  mcpCommandGroup: document.querySelector('#mcp-command-group'),
  mcpCommand: document.querySelector('#mcp-command'),
  mcpArgsGroup: document.querySelector('#mcp-args-group'),
  mcpArgs: document.querySelector('#mcp-args'),
  mcpEndpointGroup: document.querySelector('#mcp-endpoint-group'),
  mcpEndpoint: document.querySelector('#mcp-endpoint'),
  mcpTimeoutMs: document.querySelector('#mcp-timeout-ms'),
  mcpAuthType: document.querySelector('#mcp-auth-type'),
  mcpTokenEnvGroup: document.querySelector('#mcp-token-env-group'),
  mcpTokenEnv: document.querySelector('#mcp-token-env'),
  mcpTokenPathGroup: document.querySelector('#mcp-token-path-group'),
  mcpTokenPath: document.querySelector('#mcp-token-path'),
  resetMcpButton: document.querySelector('#reset-mcp-button'),
  deleteMcpButton: document.querySelector('#delete-mcp-button'),
  blockViewerOverlay: document.querySelector('#block-viewer-overlay'),
  blockViewer: document.querySelector('#block-viewer'),
  blockViewerTitle: document.querySelector('#block-viewer-title'),
  blockViewerMeta: document.querySelector('#block-viewer-meta'),
  blockViewerContent: document.querySelector('#block-viewer-content'),
  blockViewerCopyButton: document.querySelector('#block-viewer-copy-button'),
  blockViewerCloseButton: document.querySelector('#block-viewer-close-button'),
}

const MESSAGES = {
  zh: {
    'slash.title': '斜杠命令',
    'slash.loading': '正在加载命令、工具和技能…',
    'slash.empty': '没有匹配的命令、工具、Skill 或 MCP。',
    'slash.section.commands': '命令',
    'slash.section.actions': '快捷操作',
    'slash.section.skills': 'Skills',
    'slash.section.mcp': 'MCP',
    'slash.section.tools': '工具',
    'slash.action.new': '/new',
    'slash.action.newSummary': '新建一个空白会话。',
    'slash.action.history': '/history',
    'slash.action.historySummary': '打开会话历史页。',
    'slash.action.settings': '/settings',
    'slash.action.settingsSummary': '打开设置页。',
    'slash.action.provider': '/provider',
    'slash.action.providerSummary': '打开 Provider 设置。',
    'slash.action.skills': '/skills',
    'slash.action.skillsSummary': '浏览和编辑 Skills。',
    'slash.action.mcp': '/mcp',
    'slash.action.mcpSummary': '浏览和编辑 MCP 服务。',
    'slash.action.tools': '/tools',
    'slash.action.toolsSummary': '查看当前可用工具清单。',
    'slash.action.skill': '/skill {{name}}',
    'slash.action.skillSummary': '查看 Skill 详情并打开编辑器。',
    'slash.action.mcpServer': '/mcp {{name}}',
    'slash.action.mcpServerSummary': '查看 MCP 服务详情。',
    'slash.action.tool': '/tool {{name}}',
    'slash.action.toolSummary': '查看工具说明和权限。',
    'slash.executed': '已执行 {{command}}。',
    'slash.unknown': '未知斜杠命令。',
    'session.drawerKicker': '会话概览',
    'session.drawerTitle': '当前会话摘要',
    'utility.kicker': '运行辅助',
    'utility.title': '工作区与活动',
    'composer.sending': '发送中…',
    'composer.waiting': '等待响应…',
    'composer.streaming': '输出中…',
    'message.pendingUser': '刚刚发送',
    'message.waitingResponse': '等待助手响应…',
    'app.title': 'OpenClaw',
    'brand.kicker': 'OpenCoWork 智能体',
    'sidebar.newSession': '新建会话',
    'sidebar.conversations': '会话列表',
    'sidebar.refresh': '刷新会话',
    'sidebar.search': '搜索会话',
    'sidebar.searchPlaceholder': '搜索会话编号',
    'sidebar.noMatch': '没有匹配当前筛选条件的会话。',
    'sidebar.empty': '还没有已保存的会话。',
    'menu.chat': '对话',
    'menu.history': '会话',
    'menu.settings': '设置',
    'locale.switchToEnglish': 'English',
    'locale.switchToChinese': '中文',
    'locale.current.zh': '中文',
    'locale.current.en': 'English',
    'empty.title': 'OpenClaw',
    'empty.copy': '在不改后端运行时的前提下，用更清爽的工作台外壳继续处理上下文、记忆、工具和会话。',
    'empty.example1': '查看当前工作区，并总结运行时状态。',
    'empty.example2': '调整 API / Provider 设置，但不要改主循环。',
    'empty.example3': '为当前项目创建或更新本地 Skill。',
    'session.kicker': '当前会话',
    'session.defaultTitle': 'OpenClaw 会话',
    'session.defaultSubtitle': '从侧边栏加载已有会话，或者新建一个继续工作。',
    'session.empty': '暂无活动会话',
    'session.waiting': '等待第一轮输入',
    'session.loaded': '已从当前状态加载',
    'session.updated': '更新于 {{time}}',
    'session.title': 'OpenClaw / {{id}}',
    'session.subtitle': '继续处理当前后端会话，共有 {{count}} 条已存消息。',
    'session.copyId': '复制会话号',
    'session.new': '新建空白会话',
    'metric.messages': '消息数',
    'metric.tools': '工具块',
    'metric.tokens': '会话 Tokens',
    'metric.memory': '记忆',
    'metric.loaded': '已加载',
    'metric.notLoaded': '未加载',
    'conversation.title': '对话',
    'conversation.expandAll': '全部展开',
    'conversation.collapseAll': '全部收起',
    'common.expand': '展开',
    'common.collapse': '收起',
    'common.edit': '编辑',
    'common.inspect': '查看',
    'common.delete': '删除',
    'common.reset': '重置',
    'common.refresh': '刷新',
    'common.close': '关闭',
    'common.copy': '复制',
    'common.yes': '是',
    'common.no': '否',
    'common.updated': '更新时间',
    'common.status': '状态',
    'common.active': '当前',
    'common.saved': '已保存',
    'common.draft': '草稿',
    'workspace.title': '工作区',
    'workspace.path': '路径',
    'workspace.settings': '配置',
    'workspace.teamSync': '团队记忆同步',
    'workspace.visible': '{{count}} 个可见',
    'activity.title': '本轮活动',
    'activity.iterations': '迭代次数',
    'activity.prompt': '提示词预算',
    'activity.compacted': '是否压缩',
    'activity.events': '事件数',
    'activity.empty': '当前还没有活动事件。',
    'composer.placeholder': '继续使用当前运行时。Enter 发送，Ctrl + Enter 换行。',
    'composer.ready': '就绪。',
    'composer.send': '发送',
    'composer.clear': '清空输入',
    'composer.shortcut': 'Enter 发送，Ctrl + Enter 换行。',
    'composer.empty': '请先输入内容。',
    'composer.calling': '正在调用当前运行时...',
    'composer.done': '完成。本轮 {{iterations}} 次迭代，提示词估算 {{tokens}}。',
    'composer.sendFailed': '发送失败。',
    'composer.newSessionReady': '新会话已准备好。',
    'composer.deletedSession': '已删除会话 {{id}}。',
    'composer.messageCopied': '消息已复制。',
    'composer.blockCopied': '代码块已复制。',
    'composer.sessionIdCopied': '会话号已复制。',
    'composer.pathCopied': '路径已复制。',
    'composer.copyFailed': '复制失败。',
    'composer.exampleApplied': '示例内容已填入输入框。',
    'composer.refreshDone': '会话列表已刷新。',
    'composer.refreshFailed': '刷新失败。',
    'history.kicker': '会话记录',
    'history.title': '已保存会话',
    'history.empty': '还没有已保存会话。开始一次对话后，这里会自动出现。',
    'history.open': '打开',
    'history.copyId': '复制 ID',
    'history.messages': '{{count}} 条消息',
    'history.searchPlaceholder': '搜索会话号',
    'history.count': '{{visible}} / {{total}}',
    'settings.kicker': '配置中心',
    'settings.title': '设置',
    'settings.copy': '保留当前运行时逻辑不变，只在外壳层处理界面、配置录入和双语体验。',
    'settings.editApi': '编辑 API',
    'settings.pathsTitle': '项目路径',
    'settings.pathsCopy': '把最常用的配置和 Skills 路径直接放到这里，避免来回找文件。',
    'settings.configPath': '配置文件',
    'settings.skillsPath': 'Skills 目录',
    'settings.copyConfigPath': '复制配置路径',
    'settings.copySkillsDir': '复制 Skills 路径',
    'settings.newSkillShortcut': '新建 Skill',
    'settings.noPath': '暂无路径',
    'settings.tab.provider': 'Provider',
    'settings.tab.permission': '权限',
    'settings.tab.skills': 'Skills',
    'settings.tab.mcp': 'MCP',
    'settings.tab.environment': '环境',
    'settings.overviewSkillsMeta': '项目 {{project}} / 总计 {{total}}',
    'settings.overviewMcpMeta': '已配置 {{count}} 个服务',
    'settings.overviewLocaleMeta': '点击切换语言',
    'permission.title': '工具与编辑权限',
    'permission.copy': '控制工具调用和文件编辑时的权限策略。默认推荐完全允许，减少交互打断。',
    'permission.current': '当前模式',
    'permission.save': '保存权限模式',
    'permission.saved': '权限模式已保存。',
    'permission.mode.danger': '完全允许',
    'permission.mode.workspace': '运行前确认写入',
    'permission.mode.readonly': '只读',
    'provider.title': '当前 Provider',
    'provider.copy': '当前启用的 Provider 档位。真正的新增、管理和启用在下方配置档列表里完成。',
    'provider.label': 'Provider 标签',
    'provider.apiKey': 'API Key',
    'provider.apiKeyReveal': '显示',
    'provider.apiKeyHide': '隐藏',
    'provider.apiKeyClear': '保存时清空当前 API Key',
    'provider.apiKeyStatusExisting': '已保存 API Key；留空会保持原值。',
    'provider.apiKeyStatusUpdating': '保存后会覆盖当前 API Key。',
    'provider.apiKeyStatusClearing': '保存后会清空当前 API Key。',
    'provider.apiKeyStatusNew': '保存后会写入新的 API Key。',
    'provider.baseUrlEnv': 'Base URL 环境变量',
    'provider.save': '保存 Provider',
    'provider.defaults': '使用默认值',
    'provider.saved': 'Provider 设置已保存。',
    'provider.reset': 'Provider 已恢复默认设置。',
    'provider.defaultState': '使用默认值',
    'provider.savedState': '已保存到项目',
    'provider.formNote': '模型、API Key、Base URL 和超时会随当前 Provider 档一起保存。',
    'provider.formNoteSaved': '当前 Provider 已保存到项目配置；修改后会继续覆盖项目级设置。',
    'provider.formNoteDefault': '当前 Provider 仍在使用默认值；保存后才会落到项目配置。',
    'provider.modelPlaceholder': 'claude-sonnet-4-5',
    'provider.namePlaceholder': 'openai-compatible',
    'provider.apiKeyPlaceholder': 'sk-...',
    'provider.baseUrlPlaceholder': 'https://api.openai.com/v1',
    'provider.baseUrlEnvPlaceholder': 'OPENAI_BASE_URL',
    'provider.modelHint': '直接用于当前壳层发起的会话请求，不改后端主循环默认值。',
    'provider.nameHint': '用来区分供应商或路由，不会改底层运行时结构。',
    'provider.apiKeyHint': '直接填入可用的 API Key；壳层会在请求前自动接到运行环境里。',
    'provider.baseUrlHint': '填写兼容接口根地址；如果走官方默认地址，可保留默认值。',
    'provider.baseUrlEnvHint': '如果你会在不同环境之间切换地址，可以只在这里保存变量名。',
    'provider.timeoutHint': '请求超时只影响这层壳子的请求等待时间。',
    'providerProfiles.title': 'Provider 配置档',
    'providerProfiles.copy': '可以保存多个 API 提供商配置档，并选择一个作为当前启用项。',
    'providerProfiles.new': '新建 Provider 档',
    'providerProfiles.count': '{{visible}} / {{total}}',
    'providerProfiles.label': '配置档名称',
    'providerProfiles.labelPlaceholder': 'OpenAI 默认',
    'providerProfiles.labelHint': '用来区分不同 API 提供商和环境，推荐写清用途。',
    'providerProfiles.activate': '保存后设为当前启用',
    'providerProfiles.save': '保存配置档',
    'providerProfiles.saved': 'Provider 配置档已保存。',
    'providerProfiles.created': '创建新的 Provider 配置档。',
    'providerProfiles.editing': '正在编辑 {{name}}。',
    'providerProfiles.active': '当前启用',
    'providerProfiles.activateButton': '启用',
    'providerProfiles.activated': 'Provider 配置档已启用。',
    'providerProfiles.deleted': 'Provider 配置档已删除。',
    'providerProfiles.deleteConfirm': '确定删除 Provider 配置档 {{name}} 吗？',
    'providerProfiles.empty': '还没有 Provider 配置档。先创建一个可切换的 API 配置。',
    'providerProfiles.runtime': '配置档',
    'runtime.title': '运行时快照',
    'runtime.copy': '当前外壳环境的只读概览。',
    'locale.title': '界面语言',
    'locale.copy': '默认中文，可在中英文之间快速切换。',
    'locale.switch': '切换语言',
    'locale.current': '当前语言',
    'skills.title': 'Skill 库',
    'skills.copy': '项目级 Skill 可编辑，来自其他根目录的 Skill 保持只读。',
    'skills.new': '新建 Skill',
    'skills.empty': '还没有 Skill。你可以先创建一个项目级 Skill。',
    'skills.searchPlaceholder': '搜索 Skill 名称或描述',
    'skills.projectOnly': '只看项目级 Skill',
    'skills.count': '{{visible}} / {{total}}',
    'skills.noMatch': '当前筛选条件下没有匹配的 Skill。',
    'skills.none': '没有说明。',
    'skills.project': '项目级',
    'skills.readonly': '只读',
    'skills.created': '创建新的项目 Skill。',
    'skills.editing': '正在编辑 {{name}}。',
    'skills.readonlyNote': '{{name}} 来自其他目录，这里只能查看，不能直接改写。',
    'skills.reset': 'Skill 表单已重置。',
    'skills.saved': 'Skill 已保存。',
    'skills.deleted': 'Skill 已删除。',
    'skills.deleteConfirm': '确定删除 Skill {{name}} 吗？',
    'skill.formNote': 'Skill 内容会写入项目目录下的 `.opencowork/skills`，路径和允许工具会影响自动激活。',
    'skill.formNoteReadonly': '当前 Skill 来自外部目录，只能查看，不能直接在这个壳层里写回。',
    'skill.template': '内容模板',
    'skill.template.blank': '空白',
    'skill.template.basic': '基础技能',
    'skill.template.file': '文件处理',
    'skill.template.review': '代码审查',
    'skill.templateApply': '填入模板',
    'skill.templateConfirm': '当前内容不为空，确定用模板覆盖吗？',
    'skill.templateApplied': '模板已填入表单。',
    'skill.namePlaceholder': 'review-rust-runtime',
    'skill.nameHint': '名称会影响 slug 和文件夹名，尽量保持稳定。',
    'skill.whenHint': '这里写清楚触发时机，后续给模型展示的 listing 会优先用这段。',
    'skill.toolsHint': '逗号分隔。这里只限制 skill 暴露给子流程的工具面。',
    'skill.pathsHint': '命中文件路径后，skill 会在同一轮或下一轮自动回注到上下文。',
    'skill.contextHint': '只有需要和主会话隔离时才用 fork。',
    'skill.bodyHint': '这里保存真正写进 SKILL.md 的内容，可以直接写步骤、规则和边界。',
    'mcp.title': 'MCP 服务',
    'mcp.copy': '管理壳层侧的 MCP 配置，不改运行时内部逻辑。',
    'mcp.new': '新建服务',
    'mcp.searchPlaceholder': '搜索 MCP 名称、命令或地址',
    'mcp.count': '{{visible}} / {{total}}',
    'mcp.noMatch': '当前筛选条件下没有匹配的 MCP 服务。',
    'mcp.transport': '传输方式',
    'mcp.command': '命令',
    'mcp.args': '参数',
    'mcp.endpoint': '地址',
    'mcp.auth': '认证',
    'mcp.auth.none': '无',
    'mcp.auth.env': 'Bearer 环境变量',
    'mcp.auth.file': 'Bearer 文件',
    'mcp.tokenEnv': 'Token 环境变量',
    'mcp.tokenPath': 'Token 文件路径',
    'mcp.save': '保存 MCP',
    'mcp.empty': '还没有 MCP 服务。你可以先创建一个。',
    'mcp.none': '没有命令或地址。',
    'mcp.created': '创建新的 MCP 服务项。',
    'mcp.editing': '正在编辑 {{name}}。',
    'mcp.reset': 'MCP 表单已重置。',
    'mcp.saved': 'MCP 已保存。',
    'mcp.deleted': 'MCP 服务已删除。',
    'mcp.deleteConfirm': '确定删除 MCP 服务 {{name}} 吗？',
    'mcp.preset': '快速预设',
    'mcp.preset.blank': '空白',
    'mcp.preset.stdioNode': '本地 Node stdio',
    'mcp.preset.httpBearer': '远端 HTTP + Bearer',
    'mcp.preset.sseReadonly': '远端 SSE 只读',
    'mcp.presetApply': '应用预设',
    'mcp.presetConfirm': '当前 MCP 表单已经有内容，确定用预设覆盖吗？',
    'mcp.presetApplied': 'MCP 预设已应用。',
    'mcp.namePlaceholder': 'filesystem',
    'mcp.nameHint': '建议和服务职责一致，后续工具名会以它作为前缀或分组提示。',
    'mcp.commandHint': 'stdio 模式下填写启动命令，参数单独填在下方。',
    'mcp.argsHint': '使用逗号分隔，保存时会拆成参数数组。',
    'mcp.endpointHint': 'http / sse / ws 都走这里，填写完整服务地址。',
    'mcp.timeoutHint': '超时主要用于工具发现和调用等待，不改底层 transport 逻辑。',
    'mcp.authModeHint': '只有需要壳层代带 Bearer Token 时再配置认证方式。',
    'mcp.transportHint.stdio': 'stdio 模式使用本地命令启动服务，通常需要填写命令和参数。',
    'mcp.transportHint.network': '网络模式直接连接远端地址，通常只需要填写 Endpoint。',
    'mcp.authHint.none': '当前不附带认证信息。',
    'mcp.authHint.env': '请求时会从环境变量读取 Bearer Token。',
    'mcp.authHint.file': '请求时会从本地文件读取 Bearer Token。',
    'drawer.kicker': '编辑器',
    'drawer.ready': '就绪。',
    'drawer.editProvider': '编辑 Provider',
    'drawer.createSkill': '创建 Skill',
    'drawer.editSkill': '编辑 {{name}}',
    'drawer.createMcp': '创建 MCP 服务',
    'drawer.editMcp': '编辑 {{name}}',
    'field.model': '模型',
    'field.name': '名称',
    'field.baseUrl': 'Base URL',
    'field.timeout': '超时',
    'field.timeoutMs': '超时毫秒',
    'field.permission': '权限模式',
    'field.persisted': '持久化',
    'field.sessions': '会话数',
    'field.skills': '技能数',
    'field.description': '描述',
    'skill.when': '适用场景',
    'skill.argumentHint': '参数提示',
    'skill.allowedTools': '允许工具',
    'skill.paths': '触发路径',
    'skill.context': '执行上下文',
    'skill.version': '版本',
    'skill.agent': 'Agent',
    'skill.effort': '推理强度',
    'skill.body': '内容',
    'skill.save': '保存 Skill',
    'context.default': '默认',
    'context.current': '当前',
    'context.fork': '分叉',
    'teamMemory.notConfigured': '未配置',
    'teamMemory.error': '错误',
    'teamMemory.running': '运行中',
    'teamMemory.syncing': '同步中',
    'teamMemory.configured': '已配置',
    'teamMemory.none': '暂无活动',
    'teamMemory.stats': '拉取 {{pulled}} / 推送 {{pushed}}',
    'event.assistantOutput': '模型输出',
    'event.toolCall': '工具调用',
    'event.toolResult': '工具结果',
    'event.tokenUsage': 'Token 使用',
    'event.messageStop': '消息结束',
    'event.messageStopBody': '当前这轮输出已经结束。',
    'event.usage': '输入 {{input}} / 输出 {{output}} / 缓存读取 {{cacheRead}} / 缓存写入 {{cacheCreate}}',
    'message.user': '用户',
    'message.assistant': '助手',
    'message.system': '系统',
    'message.copy': '复制',
    'message.copyBlock': '复制块',
    'message.blocks': '{{count}} 个块',
    'message.chars': '{{count}} 字符',
    'message.lines': '{{count}} 行',
    'message.deleteSession': '删除会话',
    'message.inspectBlock': '查看',
    'message.viewerKicker': '块查看器',
    'message.viewerMeta': '{{type}} · {{chars}} 字符 · {{lines}} 行',
    'message.success': '成功',
    'message.failed': '失败',
    'message.summaryPath': '路径',
    'message.summaryCommand': '命令',
    'message.summaryQuery': '查询',
    'message.summaryPattern': '模式',
    'message.summaryEndpoint': '地址',
    'message.summaryCount': '数量',
    'message.summaryPreview': '预览',
    'message.summaryStored': '已落盘',
    'message.summaryToolInput': '工具输入参数',
    'message.summaryToolOutput': '工具结果预览',
    'confirm.deleteSession': '确定删除会话 {{id}} 吗？',
    'status.permissionFallback': '权限',
    'status.modelFallback': '模型',
    'status.providerSaved': 'provider: 已保存',
    'status.providerDefault': 'provider: 默认',
    'errors.providerNameRequired': 'Provider 名称不能为空。',
    'errors.providerProfileLabelRequired': 'Provider 配置档名称不能为空。',
    'errors.providerModelRequired': 'Provider 模型不能为空。',
    'errors.providerApiKeyEnvRequired': 'API Key 环境变量不能为空。',
    'errors.providerBaseUrlRequired': 'Base URL 不能为空。',
    'errors.skillNameRequired': 'Skill 名称不能为空。',
    'errors.skillContentRequired': 'Skill 内容不能为空。',
    'errors.mcpNameRequired': 'MCP 名称不能为空。',
    'errors.mcpCommandRequired': 'stdio 模式下必须填写命令。',
    'errors.mcpEndpointRequired': '当前传输方式必须填写 Endpoint。',
    'errors.mcpTokenEnvRequired': '当前认证方式必须填写 Token 环境变量。',
    'errors.mcpTokenPathRequired': '当前认证方式必须填写 Token 文件路径。',
  },
  en: {
    'slash.title': 'Slash Commands',
    'slash.loading': 'Loading commands, tools, skills, and MCP entries…',
    'slash.empty': 'No matching command, tool, skill, or MCP entry.',
    'slash.section.commands': 'Commands',
    'slash.section.actions': 'Actions',
    'slash.section.skills': 'Skills',
    'slash.section.mcp': 'MCP',
    'slash.section.tools': 'Tools',
    'slash.action.new': '/new',
    'slash.action.newSummary': 'Start a new blank session.',
    'slash.action.history': '/history',
    'slash.action.historySummary': 'Open session history.',
    'slash.action.settings': '/settings',
    'slash.action.settingsSummary': 'Open settings.',
    'slash.action.provider': '/provider',
    'slash.action.providerSummary': 'Open provider settings.',
    'slash.action.skills': '/skills',
    'slash.action.skillsSummary': 'Browse and edit skills.',
    'slash.action.mcp': '/mcp',
    'slash.action.mcpSummary': 'Browse and edit MCP servers.',
    'slash.action.tools': '/tools',
    'slash.action.toolsSummary': 'Show the current tool manifest.',
    'slash.action.skill': '/skill {{name}}',
    'slash.action.skillSummary': 'Inspect the skill and open its editor.',
    'slash.action.mcpServer': '/mcp {{name}}',
    'slash.action.mcpServerSummary': 'Inspect an MCP server entry.',
    'slash.action.tool': '/tool {{name}}',
    'slash.action.toolSummary': 'Inspect a tool summary and permission.',
    'slash.executed': 'Executed {{command}}.',
    'slash.unknown': 'Unknown slash command.',
    'session.drawerKicker': 'Overview',
    'session.drawerTitle': 'Current Session',
    'utility.kicker': 'Runtime',
    'utility.title': 'Workspace & Activity',
    'composer.sending': 'Sending…',
    'composer.waiting': 'Waiting for a response…',
    'composer.streaming': 'Streaming…',
    'message.pendingUser': 'Just sent',
    'message.waitingResponse': 'Waiting for the assistant response…',
    'app.title': 'OpenClaw',
    'brand.kicker': 'OpenCoWork Agent',
    'sidebar.newSession': 'New Session',
    'sidebar.conversations': 'Conversations',
    'sidebar.refresh': 'Refresh sessions',
    'sidebar.search': 'Search sessions',
    'sidebar.searchPlaceholder': 'Search session id',
    'sidebar.noMatch': 'No sessions match the current filter.',
    'sidebar.empty': 'No saved sessions yet.',
    'menu.chat': 'Chat',
    'menu.history': 'Sessions',
    'menu.settings': 'Settings',
    'locale.switchToEnglish': 'English',
    'locale.switchToChinese': '中文',
    'locale.current.zh': 'Chinese',
    'locale.current.en': 'English',
    'empty.title': 'OpenClaw',
    'empty.copy': 'Keep the backend runtime intact while using a cleaner shell for context, memory, tools, and sessions.',
    'empty.example1': 'Inspect the workspace and summarize the current runtime.',
    'empty.example2': 'Adjust API / Provider settings without touching the loop.',
    'empty.example3': 'Create or update a local skill for this project.',
    'session.kicker': 'Session',
    'session.defaultTitle': 'OpenClaw Session',
    'session.defaultSubtitle': 'Load an existing session from the sidebar or start a new one.',
    'session.empty': 'No active session',
    'session.waiting': 'Waiting for first turn',
    'session.loaded': 'Loaded from current state',
    'session.updated': 'Updated {{time}}',
    'session.title': 'OpenClaw / {{id}}',
    'session.subtitle': 'Continue the same backend session with {{count}} stored messages.',
    'session.copyId': 'Copy Session ID',
    'session.new': 'New Blank Session',
    'metric.messages': 'Messages',
    'metric.tools': 'Tool Blocks',
    'metric.tokens': 'Session Tokens',
    'metric.memory': 'Memory',
    'metric.loaded': 'Loaded',
    'metric.notLoaded': 'Not loaded',
    'conversation.title': 'Conversation',
    'conversation.expandAll': 'Expand All',
    'conversation.collapseAll': 'Collapse All',
    'common.expand': 'Expand',
    'common.collapse': 'Collapse',
    'common.edit': 'Edit',
    'common.inspect': 'Inspect',
    'common.delete': 'Delete',
    'common.reset': 'Reset',
    'common.refresh': 'Refresh',
    'common.close': 'Close',
    'common.copy': 'Copy',
    'common.yes': 'Yes',
    'common.no': 'No',
    'common.updated': 'Updated',
    'common.status': 'Status',
    'common.active': 'Active',
    'common.saved': 'Saved',
    'common.draft': 'Draft',
    'workspace.title': 'Workspace',
    'workspace.path': 'Path',
    'workspace.settings': 'Settings',
    'workspace.teamSync': 'Team Sync',
    'workspace.visible': '{{count}} visible',
    'activity.title': 'Turn Activity',
    'activity.iterations': 'Iterations',
    'activity.prompt': 'Prompt Budget',
    'activity.compacted': 'Compacted',
    'activity.events': 'Events',
    'activity.empty': 'No turn activity yet.',
    'composer.placeholder': 'Continue on the existing runtime. Enter sends, Ctrl + Enter adds a new line.',
    'composer.ready': 'Ready.',
    'composer.send': 'Send',
    'composer.clear': 'Clear Input',
    'composer.shortcut': 'Enter sends, Ctrl + Enter adds a new line.',
    'composer.empty': 'Enter some input first.',
    'composer.calling': 'Calling the existing runtime...',
    'composer.done': 'Done. {{iterations}} iteration(s), prompt estimate {{tokens}}.',
    'composer.sendFailed': 'Send failed.',
    'composer.newSessionReady': 'New session ready.',
    'composer.deletedSession': 'Deleted session {{id}}.',
    'composer.messageCopied': 'Message copied.',
    'composer.blockCopied': 'Block copied.',
    'composer.sessionIdCopied': 'Session ID copied.',
    'composer.pathCopied': 'Path copied.',
    'composer.copyFailed': 'Copy failed.',
    'composer.exampleApplied': 'Example copied to input.',
    'composer.refreshDone': 'Session list refreshed.',
    'composer.refreshFailed': 'Refreshing failed.',
    'history.kicker': 'Sessions',
    'history.title': 'Saved Sessions',
    'history.empty': 'No saved sessions yet. Start a conversation and it will appear here.',
    'history.open': 'Open',
    'history.copyId': 'Copy ID',
    'history.messages': '{{count}} messages',
    'history.searchPlaceholder': 'Search session id',
    'history.count': '{{visible}} / {{total}}',
    'settings.kicker': 'Configuration',
    'settings.title': 'Settings',
    'settings.copy': 'Keep the runtime unchanged and handle shell UI, config entry, and bilingual UX only in the shell.',
    'settings.editApi': 'Edit API',
    'settings.pathsTitle': 'Project Paths',
    'settings.pathsCopy': 'Keep the most-used config and skill paths here so you do not have to hunt for them.',
    'settings.configPath': 'Config File',
    'settings.skillsPath': 'Skills Directory',
    'settings.copyConfigPath': 'Copy Config Path',
    'settings.copySkillsDir': 'Copy Skills Path',
    'settings.newSkillShortcut': 'New Skill',
    'settings.noPath': 'No path yet',
    'settings.tab.provider': 'Provider',
    'settings.tab.permission': 'Permissions',
    'settings.tab.skills': 'Skills',
    'settings.tab.mcp': 'MCP',
    'settings.tab.environment': 'Environment',
    'settings.overviewSkillsMeta': 'Project {{project}} / Total {{total}}',
    'settings.overviewMcpMeta': '{{count}} configured',
    'settings.overviewLocaleMeta': 'Click to switch language',
    'permission.title': 'Tool and Edit Permissions',
    'permission.copy': 'Controls permission strategy for tool calls and file edits. Full access is recommended by default to reduce interruptions.',
    'permission.current': 'Current Mode',
    'permission.save': 'Save Permission Mode',
    'permission.saved': 'Permission mode saved.',
    'permission.mode.danger': 'Full Access',
    'permission.mode.workspace': 'Ask Before Write',
    'permission.mode.readonly': 'Read Only',
    'provider.title': 'Current Provider',
    'provider.copy': 'This shows the currently active provider profile. Create, manage, and switch profiles in the list below.',
    'provider.label': 'Provider Label',
    'provider.apiKey': 'API Key',
    'provider.apiKeyReveal': 'Show',
    'provider.apiKeyHide': 'Hide',
    'provider.apiKeyClear': 'Clear saved API key on save',
    'provider.apiKeyStatusExisting': 'An API key is already saved. Leave this blank to keep it unchanged.',
    'provider.apiKeyStatusUpdating': 'Saving will replace the current API key.',
    'provider.apiKeyStatusClearing': 'Saving will clear the current API key.',
    'provider.apiKeyStatusNew': 'Saving will store a new API key.',
    'provider.baseUrlEnv': 'Base URL Env',
    'provider.save': 'Save Provider',
    'provider.defaults': 'Use Defaults',
    'provider.saved': 'Provider settings saved.',
    'provider.reset': 'Provider settings reset to defaults.',
    'provider.defaultState': 'Using defaults',
    'provider.savedState': 'Saved in project',
    'provider.formNote': 'Model, API key, Base URL, and timeout are saved with the current provider profile.',
    'provider.formNoteSaved': 'This provider is already saved in the project config; changes here will overwrite the project-level entry.',
    'provider.formNoteDefault': 'This provider is still using defaults; it will only be written after you save it.',
    'provider.modelPlaceholder': 'claude-sonnet-4-5',
    'provider.namePlaceholder': 'openai-compatible',
    'provider.apiKeyPlaceholder': 'sk-...',
    'provider.baseUrlPlaceholder': 'https://api.openai.com/v1',
    'provider.baseUrlEnvPlaceholder': 'OPENAI_BASE_URL',
    'provider.modelHint': 'Used by shell-issued chat requests without changing the backend loop default.',
    'provider.nameHint': 'Use it to distinguish vendors or routes without changing the runtime structure.',
    'provider.apiKeyHint': 'Paste the API key directly. The shell injects it into the runtime environment before requests.',
    'provider.baseUrlHint': 'Use the compatible API root. Keep the default if you rely on the standard host.',
    'provider.baseUrlEnvHint': 'If you switch endpoints by environment, keep only the variable name here.',
    'provider.timeoutHint': 'Timeout only affects request waiting on this shell layer.',
    'providerProfiles.title': 'Provider Profiles',
    'providerProfiles.copy': 'Save multiple API provider profiles and choose one as the active runtime profile.',
    'providerProfiles.new': 'New Provider Profile',
    'providerProfiles.count': '{{visible}} / {{total}}',
    'providerProfiles.label': 'Profile Label',
    'providerProfiles.labelPlaceholder': 'OpenAI Default',
    'providerProfiles.labelHint': 'Use a clear label so different providers or environments stay easy to distinguish.',
    'providerProfiles.activate': 'Set as active after saving',
    'providerProfiles.save': 'Save Profile',
    'providerProfiles.saved': 'Provider profile saved.',
    'providerProfiles.created': 'Create a new provider profile.',
    'providerProfiles.editing': 'Editing {{name}}.',
    'providerProfiles.active': 'Active',
    'providerProfiles.activateButton': 'Activate',
    'providerProfiles.activated': 'Provider profile activated.',
    'providerProfiles.deleted': 'Provider profile deleted.',
    'providerProfiles.deleteConfirm': 'Delete provider profile {{name}}?',
    'providerProfiles.empty': 'No provider profiles yet. Create one first.',
    'providerProfiles.runtime': 'profile',
    'runtime.title': 'Runtime Snapshot',
    'runtime.copy': 'Read-only context around the current shell environment.',
    'locale.title': 'Interface Language',
    'locale.copy': 'Chinese by default, with quick switching between Chinese and English.',
    'locale.switch': 'Switch Language',
    'locale.current': 'Current Language',
    'skills.title': 'Skill Library',
    'skills.copy': 'Project skills are editable. Skills from other roots stay read-only.',
    'skills.new': 'New Skill',
    'skills.empty': 'No skills yet. Create a project skill first.',
    'skills.searchPlaceholder': 'Search skill name or description',
    'skills.projectOnly': 'Project skills only',
    'skills.count': '{{visible}} / {{total}}',
    'skills.noMatch': 'No skills match the current filter.',
    'skills.none': 'No description.',
    'skills.project': 'project',
    'skills.readonly': 'readonly',
    'skills.created': 'Create a new project skill.',
    'skills.editing': 'Editing {{name}}.',
    'skills.readonlyNote': '{{name}} is discovered from another root and is read-only here.',
    'skills.reset': 'Skill form reset.',
    'skills.saved': 'Skill saved.',
    'skills.deleted': 'Skill deleted.',
    'skills.deleteConfirm': 'Delete skill {{name}}?',
    'skill.formNote': 'Skill content is written to `.opencowork/skills`, and paths / allowed tools affect automatic activation.',
    'skill.formNoteReadonly': 'This skill comes from another root and can only be inspected from this shell.',
    'skill.template': 'Content Template',
    'skill.template.blank': 'Blank',
    'skill.template.basic': 'Basic Skill',
    'skill.template.file': 'File Workflow',
    'skill.template.review': 'Code Review',
    'skill.templateApply': 'Apply Template',
    'skill.templateConfirm': 'The current content is not empty. Replace it with the selected template?',
    'skill.templateApplied': 'Template applied to the form.',
    'skill.namePlaceholder': 'review-rust-runtime',
    'skill.nameHint': 'The name affects the slug and folder path, so keep it stable when possible.',
    'skill.whenHint': 'Describe the trigger moment clearly; the model-facing listing will rely on this first.',
    'skill.toolsHint': 'Comma separated. This limits the tool surface exposed through the skill path.',
    'skill.pathsHint': 'When touched files match these paths, the skill can be injected in the same or next turn.',
    'skill.contextHint': 'Use fork only when the skill truly needs to isolate from the main session.',
    'skill.bodyHint': 'This becomes the actual SKILL.md body, so keep steps, rules, and boundaries explicit.',
    'mcp.title': 'MCP Servers',
    'mcp.copy': 'Manage shell-side MCP entries without changing runtime internals.',
    'mcp.new': 'New Server',
    'mcp.searchPlaceholder': 'Search MCP name, command, or endpoint',
    'mcp.count': '{{visible}} / {{total}}',
    'mcp.noMatch': 'No MCP servers match the current filter.',
    'mcp.transport': 'Transport',
    'mcp.command': 'Command',
    'mcp.args': 'Args',
    'mcp.endpoint': 'Endpoint',
    'mcp.auth': 'Auth',
    'mcp.auth.none': 'none',
    'mcp.auth.env': 'bearer env',
    'mcp.auth.file': 'bearer file',
    'mcp.tokenEnv': 'Token Env',
    'mcp.tokenPath': 'Token Path',
    'mcp.save': 'Save MCP',
    'mcp.empty': 'No MCP servers yet. Create one first.',
    'mcp.none': 'No command or endpoint.',
    'mcp.created': 'Create a new MCP server entry.',
    'mcp.editing': 'Editing {{name}}.',
    'mcp.reset': 'MCP form reset.',
    'mcp.saved': 'MCP saved.',
    'mcp.deleted': 'MCP server deleted.',
    'mcp.deleteConfirm': 'Delete MCP server {{name}}?',
    'mcp.preset': 'Quick Preset',
    'mcp.preset.blank': 'Blank',
    'mcp.preset.stdioNode': 'Local Node stdio',
    'mcp.preset.httpBearer': 'Remote HTTP + Bearer',
    'mcp.preset.sseReadonly': 'Remote SSE Read-only',
    'mcp.presetApply': 'Apply Preset',
    'mcp.presetConfirm': 'The current MCP form already has content. Replace it with the selected preset?',
    'mcp.presetApplied': 'MCP preset applied.',
    'mcp.namePlaceholder': 'filesystem',
    'mcp.nameHint': 'Use a stable service-oriented name; later tool discovery will group around it.',
    'mcp.commandHint': 'Fill this only for stdio transport; args stay in the next field.',
    'mcp.argsHint': 'Comma separated values that are persisted as an argument array.',
    'mcp.endpointHint': 'http / sse / ws all use this field for the full remote endpoint.',
    'mcp.timeoutHint': 'Timeout mainly affects tool discovery and call waiting on the shell layer.',
    'mcp.authModeHint': 'Configure auth only when the shell should attach a Bearer token on your behalf.',
    'mcp.transportHint.stdio': 'stdio mode launches a local command and usually needs command plus args.',
    'mcp.transportHint.network': 'Network mode connects to a remote endpoint and usually only needs Endpoint.',
    'mcp.authHint.none': 'No auth data will be attached.',
    'mcp.authHint.env': 'Bearer token will be read from an environment variable.',
    'mcp.authHint.file': 'Bearer token will be read from a local file.',
    'drawer.kicker': 'Editor',
    'drawer.ready': 'Ready.',
    'drawer.editProvider': 'Edit Provider',
    'drawer.createSkill': 'Create Skill',
    'drawer.editSkill': 'Edit {{name}}',
    'drawer.createMcp': 'Create MCP Server',
    'drawer.editMcp': 'Edit {{name}}',
    'field.model': 'Model',
    'field.name': 'Name',
    'field.baseUrl': 'Base URL',
    'field.timeout': 'Timeout',
    'field.timeoutMs': 'Timeout Ms',
    'field.permission': 'Permission',
    'field.persisted': 'Persisted',
    'field.sessions': 'Sessions',
    'field.skills': 'Skills',
    'field.description': 'Description',
    'skill.when': 'When To Use',
    'skill.argumentHint': 'Argument Hint',
    'skill.allowedTools': 'Allowed Tools',
    'skill.paths': 'Paths',
    'skill.context': 'Context',
    'skill.version': 'Version',
    'skill.agent': 'Agent',
    'skill.effort': 'Effort',
    'skill.body': 'Body',
    'skill.save': 'Save Skill',
    'context.default': 'default',
    'context.current': 'current',
    'context.fork': 'fork',
    'teamMemory.notConfigured': 'Not configured',
    'teamMemory.error': 'Error',
    'teamMemory.running': 'Running',
    'teamMemory.syncing': 'Syncing',
    'teamMemory.configured': 'Configured',
    'teamMemory.none': 'No activity',
    'teamMemory.stats': 'pull {{pulled}} / push {{pushed}}',
    'event.assistantOutput': 'Assistant Output',
    'event.toolCall': 'Tool Call',
    'event.toolResult': 'Tool Result',
    'event.tokenUsage': 'Token Usage',
    'event.messageStop': 'Message Stop',
    'event.messageStopBody': 'Current assistant message finished.',
    'event.usage': 'input {{input}} / output {{output}} / cache read {{cacheRead}} / cache create {{cacheCreate}}',
    'message.user': 'User',
    'message.assistant': 'Assistant',
    'message.system': 'System',
    'message.copy': 'Copy',
    'message.copyBlock': 'Copy Block',
    'message.blocks': '{{count}} blocks',
    'message.chars': '{{count}} chars',
    'message.lines': '{{count}} lines',
    'message.deleteSession': 'Delete session',
    'message.inspectBlock': 'Inspect',
    'message.viewerKicker': 'Block Viewer',
    'message.viewerMeta': '{{type}} · {{chars}} chars · {{lines}} lines',
    'message.success': 'Success',
    'message.failed': 'Failed',
    'message.summaryPath': 'Path',
    'message.summaryCommand': 'Command',
    'message.summaryQuery': 'Query',
    'message.summaryPattern': 'Pattern',
    'message.summaryEndpoint': 'Endpoint',
    'message.summaryCount': 'Count',
    'message.summaryPreview': 'Preview',
    'message.summaryStored': 'Stored',
    'message.summaryToolInput': 'Tool input payload',
    'message.summaryToolOutput': 'Tool result preview',
    'confirm.deleteSession': 'Delete session {{id}}?',
    'status.permissionFallback': 'permission',
    'status.modelFallback': 'model',
    'status.providerSaved': 'provider: saved',
    'status.providerDefault': 'provider: default',
    'errors.providerNameRequired': 'Provider name must not be empty.',
    'errors.providerProfileLabelRequired': 'Provider profile label must not be empty.',
    'errors.providerModelRequired': 'Provider model must not be empty.',
    'errors.providerApiKeyEnvRequired': 'API key env must not be empty.',
    'errors.providerBaseUrlRequired': 'Base URL must not be empty.',
    'errors.skillNameRequired': 'Skill name must not be empty.',
    'errors.skillContentRequired': 'Skill content must not be empty.',
    'errors.mcpNameRequired': 'MCP name must not be empty.',
    'errors.mcpCommandRequired': 'Command is required for stdio transport.',
    'errors.mcpEndpointRequired': 'Endpoint is required for this transport.',
    'errors.mcpTokenEnvRequired': 'Token env is required for this auth mode.',
    'errors.mcpTokenPathRequired': 'Token path is required for this auth mode.',
  },
}

async function request(path, options = {}) {
  const response = await fetch(path, {
    headers: {
      'Content-Type': 'application/json',
      ...(options.headers || {}),
    },
    ...options,
  })

  if (!response.ok) {
    let message = `Request failed: ${response.status}`
    try {
      const payload = await response.json()
      if (payload?.error) {
        message = payload.error
      }
    } catch {}
    throw new Error(message)
  }

  return response.json()
}

function t(key, vars = {}) {
  const active = MESSAGES[state.locale] || MESSAGES.zh
  let value = active[key] || MESSAGES.zh[key] || key
  Object.entries(vars).forEach(([name, replacement]) => {
    value = value.replaceAll(`{{${name}}}`, String(replacement))
  })
  return value
}

function persistLocale() {
  localStorage.setItem('opencowork-shell-locale', state.locale)
}

function persistPaneState() {
  localStorage.setItem(
    'opencowork-shell-pane-state',
    JSON.stringify({
      workspace: state.paneState.workspace,
      activity: state.paneState.activity,
    }),
  )
}

function draftStorageKey(sessionId = state.currentSessionId) {
  return `opencowork-shell-draft:${sessionId || '__new__'}`
}

function persistComposerDraft() {
  const value = String(els.composerInput?.value || '')
  const key = draftStorageKey()
  if (value.trim()) {
    localStorage.setItem(key, value)
    return
  }
  localStorage.removeItem(key)
}

function restoreComposerDraft(sessionId = state.currentSessionId) {
  if (!els.composerInput) return
  const value = localStorage.getItem(draftStorageKey(sessionId)) || ''
  els.composerInput.value = value
  syncComposerHeight()
  updateComposerState()
}

function clearComposerDraft(sessionId = state.currentSessionId) {
  localStorage.removeItem(draftStorageKey(sessionId))
}

function hasComposerDraft(sessionId) {
  const value = localStorage.getItem(draftStorageKey(sessionId)) || ''
  return Boolean(value.trim())
}

function sortedSessions(sessions) {
  return [...(sessions || [])].sort((left, right) => {
    const leftTs = Number(left?.updatedAtUnixMs || 0)
    const rightTs = Number(right?.updatedAtUnixMs || 0)
    return rightTs - leftTs
  })
}

function sessionMemoryContent(session) {
  return getValue(session, 'currentSessionMemory', 'current_session_memory') || ''
}

function normalizeSessionText(value, maxLength = 96) {
  const normalized = String(value || '').replace(/\s+/g, ' ').trim()
  if (!normalized) return ''
  return normalized.length > maxLength
    ? `${normalized.slice(0, Math.max(0, maxLength - 3)).trimEnd()}...`
    : normalized
}

function extractSessionMemorySection(memory, heading) {
  const source = String(memory || '')
  if (!source) return ''
  const lines = source.split(/\r?\n/)
  const target = `# ${heading}`.toLowerCase()
  let active = false
  for (const line of lines) {
    const trimmed = line.trim()
    if (trimmed.startsWith('#')) {
      if (active) break
      active = trimmed.toLowerCase() === target
      continue
    }
    if (!active || !trimmed) continue
    if (trimmed.startsWith('_') && trimmed.endsWith('_')) continue
    return normalizeSessionText(trimmed)
  }
  return ''
}

function firstRoleText(session, role, maxLength = 72) {
  const messages = getValue(session, 'messages') || []
  const targetRole = String(role || '').toLowerCase()
  for (const message of messages) {
    if (String(message?.role || '').toLowerCase() !== targetRole) continue
    const text = (message.blocks || [])
      .filter((block) => block.type === 'text')
      .map((block) => getValue(block, 'text') || '')
      .join('\n\n')
    const normalized = normalizeSessionText(text, maxLength)
    if (normalized) return normalized
  }
  return ''
}

function latestRoleText(session, role, maxLength = 144) {
  const messages = getValue(session, 'messages') || []
  const targetRole = String(role || '').toLowerCase()
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index]
    if (String(message?.role || '').toLowerCase() !== targetRole) continue
    const text = (message.blocks || [])
      .filter((block) => block.type === 'text')
      .map((block) => getValue(block, 'text') || '')
      .join('\n\n')
    const normalized = normalizeSessionText(text, maxLength)
    if (normalized) return normalized
  }
  return ''
}

function derivedSessionTitle(session, fallbackId = '') {
  return extractSessionMemorySection(sessionMemoryContent(session), 'Session Title')
    || firstRoleText(session, 'user')
    || firstRoleText(session, 'assistant')
    || fallbackId
    || ''
}

function derivedSessionPreview(session) {
  return extractSessionMemorySection(sessionMemoryContent(session), 'Current State')
    || latestRoleText(session, 'assistant')
    || latestRoleText(session, 'user')
    || ''
}

function sessionDisplayTitle(sessionDescriptor, fallbackSession = null) {
  return normalizeSessionText(sessionDescriptor?.title, 72)
    || derivedSessionTitle(fallbackSession, sessionDescriptor?.id || state.currentSessionId || '')
    || normalizeSessionText(state.pendingTurn?.userInput, 72)
    || t('session.defaultTitle')
}

function sessionDisplayPreview(sessionDescriptor, fallbackSession = null) {
  return normalizeSessionText(sessionDescriptor?.preview, 144)
    || derivedSessionPreview(fallbackSession)
    || normalizeSessionText(state.pendingTurn?.userInput, 144)
}

function sessionStateKey(sessionId) {
  if (state.currentSessionId === sessionId) return 'active'
  if (hasComposerDraft(sessionId)) return 'draft'
  return 'saved'
}

function sessionStateLabel(sessionId) {
  const key = sessionStateKey(sessionId)
  if (key === 'active') return t('common.active')
  if (key === 'draft') return t('common.draft')
  return t('common.saved')
}

function matchesSessionQuery(session, rawQuery) {
  const query = String(rawQuery || '').trim().toLowerCase()
  if (!query) return true
  return [
    session?.id,
    session?.title,
    session?.preview,
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase()
    .includes(query)
}

function getValue(object, ...keys) {
  for (const key of keys) {
    if (object && object[key] !== undefined && object[key] !== null) {
      return object[key]
    }
  }
  return null
}

function safeCount(value) {
  return Array.isArray(value) ? value.length : 0
}

function compactText(value, maxLength = 64) {
  const text = String(value || '')
  if (!text) return '-'
  if (text.length <= maxLength) return text
  const head = Math.max(22, Math.floor(maxLength / 2) - 2)
  const tail = Math.max(14, Math.floor(maxLength / 2) - 4)
  return `${text.slice(0, head)} ... ${text.slice(-tail)}`
}

function copyText(value) {
  return navigator.clipboard.writeText(String(value || ''))
}

function lineCount(value) {
  const text = String(value || '')
  return text ? text.split('\n').length : 0
}

function wait(ms) {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

function shellSlashActions() {
  return [
    { key: 'new', title: t('slash.action.new'), subtitle: t('slash.action.newSummary'), action: { type: 'new-session' } },
    { key: 'history', title: t('slash.action.history'), subtitle: t('slash.action.historySummary'), action: { type: 'open-history' } },
    { key: 'settings', title: t('slash.action.settings'), subtitle: t('slash.action.settingsSummary'), action: { type: 'open-settings' } },
    { key: 'provider', title: t('slash.action.provider'), subtitle: t('slash.action.providerSummary'), action: { type: 'open-settings-tab', tab: 'provider' } },
    { key: 'skills', title: t('slash.action.skills'), subtitle: t('slash.action.skillsSummary'), action: { type: 'open-settings-tab', tab: 'skills' } },
    { key: 'mcp', title: t('slash.action.mcp'), subtitle: t('slash.action.mcpSummary'), action: { type: 'open-settings-tab', tab: 'mcp' } },
    { key: 'tools', title: t('slash.action.tools'), subtitle: t('slash.action.toolsSummary'), action: { type: 'show-tools' } },
  ]
}

function slashQuery() {
  return String(els.composerInput?.value || '').trimStart()
}

function isSlashInput(value = slashQuery()) {
  return value.startsWith('/')
}

function closeSlashMenu() {
  state.slashMenu.open = false
  state.slashMenu.query = ''
  state.slashMenu.items = []
  state.slashMenu.activeIndex = 0
  if (els.slashMenu) {
    els.slashMenu.classList.add('is-hidden')
  }
}

function slashItemText(item) {
  return [item.title, item.subtitle, item.keyword, item.meta].filter(Boolean).join(' ').toLowerCase()
}

function groupedSlashItems() {
  const query = state.slashMenu.query.trim().toLowerCase()
  const matches = (item) => !query || slashItemText(item).includes(query)
  const commandItems = (state.slashCatalog.commands || []).map((command) => ({
    id: `command:${command.name}`,
    section: 'commands',
    title: `/${command.name}${command.argumentHint ? ` ${command.argumentHint}` : ''}`,
    subtitle: command.summary,
    keyword: command.name,
    meta: 'CLI',
    action: { type: 'builtin-command', input: `/${command.name}` },
  }))
  const actionItems = shellSlashActions().map((action) => ({
    id: `action:${action.key}`,
    section: 'actions',
    title: action.title,
    subtitle: action.subtitle,
    keyword: action.key,
    meta: 'UI',
    action: action.action,
  }))
  const skillItems = (state.bootstrap?.skills || []).map((skill) => ({
    id: `skill:${deriveSkillSlug(skill)}`,
    section: 'skills',
    title: t('slash.action.skill', { name: skill.name }),
    subtitle: skill.whenToUse || skill.description || skill.path,
    keyword: `${skill.name} ${deriveSkillSlug(skill)}`,
    meta: 'Skill',
    action: { type: 'open-skill', slug: deriveSkillSlug(skill), name: skill.name },
  }))
  const mcpItems = (state.bootstrap?.mcpServers || []).map((server) => ({
    id: `mcp:${server.name}`,
    section: 'mcp',
    title: t('slash.action.mcpServer', { name: server.name }),
    subtitle: server.endpoint || server.command || server.transport,
    keyword: `${server.name} ${server.transport}`,
    meta: server.transport?.toUpperCase?.() || 'MCP',
    action: { type: 'open-mcp', name: server.name },
  }))
  const toolItems = (state.slashCatalog.tools || []).map((tool) => ({
    id: `tool:${tool.name}`,
    section: 'tools',
    title: t('slash.action.tool', { name: tool.name }),
    subtitle: `${tool.description || '-'} · ${tool.permission}`,
    keyword: `${tool.name} ${tool.source} ${tool.permission}`,
    meta: `${tool.source} · ${tool.permission}`,
    action: { type: 'show-tool', name: tool.name },
  }))
  return [
    { key: 'commands', label: t('slash.section.commands'), items: commandItems.filter(matches) },
    { key: 'actions', label: t('slash.section.actions'), items: actionItems.filter(matches) },
    { key: 'skills', label: t('slash.section.skills'), items: skillItems.filter(matches) },
    { key: 'mcp', label: t('slash.section.mcp'), items: mcpItems.filter(matches) },
    { key: 'tools', label: t('slash.section.tools'), items: toolItems.filter(matches) },
  ].filter((group) => group.items.length)
}

function refreshSlashItems() {
  const groups = groupedSlashItems()
  state.slashMenu.items = groups.flatMap((group) => group.items)
  if (!state.slashMenu.items.length) {
    state.slashMenu.activeIndex = 0
    return groups
  }
  state.slashMenu.activeIndex = Math.max(0, Math.min(state.slashMenu.activeIndex, state.slashMenu.items.length - 1))
  return groups
}

function renderSlashMenu() {
  if (!els.slashMenu || !els.slashMenuList || !els.slashMenuMeta) return
  if (!state.slashMenu.open) {
    els.slashMenu.classList.add('is-hidden')
    return
  }
  els.slashMenu.classList.remove('is-hidden')
  const groups = refreshSlashItems()
  const totalCount = state.slashMenu.items.length
  els.slashMenuMeta.textContent = state.slashCatalog.loading
    ? t('slash.loading')
    : state.slashMenu.query
      ? `/${state.slashMenu.query} · ${totalCount}`
      : `/${totalCount ? ` · ${totalCount}` : ''}`.trim()
  if (state.slashCatalog.loading && !groups.length) {
    els.slashMenuList.innerHTML = `<div class="slash-menu-empty">${t('slash.loading')}</div>`
    return
  }
  if (!groups.length) {
    els.slashMenuList.innerHTML = `<div class="slash-menu-empty">${t('slash.empty')}</div>`
    return
  }
  els.slashMenuList.innerHTML = ''
  let runningIndex = 0
  groups.forEach((group) => {
    const section = document.createElement('section')
    section.className = 'slash-menu-section'
    const label = document.createElement('div')
    label.className = 'slash-menu-section-label'
    label.textContent = group.label
    section.appendChild(label)
    group.items.forEach((item) => {
      const currentIndex = runningIndex
      const button = document.createElement('button')
      button.type = 'button'
      button.className = `slash-menu-item${currentIndex === state.slashMenu.activeIndex ? ' is-active' : ''}`
      button.addEventListener('click', async () => {
        state.slashMenu.activeIndex = currentIndex
        renderSlashMenu()
        await executeSlashItem(item)
      })
      const copy = document.createElement('div')
      copy.className = 'slash-menu-item-copy'
      const title = document.createElement('strong')
      title.textContent = item.title
      const subtitle = document.createElement('span')
      subtitle.textContent = item.subtitle
      copy.appendChild(title)
      copy.appendChild(subtitle)
      button.appendChild(copy)
      if (item.meta) {
        const meta = document.createElement('span')
        meta.className = 'slash-menu-item-meta'
        meta.textContent = item.meta
        button.appendChild(meta)
      }
      section.appendChild(button)
      runningIndex += 1
    })
    els.slashMenuList.appendChild(section)
  })
  requestAnimationFrame(() => {
    const active = els.slashMenuList.querySelector('.slash-menu-item.is-active')
    active?.scrollIntoView({ block: 'nearest' })
  })
}

async function ensureSlashCatalogLoaded() {
  if (state.slashCatalog.loaded || state.slashCatalog.loading) return
  state.slashCatalog.loading = true
  renderSlashMenu()
  try {
    const [commands, tools] = await Promise.all([
      request('/api/slash-specs'),
      request('/api/tool-manifest'),
    ])
    state.slashCatalog.commands = commands || []
    state.slashCatalog.tools = tools || []
    state.slashCatalog.loaded = true
  } catch (error) {
    setComposerStatus(error.message || t('slash.empty'), true)
  } finally {
    state.slashCatalog.loading = false
    renderSlashMenu()
  }
}

function updateSlashMenuFromComposer() {
  const raw = slashQuery()
  if (!isSlashInput(raw)) {
    closeSlashMenu()
    return
  }
  state.slashMenu.open = true
  state.slashMenu.query = raw.slice(1)
  state.slashMenu.activeIndex = 0
  renderSlashMenu()
  ensureSlashCatalogLoaded()
}

function moveSlashSelection(direction) {
  if (!state.slashMenu.open || !state.slashMenu.items.length) return
  const total = state.slashMenu.items.length
  state.slashMenu.activeIndex = (state.slashMenu.activeIndex + direction + total) % total
  renderSlashMenu()
}

function projectSkillDirectory() {
  const cwd = state.bootstrap?.cwd
  if (!cwd) return ''
  const separator = cwd.includes('\\') ? '\\' : '/'
  return `${cwd}${separator}.opencowork${separator}skills`
}

function updateComposerState() {
  const empty = !els.composerInput.value.trim()
  els.sendButton.disabled = state.sending || empty
  els.sendButton.textContent = state.sending
    ? t(state.pendingTurn?.phase === 'streaming' ? 'composer.streaming' : 'composer.sending')
    : t('composer.send')
  els.sendButton.classList.toggle('is-busy', state.sending)
  els.composerInput.readOnly = state.sending
  if (els.clearInputButton) {
    els.clearInputButton.disabled = state.sending || !els.composerInput.value.length
  }
}

function syncComposerHeight() {
  if (!els.composerInput) return
  els.composerInput.style.height = 'auto'
  const next = Math.min(Math.max(els.composerInput.scrollHeight, 90), 220)
  els.composerInput.style.height = `${next}px`
}

function skillTemplates() {
  if (state.locale === 'en') {
    return {
      blank: { description: '', when: '', content: '' },
      basic: {
        description: 'Reusable workflow guidance for a focused task.',
        when: 'Use when this task shows up repeatedly and needs stable boundaries.',
        content: `# Goal
Handle this task in a repeatable and bounded way.

# When To Use
- The task clearly matches this workflow
- The user wants consistent output and boundaries

# Steps
1. Gather the minimum context needed.
2. Execute the task with the allowed tools only.
3. Verify the result before responding.

# Boundaries
- Do not exceed the requested scope.
- Ask for clarification only when blocked by missing information.

# Output
- Return the concrete result first.
- Keep explanations concise and actionable.`,
      },
      file: {
        description: 'File-oriented workflow for reading, editing, and verifying project files.',
        when: 'Use when the task depends on inspecting or updating files inside the workspace.',
        content: `# Goal
Work on repository files safely and efficiently.

# Steps
1. Inspect the relevant files before editing.
2. Make the smallest change set that solves the task.
3. Run the narrowest verification that proves the change.

# Boundaries
- Do not rewrite unrelated files.
- Preserve user changes unless explicitly asked to replace them.

# Output
- Summarize changed files and verification results.`,
      },
      review: {
        description: 'Code review mode focused on bugs, regressions, and missing tests.',
        when: 'Use when the task is to review changes or inspect a diff with a reviewer mindset.',
        content: `# Goal
Review code with a bug-finding mindset.

# Priorities
- Behavioral regressions
- Edge cases and failure paths
- Missing validation or tests

# Steps
1. Read the changed code and surrounding context.
2. Identify concrete findings with file references.
3. Separate findings from open questions.

# Output
- List findings first, ordered by severity.
- Keep summaries brief.`,
      },
    }
  }

  return {
    blank: { description: '', when: '', content: '' },
    basic: {
      description: '针对某类重复任务的稳定工作流说明。',
      when: '当这类任务反复出现，且需要固定边界和输出方式时使用。',
      content: `# 目标
用稳定、可复用的方式完成这类任务。

# 适用场景
- 当前任务明显符合这套流程
- 需要固定边界、固定输出风格

# 工作步骤
1. 先收集最少但必要的上下文。
2. 只使用允许的工具执行任务。
3. 在回复前做最小但有效的验证。

# 边界
- 不扩大任务范围。
- 只有在缺少关键信息时才追问。

# 输出要求
- 先给结果，再补充必要说明。
- 说明保持简洁、可执行。`,
    },
    file: {
      description: '面向文件读写、修改和验证的工作流。',
      when: '当任务依赖读取、编辑或验证工作区文件时使用。',
      content: `# 目标
安全、高效地处理仓库内的文件任务。

# 工作步骤
1. 编辑前先检查相关文件和上下文。
2. 用尽量小的改动完成任务。
3. 运行最小必要的验证来证明结果成立。

# 边界
- 不改无关文件。
- 除非用户明确要求，否则不要覆盖已有用户修改。

# 输出要求
- 总结改动文件。
- 说明验证是否通过。`,
    },
    review: {
      description: '以代码审查模式检查风险、回归和缺失测试。',
      when: '当任务是 Review、检查 diff 或排查回归风险时使用。',
      content: `# 目标
用代码审查视角识别问题，而不是复述改动。

# 优先级
- 行为回归
- 边界条件和失败路径
- 缺失校验或测试

# 工作步骤
1. 阅读改动和周边上下文。
2. 给出具体问题，并标明文件位置。
3. 把问题和开放问题分开。

# 输出要求
- 先列 findings，再给简短总结。
- 按严重程度排序。`,
    },
  }
}

function mcpPresets() {
  return {
    blank: {
      name: '',
      transport: 'stdio',
      command: '',
      args: '',
      endpoint: '',
      timeoutMs: '90000',
      authType: 'none',
      tokenEnv: '',
      tokenPath: '',
    },
    'stdio-node': {
      name: '',
      transport: 'stdio',
      command: 'node',
      args: 'server.js',
      endpoint: '',
      timeoutMs: '90000',
      authType: 'none',
      tokenEnv: '',
      tokenPath: '',
    },
    'http-bearer': {
      name: '',
      transport: 'http',
      command: '',
      args: '',
      endpoint: 'http://127.0.0.1:3000/mcp',
      timeoutMs: '90000',
      authType: 'bearer-env',
      tokenEnv: 'MCP_AUTH_TOKEN',
      tokenPath: '',
    },
    'sse-readonly': {
      name: '',
      transport: 'sse',
      command: '',
      args: '',
      endpoint: 'http://127.0.0.1:3000/sse',
      timeoutMs: '90000',
      authType: 'none',
      tokenEnv: '',
      tokenPath: '',
    },
  }
}

function toLocaleTimestamp(value) {
  if (!value) return state.locale === 'zh' ? '刚刚' : 'Just now'
  return new Date(Number(value)).toLocaleString(state.locale === 'zh' ? 'zh-CN' : 'en-US', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function splitComma(value) {
  return String(value || '')
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean)
}

function slugify(value) {
  const slug = String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
  return slug || 'custom-skill'
}

function normalizePath(value) {
  return String(value || '')
    .replace(/\\/g, '/')
    .replace(/\/+/g, '/')
    .toLowerCase()
}

function deriveSkillSlug(skill) {
  const parts = String(skill?.path || '')
    .split(/[\\/]/)
    .filter(Boolean)
  return parts.length >= 2 ? parts[parts.length - 2] : slugify(skill?.name)
}

function isProjectLocalSkill(skill) {
  return normalizePath(skill?.path).includes('/.opencowork/skills/')
}

function currentSessionDescriptor() {
  return (state.bootstrap?.sessions || []).find((session) => session.id === state.currentSessionId) || null
}

function visibleSessionCount() {
  const sessions = sortedSessions(state.bootstrap?.sessions || [])
  const query = state.sessionFilter.trim().toLowerCase()
  return query
    ? sessions.filter((session) => matchesSessionQuery(session, query)).length
    : sessions.length
}

function buildPendingMessages() {
  if (!state.pendingTurn) return []
  const assistantText = state.pendingTurn.assistantText || t('message.waitingResponse')
  return [
    {
      role: 'user',
      blocks: [{ type: 'text', text: state.pendingTurn.userInput }],
      __pending: true,
      __pendingState: 'user',
    },
    {
      role: 'assistant',
      blocks: [{ type: 'text', text: assistantText }],
      __pending: true,
      __pendingState: state.pendingTurn.phase === 'streaming' ? 'streaming' : 'waiting',
    },
  ]
}

function buildVisibleMessages() {
  return [...(state.currentSession?.messages || []), ...buildPendingMessages()]
}

function extractLatestAssistantText(session) {
  const messages = session?.messages || []
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index]
    if (String(message?.role || '').toLowerCase() !== 'assistant') continue
    const text = (message.blocks || [])
      .filter((block) => block.type === 'text')
      .map((block) => getValue(block, 'text') || '')
      .join('\n\n')
      .trim()
    if (text) return text
  }
  return ''
}

function splitReplayText(text, preferredChunk = 28, maxSegments = 72) {
  const normalized = String(text || '')
  if (!normalized) return []
  const chunkSize = Math.max(preferredChunk, Math.ceil(normalized.length / maxSegments))
  const parts = []
  for (let index = 0; index < normalized.length; index += chunkSize) {
    parts.push(normalized.slice(index, index + chunkSize))
  }
  return parts
}

function buildAssistantReplaySegments(events, session) {
  const segments = []
  ;(events || []).forEach((event) => {
    if (event?.type !== 'assistant_text_delta') return
    splitReplayText(getValue(event, 'text') || '', 18, 36).forEach((segment) => {
      if (segment) segments.push(segment)
    })
  })
  if (segments.length) return segments
  return splitReplayText(extractLatestAssistantText(session), 24, 52)
}

function renderConversationSurface() {
  renderChatSummary()
  renderWorkspaceMeta()
  renderTurnStats()
  renderMessages()
  renderEvents()
  renderPaneState()
  updateComposerState()
}

function setComposerStatus(message, isError = false) {
  els.composerStatus.textContent = message
  els.composerStatus.dataset.tone = isError ? 'error' : 'default'
}

function setDrawerStatus(message, isError = false) {
  els.drawerStatus.textContent = message
  els.drawerStatus.dataset.tone = isError ? 'error' : 'default'
}

function updateLocaleControls() {
  document.documentElement.lang = state.locale === 'zh' ? 'zh-CN' : 'en'
  document.title = t('app.title')
  els.localeToggleIcon.textContent = state.locale === 'zh' ? 'EN' : '中'
  els.localeToggleLabel.textContent =
    state.locale === 'zh' ? t('locale.switchToEnglish') : t('locale.switchToChinese')
  if (els.localeCurrentValue) {
    els.localeCurrentValue.textContent =
      state.locale === 'zh' ? t('locale.current.zh') : t('locale.current.en')
  }
}

function applyStaticLocale() {
  document.querySelectorAll('[data-i18n]').forEach((node) => {
    const key = node.dataset.i18n
    if (!key) return
    node.textContent = t(key)
  })
  document.querySelectorAll('[data-i18n-placeholder]').forEach((node) => {
    const key = node.dataset.i18nPlaceholder
    if (!key) return
    node.setAttribute('placeholder', t(key))
  })
  document.querySelectorAll('[data-i18n-title]').forEach((node) => {
    const key = node.dataset.i18nTitle
    if (!key) return
    node.setAttribute('title', t(key))
  })
  if (els.composerStatus.dataset.tone !== 'error' && !state.sending) {
    els.composerStatus.textContent = t('composer.ready')
  }
  if (els.drawerStatus.dataset.tone !== 'error') {
    els.drawerStatus.textContent = t('drawer.ready')
  }
  updateLocaleControls()
  renderProviderFormNote()
  renderSkillFormNote()
  renderMcpFieldState()
  renderPaneState()
  updateComposerState()
  refreshBlockViewer()
}

function setLocale(locale) {
  state.locale = locale === 'en' ? 'en' : 'zh'
  persistLocale()
  applyStaticLocale()
  renderAll()
  setDrawerMode(state.drawerMode)
}

function toggleLocale() {
  setLocale(state.locale === 'zh' ? 'en' : 'zh')
}

function renderPaneState() {
  const overviewCollapsed = Boolean(state.paneState.overview)
  const utilityCollapsed = Boolean(state.paneState.utility)
  const workspaceCollapsed = Boolean(state.paneState.workspace)
  const activityCollapsed = Boolean(state.paneState.activity)
  const metrics = computeSessionMetrics({
    messages: buildVisibleMessages(),
    currentSessionMemory: sessionMemoryContent(state.currentSession),
  })
  const eventCount = normalizeEvents(state.lastEvents).length

  const hasOverview = Boolean(state.currentSessionId || state.currentSession || state.pendingTurn)

  if (els.sessionOverviewDrawer) {
    els.sessionOverviewDrawer.classList.toggle('is-hidden', !hasOverview || overviewCollapsed)
    els.sessionOverviewDrawer.classList.toggle('is-collapsed', false)
  }
  if (els.sessionOverviewContent) {
    els.sessionOverviewContent.classList.remove('is-hidden')
  }
  if (els.sessionOverviewToggle) {
    els.sessionOverviewToggle.textContent = t('common.close')
  }
  if (els.sessionOverviewMeta) {
    const bits = []
    const messageCount = buildVisibleMessages().length
    if (messageCount) bits.push(t('history.messages', { count: messageCount }))
    if (metrics.totalTokens) bits.push(`${metrics.totalTokens} tokens`)
    if (state.pendingTurn) {
      bits.push(t(state.pendingTurn.phase === 'streaming' ? 'composer.streaming' : 'composer.waiting'))
    }
    els.sessionOverviewMeta.textContent = bits.join(' · ') || '-'
  }
  if (els.sessionOverviewLaunchButton) {
    els.sessionOverviewLaunchButton.disabled = !hasOverview
    els.sessionOverviewLaunchButton.classList.toggle('is-active', hasOverview && !overviewCollapsed)
    const messageCount = buildVisibleMessages().length
    els.sessionOverviewLaunchButton.textContent = messageCount
      ? `${t('session.drawerKicker')} · ${messageCount}`
      : t('session.drawerKicker')
    els.sessionOverviewLaunchButton.title = els.sessionOverviewMeta?.textContent || t('session.drawerTitle')
  }

  if (els.utilityDrawer) {
    els.utilityDrawer.classList.toggle('is-hidden', utilityCollapsed)
  }
  if (els.utilityDrawerContent) {
    els.utilityDrawerContent.classList.remove('is-hidden')
  }
  if (els.utilityDrawerToggle) {
    els.utilityDrawerToggle.textContent = t('common.close')
  }
  if (els.utilityDrawerMeta) {
    els.utilityDrawerMeta.textContent = `${t('workspace.visible', { count: visibleSessionCount() })} · ${eventCount}`
  }
  if (els.utilityLaunchButton) {
    els.utilityLaunchButton.classList.toggle('is-active', !utilityCollapsed)
    els.utilityLaunchButton.textContent = eventCount > 0
      ? `${t('utility.kicker')} · ${eventCount}`
      : t('utility.kicker')
    els.utilityLaunchButton.title = `${t('workspace.visible', { count: visibleSessionCount() })} · ${eventCount}`
  }

  els.workspacePaneCard.classList.toggle('is-collapsed', workspaceCollapsed)
  els.workspacePaneContent.classList.toggle('is-hidden', workspaceCollapsed)
  els.workspacePaneToggle.textContent = workspaceCollapsed ? t('common.expand') : t('common.collapse')

  els.activityPaneCard.classList.toggle('is-collapsed', activityCollapsed)
  els.activityPaneContent.classList.toggle('is-hidden', activityCollapsed)
  els.activityPaneToggle.textContent = activityCollapsed ? t('common.expand') : t('common.collapse')
}

function togglePane(name) {
  const nextValue = !state.paneState[name]
  state.paneState[name] = nextValue
  if (nextValue === false) {
    if (name === 'overview') {
      state.paneState.utility = true
    } else if (name === 'utility') {
      state.paneState.overview = true
    }
  }
  persistPaneState()
  renderPaneState()
}

function isNetworkTransport(value) {
  return ['http', 'sse', 'ws'].includes(String(value || '').toLowerCase())
}

function renderProviderFormNote() {
  if (els.providerFormNote) {
    const profile = selectedProviderProfile()
    els.providerFormNote.textContent = profile ? t('provider.formNoteSaved') : t('provider.formNoteDefault')
  }
}

function renderProviderApiKeyState() {
  const profile = selectedProviderProfile()
  const hasSavedKey = Boolean(profile?.hasApiKey)
  const hasTypedKey = Boolean(els.providerApiKey?.value.trim())
  const clearing = Boolean(els.providerClearApiKey?.checked)

  if (els.providerApiKey) {
    els.providerApiKey.type = state.providerApiKeyVisible ? 'text' : 'password'
  }
  if (els.providerApiKeyVisibilityButton) {
    els.providerApiKeyVisibilityButton.textContent = state.providerApiKeyVisible
      ? t('provider.apiKeyHide')
      : t('provider.apiKeyReveal')
  }
  if (els.providerClearApiKeyRow) {
    els.providerClearApiKeyRow.classList.toggle('is-hidden', !hasSavedKey)
  }
  if (els.providerApiKeyStatus) {
    const key = clearing
      ? 'provider.apiKeyStatusClearing'
      : hasTypedKey
        ? 'provider.apiKeyStatusUpdating'
        : hasSavedKey
          ? 'provider.apiKeyStatusExisting'
          : 'provider.apiKeyStatusNew'
    els.providerApiKeyStatus.textContent = t(key)
  }
}

function renderProjectPaths() {
  const settingsPath = state.bootstrap?.settingsFile || ''
  const skillsPath = projectSkillDirectory()
  if (els.settingsFilePath) {
    els.settingsFilePath.textContent = settingsPath || t('settings.noPath')
    els.settingsFilePath.title = settingsPath
  }
  if (els.skillsDirPath) {
    els.skillsDirPath.textContent = skillsPath || t('settings.noPath')
    els.skillsDirPath.title = skillsPath
  }
}

function renderSkillFormNote() {
  if (els.skillFormNote) {
    const detail = state.selectedSkillDetail
    const readonly = detail && !isProjectLocalSkill(detail)
    els.skillFormNote.textContent = readonly ? t('skill.formNoteReadonly') : t('skill.formNote')
  }
}

function renderMcpFieldState() {
  const transport = els.mcpTransport.value
  const authType = els.mcpAuthType.value
  const network = isNetworkTransport(transport)

  els.mcpCommandGroup.classList.toggle('is-hidden', network)
  els.mcpArgsGroup.classList.toggle('is-hidden', network)
  els.mcpEndpointGroup.classList.toggle('is-hidden', !network)
  els.mcpTokenEnvGroup.classList.toggle('is-hidden', authType !== 'bearer-env')
  els.mcpTokenPathGroup.classList.toggle('is-hidden', authType !== 'bearer-file')

  if (els.mcpTransportHint) {
    els.mcpTransportHint.textContent = network
      ? t('mcp.transportHint.network')
      : t('mcp.transportHint.stdio')
  }
  if (els.mcpAuthHint) {
    const key =
      authType === 'bearer-env'
        ? 'mcp.authHint.env'
        : authType === 'bearer-file'
          ? 'mcp.authHint.file'
          : 'mcp.authHint.none'
    els.mcpAuthHint.textContent = t(key)
  }
}

function shouldCollapseBlock(content) {
  const text = String(content || '')
  return text.length > 500 || text.split('\n').length > 12
}

function usageSummary(message) {
  const usage = getValue(message, 'usage') || {}
  const inputTokens = Number(getValue(usage, 'input_tokens', 'inputTokens') || 0)
  const outputTokens = Number(getValue(usage, 'output_tokens', 'outputTokens') || 0)
  const cacheRead = Number(getValue(usage, 'cache_read_input_tokens', 'cacheReadInputTokens') || 0)
  const cacheCreate = Number(getValue(usage, 'cache_creation_input_tokens', 'cacheCreationInputTokens') || 0)
  const total = inputTokens + outputTokens + cacheRead + cacheCreate
  return total > 0 ? `${total} ${state.locale === 'zh' ? 'tokens' : 'tokens'}` : null
}

function messageRoleLabel(role) {
  const normalized = String(role || '').toLowerCase()
  if (normalized === 'user') return t('message.user')
  if (normalized === 'assistant') return t('message.assistant')
  return normalized ? normalized : t('message.system')
}

function messageAvatarLabel(role) {
  const normalized = String(role || '').toLowerCase()
  if (normalized === 'user') return 'U'
  if (normalized === 'assistant') return 'A'
  return 'S'
}

function messagePlainText(message) {
  return (message.blocks || [])
    .map((block) => blockContent(block))
    .filter(Boolean)
    .join('\n\n')
}

function blockLabel(block) {
  switch (block.type) {
    case 'text':
      return 'text'
    case 'tool_use':
      return `tool_use / ${getValue(block, 'name') || (state.locale === 'zh' ? '未知' : 'unknown')}`
    case 'tool_result':
      return `tool_result / ${getValue(block, 'tool_name', 'toolName') || (state.locale === 'zh' ? '未知' : 'unknown')}`
    default:
      return block.type || 'block'
  }
}

function blockContent(block) {
  if (block.type === 'text') return getValue(block, 'text') || ''
  if (block.type === 'tool_use') return JSON.stringify(getValue(block, 'input') || {}, null, 2)
  if (block.type === 'tool_result') {
    const output = getValue(block, 'output')
    return typeof output === 'string' ? output : JSON.stringify(output || {}, null, 2)
  }
  return JSON.stringify(block, null, 2)
}

function inlineSummaryText(value, maxLength = 120) {
  const text = String(value || '')
    .replace(/\s+/g, ' ')
    .trim()
  return text ? compactText(text, maxLength) : null
}

function summarizeFieldValue(value, maxLength = 48) {
  if (value === null || value === undefined) return null
  if (Array.isArray(value)) {
    return value.length ? compactText(value.map((item) => String(item)).join(', '), maxLength) : null
  }
  if (typeof value === 'object') {
    const direct =
      getValue(value, 'path', 'file', 'target', 'message', 'summary', 'preview', 'stdout', 'stderr') || null
    if (direct && typeof direct !== 'object') {
      return summarizeFieldValue(direct, maxLength)
    }
    return inlineSummaryText(JSON.stringify(value), maxLength)
  }
  return inlineSummaryText(value, maxLength)
}

function summaryLabelForKey(key) {
  if (['path', 'file', 'target'].includes(key)) return 'message.summaryPath'
  if (key === 'command') return 'message.summaryCommand'
  if (key === 'query') return 'message.summaryQuery'
  if (key === 'pattern') return 'message.summaryPattern'
  if (['endpoint', 'url'].includes(key)) return 'message.summaryEndpoint'
  if (['preview', 'message', 'summary', 'stdout', 'stderr'].includes(key)) return 'message.summaryPreview'
  return null
}

function collectSummaryEntries(source, keys, limit = 3) {
  const entries = []
  keys.forEach((key) => {
    if (entries.length >= limit) return
    const labelKey = summaryLabelForKey(key)
    const value = summarizeFieldValue(getValue(source, key))
    if (!labelKey || !value) return
    if (entries.some((entry) => entry.labelKey === labelKey && entry.value === value)) return
    entries.push({ labelKey, value })
  })
  return entries
}

function summaryCount(source) {
  if (!source || typeof source !== 'object') return null
  const direct = getValue(source, 'count', 'total', 'matches', 'results', 'items', 'files', 'diagnostics', 'lines')
  if (typeof direct === 'number' && Number.isFinite(direct)) return direct
  if (Array.isArray(direct)) return direct.length
  return null
}

function summarySuccess(source) {
  if (!source || typeof source !== 'object') return null
  const value = getValue(source, 'success', 'ok')
  return typeof value === 'boolean' ? value : null
}

function formatSummaryEntries(entries) {
  return entries.map((entry) => `${t(entry.labelKey)}: ${entry.value}`).join(' · ')
}

function blockSummaryText(block) {
  if (block.type === 'tool_use') {
    const input = getValue(block, 'input') || {}
    const entries = collectSummaryEntries(input, ['path', 'file', 'target', 'command', 'query', 'pattern', 'url', 'endpoint'])
    return entries.length ? formatSummaryEntries(entries) : t('message.summaryToolInput')
  }

  if (block.type === 'tool_result') {
    const output = getValue(block, 'output')
    if (typeof output === 'string') return inlineSummaryText(output, 140) || t('message.summaryToolOutput')
    const entries = collectSummaryEntries(output || {}, [
      'path',
      'file',
      'target',
      'summary',
      'message',
      'preview',
      'stdout',
      'stderr',
      'url',
      'endpoint',
    ])
    const count = summaryCount(output)
    if (count !== null && entries.length < 3) {
      entries.push({ labelKey: 'message.summaryCount', value: String(count) })
    }
    return entries.length ? formatSummaryEntries(entries) : t('message.summaryToolOutput')
  }

  return null
}

function blockSummaryChips(block) {
  const chips = []

  if (block.type === 'tool_use') {
    const input = getValue(block, 'input') || {}
    const path = summarizeFieldValue(getValue(input, 'path', 'file', 'target'), 28)
    const command = summarizeFieldValue(getValue(input, 'command'), 28)
    const query = summarizeFieldValue(getValue(input, 'query', 'pattern'), 24)
    if (path) chips.push({ tone: 'neutral', text: `${t('message.summaryPath')}: ${path}` })
    if (command) chips.push({ tone: 'neutral', text: `${t('message.summaryCommand')}: ${command}` })
    if (query) chips.push({ tone: 'neutral', text: `${t('message.summaryQuery')}: ${query}` })
  }

  if (block.type === 'tool_result') {
    const output = getValue(block, 'output')
    const success = summarySuccess(output)
    const path = summarizeFieldValue(getValue(output, 'path', 'file', 'target'), 28)
    const count = summaryCount(output)
    const stored = summarizeFieldValue(getValue(output, 'stored_path', 'storedPath', 'preview_path', 'previewPath'), 24)

    if (success !== null) {
      chips.push({
        tone: success ? 'success' : 'danger',
        text: success ? t('message.success') : t('message.failed'),
      })
    }
    if (path) chips.push({ tone: 'neutral', text: `${t('message.summaryPath')}: ${path}` })
    if (count !== null) chips.push({ tone: 'neutral', text: `${t('message.summaryCount')}: ${count}` })
    if (stored) chips.push({ tone: 'success', text: t('message.summaryStored') })
  }

  return chips.slice(0, 4)
}

function looksLikeCodeContent(content, label = '', type = '') {
  const text = String(content || '')
  if (!text.trim()) return false
  if (String(type || '').toLowerCase() !== 'text') return true
  if (/```/.test(text)) return true
  if (/(json|diff|patch|bash|shell|command|tool|code)/i.test(String(label || ''))) return true
  const lines = text.split('\n')
  if (lines.length < 3) return false
  let score = 0
  if (/^\s{2,}\S/m.test(text)) score += 1
  if (/[{}[\]();<>=>]/.test(text)) score += 1
  if (/\b(function|const|let|var|class|def|fn|return|import|export|from|SELECT|INSERT|UPDATE|DELETE|CREATE|cargo|git|npm|pnpm|yarn|powershell)\b/i.test(text)) {
    score += 1
  }
  return score >= 2
}

function renderBlockViewerContent() {
  const content = String(state.blockViewer.content || '')
  const isCodeish = looksLikeCodeContent(content, state.blockViewer.title, state.blockViewer.kind)
  els.blockViewerContent.innerHTML = ''
  els.blockViewerContent.classList.toggle('is-code', isCodeish)
  els.blockViewerContent.classList.toggle('is-plain', !isCodeish)

  if (!isCodeish) {
    els.blockViewerContent.textContent = content
    return
  }

  const shell = document.createElement('div')
  shell.className = 'block-viewer-shell'

  const linesNode = document.createElement('div')
  linesNode.className = 'block-viewer-lines'

  content.split('\n').forEach((line, index) => {
    const lineNode = document.createElement('div')
    lineNode.className = 'block-viewer-line'

    const lineNumber = document.createElement('span')
    lineNumber.className = 'block-viewer-line-number'
    lineNumber.textContent = String(index + 1)

    const lineText = document.createElement('span')
    lineText.className = 'block-viewer-line-text'
    lineText.textContent = line || ' '

    lineNode.appendChild(lineNumber)
    lineNode.appendChild(lineText)
    linesNode.appendChild(lineNode)
  })

  shell.appendChild(linesNode)
  els.blockViewerContent.appendChild(shell)
}

function openBlockViewer({ title, content }) {
  const normalized = String(content || '')
  state.blockViewer = {
    title: title || 'block',
    content: normalized,
    kind: title || 'block',
    meta: t('message.viewerMeta', {
      type: title || 'block',
      chars: normalized.length,
      lines: lineCount(normalized),
    }),
  }
  els.blockViewerTitle.textContent = state.blockViewer.title
  els.blockViewerMeta.textContent = state.blockViewer.meta
  renderBlockViewerContent()
  els.blockViewerOverlay.classList.remove('is-hidden')
  els.blockViewer.classList.remove('is-hidden')
  els.blockViewer.setAttribute('aria-hidden', 'false')
}

function closeBlockViewer() {
  els.blockViewerOverlay.classList.add('is-hidden')
  els.blockViewer.classList.add('is-hidden')
  els.blockViewer.setAttribute('aria-hidden', 'true')
}

function refreshBlockViewer() {
  if (els.blockViewer.classList.contains('is-hidden')) return
  const content = state.blockViewer.content || ''
  els.blockViewerTitle.textContent = state.blockViewer.title || 'block'
  els.blockViewerMeta.textContent = t('message.viewerMeta', {
    type: state.blockViewer.title || 'block',
    chars: content.length,
    lines: lineCount(content),
  })
  renderBlockViewerContent()
}

function normalizeEvents(events) {
  const normalized = []
  ;(events || []).forEach((event) => {
    if (
      event?.type === 'assistant_text_delta' &&
      normalized.length &&
      normalized[normalized.length - 1].type === 'assistant_text_delta'
    ) {
      normalized[normalized.length - 1].text += getValue(event, 'text') || ''
      return
    }
    normalized.push({ ...event })
  })
  return normalized
}

function eventTitle(event) {
  switch (event.type) {
    case 'assistant_text_delta':
      return t('event.assistantOutput')
    case 'tool_call':
      return getValue(event, 'name') || t('event.toolCall')
    case 'tool_result':
      return getValue(event, 'tool_name', 'toolName') || t('event.toolResult')
    case 'usage':
      return t('event.tokenUsage')
    case 'message_stop':
      return t('event.messageStop')
    default:
      return event.type || 'event'
  }
}

function eventBody(event) {
  switch (event.type) {
    case 'assistant_text_delta':
      return (getValue(event, 'text') || '').trim() || 'Model is streaming text.'
    case 'tool_call':
      return JSON.stringify(getValue(event, 'input') || {}, null, 2)
    case 'tool_result': {
      const output = getValue(event, 'output')
      return typeof output === 'string' ? output : JSON.stringify(output || {}, null, 2)
    }
    case 'usage': {
      const usage = getValue(event, 'usage') || {}
      const inputTokens = Number(getValue(usage, 'input_tokens', 'inputTokens') || 0)
      const outputTokens = Number(getValue(usage, 'output_tokens', 'outputTokens') || 0)
      const cacheRead = Number(getValue(usage, 'cache_read_input_tokens', 'cacheReadInputTokens') || 0)
      const cacheCreate = Number(getValue(usage, 'cache_creation_input_tokens', 'cacheCreationInputTokens') || 0)
      return t('event.usage', {
        input: inputTokens,
        output: outputTokens,
        cacheRead,
        cacheCreate,
      })
    }
    case 'message_stop':
      return t('event.messageStopBody')
    default:
      return JSON.stringify(event, null, 2)
  }
}

// RENDER
function setView(view) {
  state.currentView = view
  els.chatView.classList.toggle('is-hidden', view !== 'chat')
  els.historyView.classList.toggle('is-hidden', view !== 'history')
  els.settingsView.classList.toggle('is-hidden', view !== 'settings')
  els.navButtons.forEach((button) => {
    button.classList.toggle('is-active', button.dataset.view === view)
  })
}

function setSettingsTab(tab) {
  state.settingsTab = tab
  els.settingsTabs.forEach((button) => {
    button.classList.toggle('is-active', button.dataset.settingsTab === tab)
  })
  els.settingsProviderPanel.classList.toggle('is-hidden', tab !== 'provider')
  els.settingsPermissionPanel.classList.toggle('is-hidden', tab !== 'permission')
  els.settingsSkillsPanel.classList.toggle('is-hidden', tab !== 'skills')
  els.settingsMcpPanel.classList.toggle('is-hidden', tab !== 'mcp')
  els.settingsEnvironmentPanel.classList.toggle('is-hidden', tab !== 'environment')
}

function setDrawerMode(mode) {
  state.drawerMode = mode
  const titles = {
    provider: t('drawer.editProvider'),
    skill: state.selectedSkillDetail
      ? t('drawer.editSkill', { name: state.selectedSkillDetail.name })
      : t('drawer.createSkill'),
    mcp: state.selectedMcpName
      ? t('drawer.editMcp', { name: state.selectedMcpName })
      : t('drawer.createMcp'),
  }
  els.drawerTitle.textContent = titles[mode]
  els.drawerProviderPanel.classList.toggle('is-hidden', mode !== 'provider')
  els.drawerSkillPanel.classList.toggle('is-hidden', mode !== 'skill')
  els.drawerMcpPanel.classList.toggle('is-hidden', mode !== 'mcp')
}

function openDrawer(mode) {
  setDrawerMode(mode)
  els.body.classList.add('is-drawer-open')
  els.drawerOverlay.classList.remove('is-hidden')
  els.settingsDrawer.classList.remove('is-hidden')
  els.settingsDrawer.setAttribute('aria-hidden', 'false')
  setDrawerStatus(t('drawer.ready'))
  queueMicrotask(() => {
    if (mode === 'provider') {
      els.providerProfileLabel.focus()
      return
    }
    if (mode === 'skill') {
      els.skillName.focus()
      return
    }
    if (mode === 'mcp') {
      els.mcpName.focus()
    }
  })
}

function closeDrawer() {
  els.body.classList.remove('is-drawer-open')
  els.drawerOverlay.classList.add('is-hidden')
  els.settingsDrawer.classList.add('is-hidden')
  els.settingsDrawer.setAttribute('aria-hidden', 'true')
}

function toggleSidebar() {
  state.sidebarCollapsed = !state.sidebarCollapsed
  els.body.classList.toggle('sidebar-collapsed', state.sidebarCollapsed)
  els.sidebar.classList.toggle('is-collapsed', state.sidebarCollapsed)
}

function teamMemoryLabel(teamMemory) {
  if (!teamMemory) return t('teamMemory.notConfigured')
  if (getValue(teamMemory, 'last_error')) return t('teamMemory.error')
  if (getValue(teamMemory, 'running')) {
    return getValue(teamMemory, 'pending_changes') ? t('teamMemory.syncing') : t('teamMemory.running')
  }
  return getValue(teamMemory, 'endpoint') ? t('teamMemory.configured') : t('teamMemory.notConfigured')
}

function teamMemoryDetail(teamMemory) {
  if (!teamMemory) return t('teamMemory.none')
  const lastError = getValue(teamMemory, 'last_error')
  if (lastError) return compactText(lastError, 52)
  const pulled = Number(getValue(teamMemory, 'files_pulled') || 0)
  const pushed = Number(getValue(teamMemory, 'files_pushed') || 0)
  return getValue(teamMemory, 'endpoint')
    ? t('teamMemory.stats', { pulled, pushed })
    : t('teamMemory.none')
}

function computeSessionMetrics(session) {
  const metrics = {
    messageCount: 0,
    toolBlockCount: 0,
    totalTokens: 0,
    hasMemory: false,
  }
  if (!session) return metrics

  metrics.messageCount = safeCount(session.messages)
  metrics.hasMemory = Boolean(sessionMemoryContent(session))

  ;(session.messages || []).forEach((message) => {
    ;(message.blocks || []).forEach((block) => {
      if (block.type === 'tool_use' || block.type === 'tool_result') {
        metrics.toolBlockCount += 1
      }
    })
    const usage = message.usage || {}
    metrics.totalTokens += Number(getValue(usage, 'input_tokens', 'inputTokens') || 0)
    metrics.totalTokens += Number(getValue(usage, 'output_tokens', 'outputTokens') || 0)
    metrics.totalTokens += Number(getValue(usage, 'cache_creation_input_tokens', 'cacheCreationInputTokens') || 0)
    metrics.totalTokens += Number(getValue(usage, 'cache_read_input_tokens', 'cacheReadInputTokens') || 0)
  })

  return metrics
}

function renderShellMeta() {
  const provider = state.bootstrap?.provider || {}
  els.runtimeModel.textContent = provider.model || t('status.modelFallback')
  els.runtimePermission.textContent = permissionModeLabel(state.bootstrap?.permissionMode || t('status.permissionFallback'))
  els.providerPersisted.textContent = provider.persisted ? t('status.providerSaved') : t('status.providerDefault')
  els.teamMemoryState.textContent = teamMemoryLabel(state.bootstrap?.teamMemorySync)
  els.settingsWorkspaceChip.textContent = compactText(state.bootstrap?.cwd || '-', 40)
  if (els.localeCurrentValue) {
    els.localeCurrentValue.textContent =
      state.locale === 'zh' ? t('locale.current.zh') : t('locale.current.en')
  }
}

function renderSettingsOverview() {
  const provider = state.bootstrap?.provider || {}
  const skills = state.bootstrap?.skills || []
  const mcpServers = state.bootstrap?.mcpServers || []
  const projectSkillCount = skills.filter((skill) => isProjectLocalSkill(skill)).length

  if (els.overviewProviderValue) {
    const providerLabel = provider.model || provider.name || '-'
    els.overviewProviderValue.textContent = compactText(providerLabel, 28)
    els.overviewProviderValue.title = providerLabel
    els.overviewProviderMeta.textContent = provider.persisted ? t('provider.savedState') : t('provider.defaultState')
  }
  if (els.overviewSkillsValue) {
    els.overviewSkillsValue.textContent = String(skills.length)
    els.overviewSkillsMeta.textContent = t('settings.overviewSkillsMeta', {
      project: projectSkillCount,
      total: skills.length,
    })
  }
  if (els.overviewMcpValue) {
    els.overviewMcpValue.textContent = String(mcpServers.length)
    els.overviewMcpMeta.textContent = t('settings.overviewMcpMeta', { count: mcpServers.length })
  }
  if (els.overviewLocaleValue) {
    els.overviewLocaleValue.textContent = state.locale === 'zh' ? t('locale.current.zh') : t('locale.current.en')
    els.overviewLocaleMeta.textContent = t('settings.overviewLocaleMeta')
  }
}

function permissionModeLabel(value) {
  switch (String(value || '').toLowerCase()) {
    case 'danger-full-access':
      return t('permission.mode.danger')
    case 'workspace-write':
      return t('permission.mode.workspace')
    case 'read-only':
      return t('permission.mode.readonly')
    default:
      return value || '-'
  }
}

function currentProviderProfiles() {
  return [...(state.bootstrap?.providerProfiles || [])].sort((left, right) => {
    if (left.active !== right.active) return left.active ? -1 : 1
    return String(left.label || '').localeCompare(String(right.label || ''))
  })
}

function selectedProviderProfile() {
  const profiles = state.bootstrap?.providerProfiles || []
  if (state.selectedProviderProfileId) {
    const matched = profiles.find((profile) => profile.id === state.selectedProviderProfileId)
    if (matched) return matched
  }
  return profiles.find((profile) => profile.active) || null
}

function renderSidebarSessions() {
  const sessions = sortedSessions(state.bootstrap?.sessions || [])
  const query = state.sessionFilter.trim().toLowerCase()
  const visible = query
    ? sessions.filter((session) => matchesSessionQuery(session, query))
    : sessions

  els.sessionList.innerHTML = ''
  if (!visible.length) {
    els.sessionList.innerHTML = query
      ? `<div class="sidebar-empty">${t('sidebar.noMatch')}</div>`
      : `<div class="sidebar-empty">${t('sidebar.empty')}</div>`
    return
  }

  visible.forEach((session) => {
    const row = document.createElement('div')
    row.className = `conversation-item${state.currentSessionId === session.id ? ' is-active' : ''}`

    const infoButton = document.createElement('button')
    infoButton.type = 'button'
    infoButton.className = 'conversation-main'
    infoButton.addEventListener('click', async () => {
      await loadSession(session.id)
      setView('chat')
    })

    const title = document.createElement('span')
    title.className = 'conversation-title'
    title.textContent = sessionDisplayTitle(session)
    title.title = session.id

    const preview = sessionDisplayPreview(session)
    const meta = document.createElement('span')
    meta.className = 'conversation-meta'
    meta.textContent = preview || t('history.messages', { count: session.messageCount })
    meta.title = preview || session.id

    const footer = document.createElement('div')
    footer.className = 'conversation-footer'

    const updated = document.createElement('span')
    updated.className = 'conversation-updated'
    updated.textContent = toLocaleTimestamp(session.updatedAtUnixMs)

    const count = document.createElement('span')
    count.className = 'conversation-count'
    count.textContent = t('history.messages', { count: session.messageCount })

    const statePill = document.createElement('span')
    const stateKey = sessionStateKey(session.id)
    statePill.className = `conversation-state conversation-state--${stateKey}`
    statePill.textContent = sessionStateLabel(session.id)

    footer.appendChild(updated)
    footer.appendChild(count)
    footer.appendChild(statePill)

    infoButton.appendChild(title)
    infoButton.appendChild(meta)
    infoButton.appendChild(footer)

    const deleteButton = document.createElement('button')
    deleteButton.type = 'button'
    deleteButton.className = 'conversation-delete'
    deleteButton.textContent = '×'
    deleteButton.title = t('message.deleteSession')
    deleteButton.addEventListener('click', async (event) => {
      event.stopPropagation()
      await deleteSession(session.id)
    })

    row.appendChild(infoButton)
    row.appendChild(deleteButton)
    els.sessionList.appendChild(row)
  })
}

function renderChatSummary() {
  const descriptor = currentSessionDescriptor()
  const activeMessages = buildVisibleMessages()
  const metrics = computeSessionMetrics({
    messages: activeMessages,
    currentSessionMemory: sessionMemoryContent(state.currentSession),
  })
  const hasSurface = Boolean(state.currentSessionId || state.currentSession || state.pendingTurn)
  const displayTitle = sessionDisplayTitle(descriptor, state.currentSession)
  const displayPreview = sessionDisplayPreview(descriptor, state.currentSession)

  els.summaryMessages.textContent = String(metrics.messageCount)
  els.summaryTools.textContent = String(metrics.toolBlockCount)
  els.summaryTokens.textContent = String(metrics.totalTokens)
  els.summaryMemory.textContent = metrics.hasMemory ? t('metric.loaded') : t('metric.notLoaded')

  if (!hasSurface) {
    els.chatEmptyState.classList.remove('is-hidden')
    els.sessionBanner.classList.add('is-hidden')
    els.metricsGrid.classList.add('is-hidden')
    els.messageToolbar.classList.add('is-hidden')
    if (els.sessionOverviewDrawer) {
      els.sessionOverviewDrawer.classList.add('is-hidden')
    }
    els.chatTitle.textContent = t('session.defaultTitle')
    els.chatSubtitle.textContent = t('session.defaultSubtitle')
    els.currentSessionChip.textContent = t('session.empty')
    els.currentSessionChip.title = ''
    els.sessionUpdatedChip.textContent = t('session.waiting')
    if (els.sessionCopyIdButton) els.sessionCopyIdButton.disabled = true
    return
  }

  els.chatEmptyState.classList.add('is-hidden')
  els.sessionBanner.classList.remove('is-hidden')
  els.metricsGrid.classList.remove('is-hidden')
  els.messageToolbar.classList.remove('is-hidden')
  els.chatTitle.textContent = displayTitle
  if (state.pendingTurn) {
    els.chatSubtitle.textContent = t(
      state.pendingTurn.phase === 'streaming' ? 'composer.streaming' : 'composer.waiting',
    )
  } else {
    els.chatSubtitle.textContent = displayPreview || t('session.subtitle', { count: metrics.messageCount })
  }
  els.currentSessionChip.textContent = displayTitle || t('session.waiting')
  els.currentSessionChip.title = state.currentSessionId || displayTitle || ''
  els.sessionUpdatedChip.textContent = state.pendingTurn
    ? t(state.pendingTurn.phase === 'streaming' ? 'composer.streaming' : 'composer.waiting')
    : descriptor
      ? t('session.updated', { time: toLocaleTimestamp(descriptor.updatedAtUnixMs) })
      : t('session.loaded')
  if (els.sessionCopyIdButton) els.sessionCopyIdButton.disabled = !state.currentSessionId
}

function renderWorkspaceMeta() {
  els.workspaceCwd.textContent = compactText(state.bootstrap?.cwd || '-')
  els.workspaceCwd.title = state.bootstrap?.cwd || ''
  els.workspaceSettings.textContent = compactText(state.bootstrap?.settingsFile || '-')
  els.workspaceSettings.title = state.bootstrap?.settingsFile || ''
  const detail = teamMemoryDetail(state.bootstrap?.teamMemorySync)
  els.teamMemoryDetail.textContent = detail
  els.teamMemoryDetail.title = detail
  const sessions = sortedSessions(state.bootstrap?.sessions || [])
  const query = state.sessionFilter.trim().toLowerCase()
  const visible = query
    ? sessions.filter((session) => matchesSessionQuery(session, query))
    : sessions
  els.sessionMeta.textContent = t('workspace.visible', { count: visible.length })
}

function renderTurnStats() {
  els.turnIterations.textContent = state.turnStats.iterations === null ? '-' : String(state.turnStats.iterations)
  els.turnPromptTokens.textContent =
    state.turnStats.estimatedPromptTokens === null ? '-' : String(state.turnStats.estimatedPromptTokens)
  els.turnCompacted.textContent =
    state.turnStats.compacted === null ? '-' : state.turnStats.compacted ? t('common.yes') : t('common.no')
}

function renderMessages() {
  const messages = buildVisibleMessages()
  const collapsibleKeys = []
  const previousBottomGap = els.messageList.scrollHeight - els.messageList.scrollTop - els.messageList.clientHeight
  const keepPinned = state.forceMessageScroll || previousBottomGap < 96
  els.messageList.innerHTML = ''
  els.messageCount.textContent = String(messages.length)

  if (!messages.length) {
    els.expandAllButton.disabled = true
    els.expandAllButton.textContent = t('conversation.expandAll')
    state.forceMessageScroll = false
    return
  }

  messages.forEach((message, messageIndex) => {
    const roleValue = String(message.role || 'assistant').toLowerCase()
    const isPending = Boolean(message.__pending)
    const row = document.createElement('div')
    row.className = `message-row message-row--${roleValue}`

    const avatar = document.createElement('div')
    avatar.className = `message-avatar message-avatar--${roleValue}`
    if (isPending) {
      avatar.classList.add('message-avatar--pending')
    }
    avatar.textContent = messageAvatarLabel(roleValue)

    const article = document.createElement('article')
    article.className = `message-card message-card--${roleValue}`
    if (isPending) {
      article.classList.add('message-card--pending')
    }

    const header = document.createElement('div')
    header.className = 'message-header'

    const heading = document.createElement('div')
    heading.className = 'message-heading'

    const role = document.createElement('div')
    role.className = 'message-role'
    role.textContent = messageRoleLabel(roleValue)

    const meta = document.createElement('div')
    meta.className = 'message-card-meta'
    if (isPending) {
      meta.textContent =
        message.__pendingState === 'user'
          ? t('message.pendingUser')
          : t(message.__pendingState === 'streaming' ? 'composer.streaming' : 'composer.waiting')
    } else {
      meta.textContent = usageSummary(message) || t('message.blocks', { count: safeCount(message.blocks) })
    }

    const actions = document.createElement('div')
    actions.className = 'message-actions'

    const copyButton = document.createElement('button')
    copyButton.type = 'button'
    copyButton.className = 'message-action-button'
    copyButton.textContent = t('message.copy')
    copyButton.addEventListener('click', async () => {
      try {
        await copyText(messagePlainText(message))
        setComposerStatus(t('composer.messageCopied'))
      } catch {
        setComposerStatus(t('composer.copyFailed'), true)
      }
    })

    heading.appendChild(role)
    heading.appendChild(meta)
    actions.appendChild(copyButton)
    header.appendChild(heading)
    header.appendChild(actions)
    article.appendChild(header)

    const body = document.createElement('div')
    body.className = 'message-body'

    ;(message.blocks || []).forEach((block, blockIndex) => {
      const content = blockContent(block)
      const label = blockLabel(block)
      const isTextBlock = block.type === 'text'
      const summary = blockSummaryText(block)
      const chips = blockSummaryChips(block)
      const key = `${messageIndex}:${blockIndex}`
      const isCodeish = looksLikeCodeContent(content, label, block.type)
      const collapseCandidate = shouldCollapseBlock(content)
      const isExpanded = state.expandedBlocks.has(key)
      const shouldShowTextHeader =
        !isTextBlock ||
        isCodeish ||
        collapseCandidate ||
        lineCount(content) > 5 ||
        String(content).length > 280 ||
        isPending
      if (collapseCandidate) collapsibleKeys.push(key)

      const blockNode = document.createElement('section')
      blockNode.className = `message-block message-block--${String(block.type || 'block').replace(/[^a-z0-9_-]+/gi, '-')}`
      if (isCodeish) {
        blockNode.classList.add('message-block--codeish')
      }
      if (isPending) {
        blockNode.classList.add('message-block--pending')
      }

      const blockHeader = document.createElement('div')
      blockHeader.className = 'message-block-header'

      const blockActions = document.createElement('div')
      blockActions.className = 'inline-actions'
      if (!isTextBlock) {
        const tag = document.createElement('span')
        tag.className = `message-block-tag message-block-tag--${String(block.type || 'block').replace(/[^a-z0-9_-]+/gi, '-')}`
        tag.textContent = label
        blockHeader.appendChild(tag)
      }
      if (!isTextBlock || shouldShowTextHeader) {
        const length = document.createElement('span')
        length.className = 'message-block-length'
        length.textContent = `${t('message.chars', { count: String(content).length })} / ${t('message.lines', { count: lineCount(content) })}`
        blockActions.appendChild(length)

        const inspectButton = document.createElement('button')
        inspectButton.type = 'button'
        inspectButton.className = 'inline-ghost'
        inspectButton.textContent = t('message.inspectBlock')
        inspectButton.addEventListener('click', () => {
          openBlockViewer({ title: label, content })
        })
        blockActions.appendChild(inspectButton)

        const copyBlockButton = document.createElement('button')
        copyBlockButton.type = 'button'
        copyBlockButton.className = 'inline-ghost'
        copyBlockButton.textContent = t('message.copyBlock')
        copyBlockButton.addEventListener('click', async () => {
          try {
            await copyText(content)
            setComposerStatus(t('composer.blockCopied'))
          } catch {
            setComposerStatus(t('composer.copyFailed'), true)
          }
        })
        blockActions.appendChild(copyBlockButton)

        if (collapseCandidate) {
          const button = document.createElement('button')
          button.type = 'button'
          button.className = 'inline-ghost'
          button.textContent = isExpanded ? t('common.collapse') : t('common.expand')
          button.addEventListener('click', () => {
            if (state.expandedBlocks.has(key)) {
              state.expandedBlocks.delete(key)
            } else {
              state.expandedBlocks.add(key)
            }
            renderMessages()
          })
          blockActions.appendChild(button)
        }
      }

      if (blockActions.childNodes.length) {
        blockHeader.appendChild(blockActions)
      }

      if (summary || chips.length) {
        const details = document.createElement('div')
        details.className = 'message-block-details'

        if (summary) {
          const summaryNode = document.createElement('p')
          summaryNode.className = 'message-block-summary'
          summaryNode.textContent = summary
          details.appendChild(summaryNode)
        }

        if (chips.length) {
          const chipRow = document.createElement('div')
          chipRow.className = 'message-block-chip-row'
          chips.forEach((chip) => {
            const chipNode = document.createElement('span')
            chipNode.className = `message-block-chip message-block-chip--${chip.tone || 'neutral'}`
            chipNode.textContent = chip.text
            chipRow.appendChild(chipNode)
          })
          details.appendChild(chipRow)
        }

        if (blockHeader.childNodes.length) {
          blockNode.appendChild(blockHeader)
        }
        blockNode.appendChild(details)
      } else if (blockHeader.childNodes.length) {
        blockNode.appendChild(blockHeader)
      }

      const contentNode = document.createElement('pre')
      contentNode.className = 'message-block-content'
      if (isCodeish) {
        contentNode.classList.add('message-block-content--codeish')
      }
      if (isPending) {
        contentNode.classList.add('message-block-content--pending')
      }
      if (collapseCandidate && !isExpanded) {
        contentNode.classList.add('is-collapsed')
      }
      contentNode.textContent = content

      blockNode.appendChild(contentNode)
      body.appendChild(blockNode)
    })

    article.appendChild(body)
    row.appendChild(avatar)
    row.appendChild(article)
    els.messageList.appendChild(row)
  })

  const allExpanded =
    collapsibleKeys.length > 0 && collapsibleKeys.every((key) => state.expandedBlocks.has(key))
  els.expandAllButton.disabled = collapsibleKeys.length === 0
  els.expandAllButton.textContent = allExpanded ? t('conversation.collapseAll') : t('conversation.expandAll')
  if (keepPinned) {
    els.messageList.scrollTop = els.messageList.scrollHeight
  }
  state.forceMessageScroll = false
}

function renderEvents() {
  const events = normalizeEvents(state.lastEvents)
  els.eventList.innerHTML = ''
  els.eventCount.textContent = String(events.length)
  els.turnEventTotal.textContent = String(events.length)

  if (!events.length) {
    const key = state.pendingTurn
      ? state.pendingTurn.phase === 'streaming'
        ? 'composer.streaming'
        : 'composer.waiting'
      : 'activity.empty'
    els.eventList.innerHTML = `<div class="pane-empty">${t(key)}</div>`
    return
  }

  events.forEach((event) => {
    const item = document.createElement('article')
    item.className = 'event-item'

    const titleRow = document.createElement('div')
    titleRow.className = 'event-title-row'

    const title = document.createElement('h4')
    title.className = 'event-title'
    title.textContent = eventTitle(event)

    const type = document.createElement('span')
    type.className = 'event-type'
    type.textContent = event.type || 'event'

    const body = document.createElement('pre')
    body.className = 'event-body'
    body.textContent = eventBody(event)

    titleRow.appendChild(title)
    titleRow.appendChild(type)
    item.appendChild(titleRow)
    item.appendChild(body)
    els.eventList.appendChild(item)
  })
}

function renderHistoryList() {
  const sessions = sortedSessions(state.bootstrap?.sessions || [])
  const query = state.historyFilter.trim().toLowerCase()
  const visible = query
    ? sessions.filter((session) => matchesSessionQuery(session, query))
    : sessions
  els.historyList.innerHTML = ''
  if (els.historyCountChip) {
    els.historyCountChip.textContent = t('history.count', {
      visible: visible.length,
      total: sessions.length,
    })
  }

  if (!visible.length) {
    els.historyList.innerHTML =
      `<div class="history-empty">${sessions.length ? t('sidebar.noMatch') : t('history.empty')}</div>`
    return
  }

  visible.forEach((session) => {
    const card = document.createElement('article')
    card.className = `history-card${state.currentSessionId === session.id ? ' is-active' : ''}`
    card.addEventListener('click', async () => {
      await loadSession(session.id)
      setView('chat')
    })

    const header = document.createElement('div')
    header.className = 'history-card-header'

    const title = document.createElement('div')
    title.className = 'history-card-title'

    const heading = document.createElement('strong')
    heading.textContent = sessionDisplayTitle(session)
    heading.title = session.id

    const titleMeta = document.createElement('span')
    titleMeta.textContent = session.id

    title.appendChild(heading)
    title.appendChild(titleMeta)

    const actions = document.createElement('div')
    actions.className = 'page-actions'

    const openButton = document.createElement('button')
    openButton.type = 'button'
    openButton.className = 'ghost-button'
    openButton.textContent = t('history.open')
    openButton.addEventListener('click', async (event) => {
      event.stopPropagation()
      await loadSession(session.id)
      setView('chat')
    })

    const copyIdButton = document.createElement('button')
    copyIdButton.type = 'button'
    copyIdButton.className = 'ghost-button'
    copyIdButton.textContent = t('history.copyId')
    copyIdButton.addEventListener('click', async (event) => {
      event.stopPropagation()
      try {
        await copyText(session.id)
        setComposerStatus(t('composer.sessionIdCopied'))
      } catch {
        setComposerStatus(t('composer.copyFailed'), true)
      }
    })

    const deleteButton = document.createElement('button')
    deleteButton.type = 'button'
    deleteButton.className = 'ghost-button ghost-button--danger'
    deleteButton.textContent = t('common.delete')
    deleteButton.addEventListener('click', async (event) => {
      event.stopPropagation()
      await deleteSession(session.id)
      renderHistoryList()
    })

    actions.appendChild(openButton)
    actions.appendChild(copyIdButton)
    actions.appendChild(deleteButton)
    header.appendChild(title)
    header.appendChild(actions)

    const body = document.createElement('div')
    body.className = 'history-card-body'

    const preview = sessionDisplayPreview(session)
    if (preview) {
      const previewNode = document.createElement('p')
      previewNode.className = 'history-card-preview'
      previewNode.textContent = preview
      previewNode.title = preview
      body.appendChild(previewNode)
    }

    const metaRow = document.createElement('div')
    metaRow.className = 'history-card-meta-row'

    const updatedChip = document.createElement('span')
    updatedChip.className = 'history-chip history-chip--time'
    updatedChip.textContent = toLocaleTimestamp(session.updatedAtUnixMs)

    const countChip = document.createElement('span')
    countChip.className = 'history-chip history-chip--count'
    countChip.textContent = t('history.messages', { count: session.messageCount })

    const stateChip = document.createElement('span')
    const stateKey = sessionStateKey(session.id)
    stateChip.className = `history-chip history-chip--${stateKey}`
    stateChip.textContent = sessionStateLabel(session.id)

    metaRow.appendChild(updatedChip)
    metaRow.appendChild(countChip)
    metaRow.appendChild(stateChip)
    body.appendChild(metaRow)

    card.appendChild(header)
    card.appendChild(body)
    els.historyList.appendChild(card)
  })
}

function renderProviderSummary() {
  const provider = state.bootstrap?.provider || {}
  els.providerSummaryPermission.textContent = permissionModeLabel(state.bootstrap?.permissionMode || '-')
  els.providerSummaryPersisted.textContent = provider.persisted ? t('provider.savedState') : t('provider.defaultState')
  els.providerSummarySessionCount.textContent = String(safeCount(state.bootstrap?.sessions))
  els.providerSummarySkillCount.textContent = String(safeCount(state.bootstrap?.skills))
  renderProviderFormNote()
}

function renderPermissionSummary() {
  const mode = state.bootstrap?.permissionMode || 'danger-full-access'
  if (els.permissionSummaryValue) {
    els.permissionSummaryValue.textContent = permissionModeLabel(mode)
  }
  if (els.permissionModeSelect) {
    els.permissionModeSelect.value = mode
  }
}

function renderProviderProfileEditor() {
  const profile = selectedProviderProfile()
  const runtimeProvider = state.bootstrap?.provider || {}
  const source = profile || {
    label: '',
    model: runtimeProvider.model || '',
    name: runtimeProvider.name || '',
    apiKey: '',
    baseUrl: runtimeProvider.baseUrl || '',
    baseUrlEnv: runtimeProvider.baseUrlEnv || '',
    timeoutMs: runtimeProvider.timeoutMs || 90000,
    active: true,
  }

  els.providerProfileLabel.value = source.label || ''
  els.providerModel.value = source.model || ''
  els.providerName.value = source.name || ''
  els.providerApiKey.value = ''
  els.providerBaseUrl.value = source.baseUrl || ''
  els.providerBaseUrlEnv.value = source.baseUrlEnv || ''
  els.providerTimeoutMs.value = source.timeoutMs || 90000
  els.providerProfileActivate.checked = profile ? Boolean(profile.active) : true
  if (els.providerClearApiKey) {
    els.providerClearApiKey.checked = false
  }
  els.deleteProviderProfileButton.disabled = !profile
  els.providerProfileState.textContent = profile
    ? t('providerProfiles.editing', { name: profile.label })
    : t('providerProfiles.created')
  state.providerApiKeyVisible = false
  renderProviderFormNote()
  renderProviderApiKeyState()
}

function renderProviderProfilesList() {
  const profiles = currentProviderProfiles()
  if (els.providerProfileCountChip) {
    els.providerProfileCountChip.textContent = t('providerProfiles.count', {
      visible: profiles.length,
      total: profiles.length,
    })
  }
  els.providerProfileList.innerHTML = ''
  if (!profiles.length) {
    els.providerProfileList.innerHTML = `<div class="settings-empty">${t('providerProfiles.empty')}</div>`
    return
  }

  profiles.forEach((profile) => {
    const card = document.createElement('article')
    card.className = `settings-list-card${state.selectedProviderProfileId === profile.id ? ' is-selected' : ''}`
    card.addEventListener('click', () => {
      state.selectedProviderProfileId = profile.id
      renderProviderProfileEditor()
      renderProviderProfilesList()
      openDrawer('provider')
    })

    const header = document.createElement('div')
    header.className = 'settings-list-header'

    const titleBlock = document.createElement('div')
    titleBlock.className = 'settings-list-copy'
    const title = document.createElement('h4')
    title.textContent = profile.label
    const description = document.createElement('p')
    description.textContent = `${profile.model} / ${profile.name}`
    titleBlock.appendChild(title)
    titleBlock.appendChild(description)

    const actions = document.createElement('div')
    actions.className = 'page-actions'

    const activateButton = document.createElement('button')
    activateButton.type = 'button'
    activateButton.className = 'ghost-button'
    activateButton.textContent = profile.active ? t('providerProfiles.active') : t('providerProfiles.activateButton')
    activateButton.disabled = profile.active
    activateButton.addEventListener('click', async (event) => {
      event.stopPropagation()
      await activateProviderProfile(profile.id)
    })

    const editButton = document.createElement('button')
    editButton.type = 'button'
    editButton.className = 'ghost-button'
    editButton.textContent = t('common.edit')
    editButton.addEventListener('click', (event) => {
      event.stopPropagation()
      state.selectedProviderProfileId = profile.id
      renderProviderProfileEditor()
      renderProviderProfilesList()
      openDrawer('provider')
    })

    actions.appendChild(activateButton)
    actions.appendChild(editButton)
    header.appendChild(titleBlock)
    header.appendChild(actions)

    const meta = document.createElement('div')
    meta.className = 'settings-chip-row'
    ;[
      profile.active ? t('providerProfiles.active') : t('providerProfiles.runtime'),
      compactText(profile.baseUrl, 32),
      `${profile.timeoutMs} ms`,
    ]
      .filter(Boolean)
      .forEach((value, index) => {
        const chip = document.createElement('span')
        chip.className = `status-tag${index === 0 && profile.active ? ' status-tag--accent' : ''}`
        chip.textContent = value
        meta.appendChild(chip)
      })

    card.appendChild(header)
    card.appendChild(meta)
    els.providerProfileList.appendChild(card)
  })
}

function renderSkillEditor() {
  const detail = state.selectedSkillDetail
  const editable = detail ? isProjectLocalSkill(detail) : true
  if (els.skillTemplate) {
    els.skillTemplate.value = 'blank'
  }
  if (els.applySkillTemplateButton) {
    els.applySkillTemplateButton.disabled = !editable
  }

  if (!detail) {
    els.skillEditorState.textContent = t('skills.created')
    els.skillForm.reset()
    els.deleteSkillButton.disabled = true
    ;[
      els.skillName,
      els.skillDescription,
      els.skillWhen,
      els.skillArgumentHint,
      els.skillTools,
      els.skillPaths,
      els.skillContext,
      els.skillVersion,
      els.skillAgent,
      els.skillModel,
      els.skillEffort,
      els.skillContent,
    ].forEach((field) => {
      field.disabled = false
    })
    renderSkillFormNote()
    return
  }

  els.skillName.value = detail.name || ''
  els.skillDescription.value = detail.description || ''
  els.skillWhen.value = detail.whenToUse || ''
  els.skillArgumentHint.value = detail.argumentHint || ''
  els.skillTools.value = (detail.allowedTools || []).join(', ')
  els.skillPaths.value = (detail.paths || []).join(', ')
  els.skillContext.value = detail.executionContext || ''
  els.skillVersion.value = detail.version || ''
  els.skillAgent.value = detail.agent || ''
  els.skillModel.value = detail.model || ''
  els.skillEffort.value = detail.effort || ''
  els.skillContent.value = detail.content || ''
  els.skillEditorState.textContent = editable
    ? t('skills.editing', { name: detail.name })
    : t('skills.readonlyNote', { name: detail.name })
  els.deleteSkillButton.disabled = !editable
  ;[
    els.skillName,
    els.skillDescription,
    els.skillWhen,
    els.skillArgumentHint,
    els.skillTools,
    els.skillPaths,
    els.skillContext,
    els.skillVersion,
    els.skillAgent,
    els.skillModel,
    els.skillEffort,
    els.skillContent,
  ].forEach((field) => {
    field.disabled = !editable
  })
  renderSkillFormNote()
}

function renderSkillsList() {
  const skills = [...(state.bootstrap?.skills || [])].sort((left, right) => {
    const leftLocal = isProjectLocalSkill(left) ? 0 : 1
    const rightLocal = isProjectLocalSkill(right) ? 0 : 1
    if (leftLocal !== rightLocal) return leftLocal - rightLocal
    return String(left.name || '').localeCompare(String(right.name || ''))
  })
  const query = state.skillFilter.trim().toLowerCase()
  const visible = skills.filter((skill) => {
    const matchesQuery = !query
      || String(skill.name || '').toLowerCase().includes(query)
      || String(skill.description || '').toLowerCase().includes(query)
      || String(skill.whenToUse || '').toLowerCase().includes(query)
    const matchesScope = !state.skillProjectOnly || isProjectLocalSkill(skill)
    return matchesQuery && matchesScope
  })
  els.skillList.innerHTML = ''
  if (els.skillCountChip) {
    els.skillCountChip.textContent = t('skills.count', {
      visible: visible.length,
      total: skills.length,
    })
  }

  if (!visible.length) {
    els.skillList.innerHTML =
      `<div class="settings-empty">${skills.length ? t('skills.noMatch') : t('skills.empty')}</div>`
    return
  }

  visible.forEach((skill) => {
    const slug = deriveSkillSlug(skill)
    const editable = isProjectLocalSkill(skill)
    const card = document.createElement('article')
    card.className = `settings-list-card${state.selectedSkillSlug === slug ? ' is-selected' : ''}`
    card.addEventListener('click', async () => {
      await selectSkill(slug)
      openDrawer('skill')
    })

    const header = document.createElement('div')
    header.className = 'settings-list-header'

    const titleBlock = document.createElement('div')
    titleBlock.className = 'settings-list-copy'

    const title = document.createElement('h4')
    title.textContent = skill.name

    const description = document.createElement('p')
    description.textContent = skill.description || skill.whenToUse || t('skills.none')

    titleBlock.appendChild(title)
    titleBlock.appendChild(description)

    const actions = document.createElement('div')
    actions.className = 'page-actions'

    const editButton = document.createElement('button')
    editButton.type = 'button'
    editButton.className = 'ghost-button'
    editButton.textContent = editable ? t('common.edit') : t('common.inspect')
    editButton.addEventListener('click', async (event) => {
      event.stopPropagation()
      await selectSkill(slug)
      openDrawer('skill')
    })

    actions.appendChild(editButton)
    header.appendChild(titleBlock)
    header.appendChild(actions)

    const meta = document.createElement('div')
    meta.className = 'settings-chip-row'
    ;[
      editable ? t('skills.project') : skill.origin || t('skills.readonly'),
      skill.executionContext,
      ...(skill.allowedTools || []).slice(0, 3),
    ]
      .filter(Boolean)
      .forEach((value) => {
        const chip = document.createElement('span')
        chip.className = 'status-tag'
        chip.textContent = value
        meta.appendChild(chip)
      })

    card.appendChild(header)
    card.appendChild(meta)
    els.skillList.appendChild(card)
  })
}

function renderMcpEditor() {
  const server = (state.bootstrap?.mcpServers || []).find((item) => item.name === state.selectedMcpName)
  if (els.mcpPreset) {
    els.mcpPreset.value = 'blank'
  }

  if (!server) {
    els.mcpEditorState.textContent = t('mcp.created')
    els.mcpForm.reset()
    els.mcpTransport.value = 'stdio'
    els.mcpAuthType.value = 'none'
    els.deleteMcpButton.disabled = true
    renderMcpFieldState()
    return
  }

  els.mcpName.value = server.name || ''
  els.mcpTransport.value = server.transport || 'stdio'
  els.mcpCommand.value = server.command || ''
  els.mcpArgs.value = (server.args || []).join(', ')
  els.mcpEndpoint.value = server.endpoint || ''
  els.mcpTimeoutMs.value = server.timeoutMs || ''
  els.mcpAuthType.value = server.authType || 'none'
  els.mcpTokenEnv.value = server.tokenEnv || ''
  els.mcpTokenPath.value = server.tokenPath || ''
  els.mcpEditorState.textContent = t('mcp.editing', { name: server.name })
  els.deleteMcpButton.disabled = false
  renderMcpFieldState()
}

function renderMcpList() {
  const servers = [...(state.bootstrap?.mcpServers || [])].sort((left, right) =>
    String(left.name || '').localeCompare(String(right.name || ''))
  )
  const query = state.mcpFilter.trim().toLowerCase()
  const visible = servers.filter((server) => {
    const haystack = [server.name, server.command, server.endpoint, server.transport, server.authType]
      .filter(Boolean)
      .join(' ')
      .toLowerCase()
    return !query || haystack.includes(query)
  })
  els.mcpList.innerHTML = ''
  if (els.mcpCountChip) {
    els.mcpCountChip.textContent = t('mcp.count', {
      visible: visible.length,
      total: servers.length,
    })
  }

  if (!visible.length) {
    els.mcpList.innerHTML =
      `<div class="settings-empty">${servers.length ? t('mcp.noMatch') : t('mcp.empty')}</div>`
    return
  }

  visible.forEach((server) => {
    const card = document.createElement('article')
    card.className = `settings-list-card${state.selectedMcpName === server.name ? ' is-selected' : ''}`
    card.addEventListener('click', () => {
      state.selectedMcpName = server.name
      renderMcpEditor()
      renderMcpList()
      openDrawer('mcp')
    })

    const header = document.createElement('div')
    header.className = 'settings-list-header'

    const titleBlock = document.createElement('div')
    titleBlock.className = 'settings-list-copy'

    const title = document.createElement('h4')
    title.textContent = server.name

    const description = document.createElement('p')
    description.textContent = server.command || server.endpoint || t('mcp.none')

    titleBlock.appendChild(title)
    titleBlock.appendChild(description)

    const actions = document.createElement('div')
    actions.className = 'page-actions'

    const editButton = document.createElement('button')
    editButton.type = 'button'
    editButton.className = 'ghost-button'
    editButton.textContent = t('common.edit')
    editButton.addEventListener('click', (event) => {
      event.stopPropagation()
      state.selectedMcpName = server.name
      renderMcpEditor()
      renderMcpList()
      openDrawer('mcp')
    })

    actions.appendChild(editButton)
    header.appendChild(titleBlock)
    header.appendChild(actions)

    const meta = document.createElement('div')
    meta.className = 'settings-chip-row'
    ;[server.transport, server.authType, server.timeoutMs ? `${server.timeoutMs} ms` : null]
      .filter(Boolean)
      .forEach((value) => {
        const chip = document.createElement('span')
        chip.className = 'status-tag'
        chip.textContent = value
        meta.appendChild(chip)
      })

    card.appendChild(header)
    card.appendChild(meta)
    els.mcpList.appendChild(card)
  })
}

function renderAll() {
  renderShellMeta()
  renderSidebarSessions()
  renderChatSummary()
  renderWorkspaceMeta()
  renderTurnStats()
  renderMessages()
  renderEvents()
  renderPaneState()
  renderHistoryList()
  renderProviderSummary()
  renderPermissionSummary()
  renderProviderProfileEditor()
  renderProviderProfilesList()
  renderSettingsOverview()
  renderProjectPaths()
  renderSkillEditor()
  renderSkillsList()
  renderMcpEditor()
  renderMcpList()
  updateComposerState()
  syncComposerHeight()
  updateSlashMenuFromComposer()
}

// DATA
async function loadBootstrap({ allowAutoSelect = false } = {}) {
  state.bootstrap = await request('/api/bootstrap')
  state.slashCatalog.loaded = false
  state.slashCatalog.tools = []

  const availableProviderProfileIds = new Set((state.bootstrap.providerProfiles || []).map((profile) => profile.id))
  if (
    state.selectedProviderProfileId
    && !availableProviderProfileIds.has(state.selectedProviderProfileId)
  ) {
    state.selectedProviderProfileId = null
  }
  if (!state.selectedProviderProfileId) {
    state.selectedProviderProfileId = state.bootstrap.activeProviderProfileId || null
  }

  const availableSkillSlugs = new Set((state.bootstrap.skills || []).map(deriveSkillSlug))
  if (state.selectedSkillSlug && !availableSkillSlugs.has(state.selectedSkillSlug)) {
    state.selectedSkillSlug = null
    state.selectedSkillDetail = null
  }

  const availableMcpNames = new Set((state.bootstrap.mcpServers || []).map((server) => server.name))
  if (state.selectedMcpName && !availableMcpNames.has(state.selectedMcpName)) {
    state.selectedMcpName = null
  }

  const sessions = state.bootstrap.sessions || []
  const stillExists = state.currentSessionId
    ? sessions.some((session) => session.id === state.currentSessionId)
    : false

  if (!stillExists) {
    state.currentSessionId = null
    state.currentSession = null
    state.lastEvents = []
  }

  if (state.currentSessionId && !state.currentSession) {
    await loadSession(state.currentSessionId, false)
    renderAll()
    return
  }

  if (allowAutoSelect && !state.currentSessionId && sessions.length) {
    await loadSession(sessions[0].id, false)
    renderAll()
    return
  }

  renderAll()
  if (!state.currentSessionId) {
    restoreComposerDraft(null)
  }
}

async function loadSession(sessionId, rerender = true) {
  state.currentSessionId = sessionId
  state.currentSession = await request(`/api/sessions/${encodeURIComponent(sessionId)}`)
  state.lastEvents = []
  state.forceMessageScroll = true
  closeBlockViewer()
  state.turnStats = {
    iterations: null,
    estimatedPromptTokens: null,
    compacted: null,
  }
  restoreComposerDraft(sessionId)
  if (rerender) renderAll()
}

// ACTIONS
async function submitChat(event) {
  event.preventDefault()
  if (state.sending) return

  const input = els.composerInput.value.trim()
  const previousDraftSessionId = state.currentSessionId
  if (!input) {
    setComposerStatus(t('composer.empty'), true)
    return
  }

  if (input.startsWith('/')) {
    await executeSlashInput(input)
    return
  }

  state.sending = true
  state.pendingTurn = {
    userInput: input,
    assistantText: '',
    phase: 'waiting',
  }
  els.composerInput.value = ''
  clearComposerDraft(previousDraftSessionId)
  syncComposerHeight()
  state.forceMessageScroll = true
  renderConversationSurface()
  setComposerStatus(t('composer.waiting'))

  try {
    const response = await request('/api/chat', {
      method: 'POST',
      body: JSON.stringify({
        sessionId: state.currentSessionId,
        input,
        model: els.providerModel.value.trim() || undefined,
      }),
    })

    state.lastEvents = response.events || []
    state.turnStats = {
      iterations: response.iterations ?? null,
      estimatedPromptTokens: response.estimatedPromptTokens ?? null,
      compacted: response.compacted ?? null,
    }
    if (state.pendingTurn) {
      state.pendingTurn.phase = 'streaming'
      renderConversationSurface()
      setComposerStatus(t('composer.streaming'))
      for (const segment of buildAssistantReplaySegments(response.events || [], response.session)) {
        if (!state.pendingTurn) break
        state.pendingTurn.assistantText += segment
        state.forceMessageScroll = true
        renderConversationSurface()
        await wait(segment.includes('\n') ? 24 : 14)
      }
    }

    state.currentSessionId = response.sessionId
    state.currentSession = response.session
    state.pendingTurn = null
    state.forceMessageScroll = true
    state.expandedBlocks.clear()
    renderConversationSurface()

    await loadBootstrap({ allowAutoSelect: false })
    setView('chat')
    setComposerStatus(t('composer.done', {
      iterations: response.iterations,
      tokens: response.estimatedPromptTokens,
    }))
  } catch (error) {
    if (state.pendingTurn) {
      els.composerInput.value = state.pendingTurn.userInput
      persistComposerDraft()
      syncComposerHeight()
    }
    state.pendingTurn = null
    renderConversationSurface()
    setComposerStatus(error.message || t('composer.sendFailed'), true)
  } finally {
    state.sending = false
    updateComposerState()
  }
}

function startNewSession() {
  state.currentSessionId = null
  state.currentSession = null
  state.lastEvents = []
  state.forceMessageScroll = true
  closeBlockViewer()
  state.turnStats = {
    iterations: null,
    estimatedPromptTokens: null,
    compacted: null,
  }
  state.expandedBlocks.clear()
  restoreComposerDraft(null)
  renderAll()
  setComposerStatus(t('composer.newSessionReady'))
  setView('chat')
  queueMicrotask(() => els.composerInput.focus())
}

async function copyCurrentSessionId() {
  if (!state.currentSessionId) return
  try {
    await copyText(state.currentSessionId)
    setComposerStatus(t('composer.sessionIdCopied'))
  } catch {
    setComposerStatus(t('composer.copyFailed'), true)
  }
}

async function copyProjectPath(kind) {
  const value = kind === 'skills' ? projectSkillDirectory() : state.bootstrap?.settingsFile || ''
  if (!value) {
    setComposerStatus(t('settings.noPath'), true)
    return
  }
  try {
    await copyText(value)
    setComposerStatus(t('composer.pathCopied'))
  } catch {
    setComposerStatus(t('composer.copyFailed'), true)
  }
}

function applySelectedSkillTemplate() {
  if (!els.skillTemplate) return
  const template = skillTemplates()[els.skillTemplate.value] || skillTemplates().blank
  const hasContent = els.skillContent.value.trim().length > 0
  const willReplace = hasContent && els.skillContent.value !== (template.content || '')
  if (willReplace && !window.confirm(t('skill.templateConfirm'))) {
    return
  }

  if (!els.skillDescription.value.trim() && template.description) {
    els.skillDescription.value = template.description
  }
  if (!els.skillWhen.value.trim() && template.when) {
    els.skillWhen.value = template.when
  }
  els.skillContent.value = template.content || ''
  setDrawerStatus(t('skill.templateApplied'))
}

function applySelectedMcpPreset() {
  if (!els.mcpPreset) return
  const preset = mcpPresets()[els.mcpPreset.value] || mcpPresets().blank
  const hasContent = [
    els.mcpName.value,
    els.mcpCommand.value,
    els.mcpArgs.value,
    els.mcpEndpoint.value,
    els.mcpTokenEnv.value,
    els.mcpTokenPath.value,
  ].some((value) => String(value || '').trim().length > 0)
  if (hasContent && !window.confirm(t('mcp.presetConfirm'))) {
    return
  }

  els.mcpName.value = preset.name
  els.mcpTransport.value = preset.transport
  els.mcpCommand.value = preset.command
  els.mcpArgs.value = preset.args
  els.mcpEndpoint.value = preset.endpoint
  els.mcpTimeoutMs.value = preset.timeoutMs
  els.mcpAuthType.value = preset.authType
  els.mcpTokenEnv.value = preset.tokenEnv
  els.mcpTokenPath.value = preset.tokenPath
  renderMcpFieldState()
  setDrawerStatus(t('mcp.presetApplied'))
}

function clearComposerInput() {
  els.composerInput.value = ''
  clearComposerDraft()
  closeSlashMenu()
  syncComposerHeight()
  updateComposerState()
  els.composerInput.focus()
}

function toolByName(name) {
  return (state.slashCatalog.tools || []).find((tool) => tool.name === name) || null
}

async function showToolManifest(name = null) {
  await ensureSlashCatalogLoaded()
  if (name) {
    const tool = toolByName(name)
    if (!tool) {
      setComposerStatus(t('slash.unknown'), true)
      return
    }
    openBlockViewer({
      title: tool.name,
      content: `${tool.description}\n\nPermission: ${tool.permission}\nSource: ${tool.source}`,
    })
    return
  }
  const content = (state.slashCatalog.tools || [])
    .map((tool) => `- ${tool.name} [${tool.source}] (${tool.permission})\n  ${tool.description}`)
    .join('\n\n')
  openBlockViewer({
    title: '/tools',
    content: content || '-',
  })
}

async function openSkillFromSlash(slug) {
  setView('settings')
  setSettingsTab('skills')
  await selectSkill(slug)
  openDrawer('skill')
}

function openMcpFromSlash(name) {
  setView('settings')
  setSettingsTab('mcp')
  state.selectedMcpName = name
  renderMcpEditor()
  renderMcpList()
  openDrawer('mcp')
}

async function executeSlashInput(rawInput) {
  const input = String(rawInput || '').trim()
  if (!input.startsWith('/')) return false
  const [command, ...rest] = input.slice(1).split(/\s+/)
  const argument = rest.join(' ').trim()

  const finishSlash = (statusKeyOrText, vars = {}) => {
    els.composerInput.value = ''
    clearComposerDraft()
    closeSlashMenu()
    syncComposerHeight()
    updateComposerState()
    setComposerStatus(
      statusKeyOrText.includes('.') ? t(statusKeyOrText, vars) : statusKeyOrText,
    )
  }

  switch (command) {
    case 'help':
    case 'status':
    case 'compact':
    case 'permissions': {
      const response = await request('/api/slash', {
        method: 'POST',
        body: JSON.stringify({
          input,
          sessionId: state.currentSessionId,
        }),
      })
      openBlockViewer({
        title: response.title,
        content: response.output,
      })
      finishSlash('slash.executed', { command: input })
      return true
    }
    case 'new':
      finishSlash('slash.executed', { command: input })
      startNewSession()
      return true
    case 'history':
      finishSlash('slash.executed', { command: input })
      setView('history')
      return true
    case 'settings':
      finishSlash('slash.executed', { command: input })
      setView('settings')
      return true
    case 'provider':
      finishSlash('slash.executed', { command: input })
      setView('settings')
      setSettingsTab('provider')
      return true
    case 'skills':
      finishSlash('slash.executed', { command: input })
      setView('settings')
      setSettingsTab('skills')
      return true
    case 'mcp':
      if (argument) {
        const server = (state.bootstrap?.mcpServers || []).find((item) => item.name === argument)
        if (!server) {
          setComposerStatus(t('slash.unknown'), true)
          return true
        }
        finishSlash('slash.executed', { command: input })
        openMcpFromSlash(server.name)
        return true
      }
      finishSlash('slash.executed', { command: input })
      setView('settings')
      setSettingsTab('mcp')
      return true
    case 'tools':
      await showToolManifest()
      finishSlash('slash.executed', { command: input })
      return true
    case 'tool':
      await showToolManifest(argument)
      finishSlash('slash.executed', { command: input })
      return true
    case 'skill': {
      const target = (state.bootstrap?.skills || []).find((skill) => {
        const slug = deriveSkillSlug(skill)
        return slug === argument || skill.name === argument
      })
      if (!target) {
        setComposerStatus(t('slash.unknown'), true)
        return true
      }
      finishSlash('slash.executed', { command: input })
      await openSkillFromSlash(deriveSkillSlug(target))
      return true
    }
    default:
      setComposerStatus(t('slash.unknown'), true)
      return true
  }
}

async function executeSlashItem(item) {
  if (!item) return
  switch (item.action.type) {
    case 'builtin-command':
      await executeSlashInput(item.action.input)
      return
    case 'new-session':
      await executeSlashInput('/new')
      return
    case 'open-history':
      await executeSlashInput('/history')
      return
    case 'open-settings':
      await executeSlashInput('/settings')
      return
    case 'open-settings-tab':
      await executeSlashInput(`/${item.action.tab}`)
      return
    case 'show-tools':
      await executeSlashInput('/tools')
      return
    case 'open-skill':
      await executeSlashInput(`/skill ${item.action.slug}`)
      return
    case 'open-mcp':
      await executeSlashInput(`/mcp ${item.action.name}`)
      return
    case 'show-tool':
      await executeSlashInput(`/tool ${item.action.name}`)
      return
    default:
      setComposerStatus(t('slash.unknown'), true)
  }
}

async function deleteSession(sessionId) {
  const confirmed = window.confirm(t('confirm.deleteSession', { id: sessionId }))
  if (!confirmed) return

  try {
    const sessions = await request(`/api/sessions/${encodeURIComponent(sessionId)}`, {
      method: 'DELETE',
    })
    if (state.bootstrap) {
      state.bootstrap.sessions = sessions
    }
    if (state.currentSessionId === sessionId) {
      state.currentSessionId = null
      state.currentSession = null
      state.lastEvents = []
      state.turnStats = {
        iterations: null,
        estimatedPromptTokens: null,
        compacted: null,
      }
    }
    renderAll()
    setComposerStatus(t('composer.deletedSession', { id: sessionId }))
  } catch (error) {
    setComposerStatus(error.message || t('common.delete'), true)
  }
}

function toggleExpandAll() {
  const session = state.currentSession
  if (!session) return

  const keys = []
  ;(session.messages || []).forEach((message, messageIndex) => {
    ;(message.blocks || []).forEach((block, blockIndex) => {
      const key = `${messageIndex}:${blockIndex}`
      if (shouldCollapseBlock(blockContent(block))) keys.push(key)
    })
  })

  const allExpanded = keys.length > 0 && keys.every((key) => state.expandedBlocks.has(key))
  if (allExpanded) {
    keys.forEach((key) => state.expandedBlocks.delete(key))
  } else {
    keys.forEach((key) => state.expandedBlocks.add(key))
  }
  renderMessages()
}

async function submitProvider(event) {
  event.preventDefault()
  if (!els.providerProfileLabel.value.trim()) {
    setDrawerStatus(t('errors.providerProfileLabelRequired'), true)
    els.providerProfileLabel.focus()
    return
  }
  if (!els.providerModel.value.trim()) {
    setDrawerStatus(t('errors.providerModelRequired'), true)
    els.providerModel.focus()
    return
  }
  if (!els.providerName.value.trim()) {
    setDrawerStatus(t('errors.providerNameRequired'), true)
    els.providerName.focus()
    return
  }
  if (!els.providerBaseUrl.value.trim()) {
    setDrawerStatus(t('errors.providerBaseUrlRequired'), true)
    els.providerBaseUrl.focus()
    return
  }
  try {
    await request('/api/provider-profiles', {
      method: 'POST',
      body: JSON.stringify({
        id: state.selectedProviderProfileId,
        label: els.providerProfileLabel.value.trim(),
        model: els.providerModel.value.trim(),
        name: els.providerName.value.trim(),
        apiKey: els.providerApiKey.value.trim() || null,
        clearApiKey: Boolean(els.providerClearApiKey?.checked),
        baseUrl: els.providerBaseUrl.value.trim(),
        baseUrlEnv: els.providerBaseUrlEnv.value.trim() || null,
        timeoutMs: Number(els.providerTimeoutMs.value || 90000),
        activate: Boolean(els.providerProfileActivate.checked),
      }),
    })
    await loadBootstrap({ allowAutoSelect: false })
    const matchingProfile = (state.bootstrap.providerProfiles || []).find((profile) =>
      profile.label === els.providerProfileLabel.value.trim()
      && profile.model === els.providerModel.value.trim()
      && profile.name === els.providerName.value.trim()
    )
    state.selectedProviderProfileId =
      matchingProfile?.id
      || (els.providerProfileActivate.checked ? state.bootstrap.activeProviderProfileId : null)
      || state.selectedProviderProfileId
    renderProviderProfileEditor()
    renderProviderProfilesList()
    setDrawerStatus(t('providerProfiles.saved'))
  } catch (error) {
    setDrawerStatus(error.message || t('providerProfiles.save'), true)
  }
}

function newProviderProfile() {
  state.selectedProviderProfileId = null
  setSettingsTab('provider')
  renderProviderProfileEditor()
  renderProviderProfilesList()
  openDrawer('provider')
}

function resetProvider() {
  renderProviderProfileEditor()
  setDrawerStatus(t('common.reset'))
}

async function savePermissionMode() {
  try {
    await request('/api/permission', {
      method: 'POST',
      body: JSON.stringify({
        permissionMode: els.permissionModeSelect.value,
      }),
    })
    await loadBootstrap({ allowAutoSelect: false })
    setComposerStatus(t('permission.saved'))
  } catch (error) {
    setComposerStatus(error.message || t('permission.save'), true)
  }
}

async function activateProviderProfile(profileId) {
  try {
    await request(`/api/provider-profiles/${encodeURIComponent(profileId)}/activate`, {
      method: 'POST',
    })
    await loadBootstrap({ allowAutoSelect: false })
    state.selectedProviderProfileId = profileId
    setComposerStatus(t('providerProfiles.activated'))
  } catch (error) {
    setComposerStatus(error.message || t('providerProfiles.activateButton'), true)
  }
}

async function deleteProviderProfile() {
  const profile = selectedProviderProfile()
  if (!profile) return
  const confirmed = window.confirm(t('providerProfiles.deleteConfirm', { name: profile.label }))
  if (!confirmed) return

  try {
    await request(`/api/provider-profiles/${encodeURIComponent(profile.id)}`, {
      method: 'DELETE',
    })
    state.selectedProviderProfileId = null
    await loadBootstrap({ allowAutoSelect: false })
    closeDrawer()
    setComposerStatus(t('providerProfiles.deleted'))
  } catch (error) {
    setDrawerStatus(error.message || t('common.delete'), true)
  }
}

async function selectSkill(slug) {
  state.selectedSkillSlug = slug
  state.selectedSkillDetail = await request(`/api/skills/${encodeURIComponent(slug)}`)
  renderSkillEditor()
  renderSkillsList()
}

function newSkill() {
  state.selectedSkillSlug = null
  state.selectedSkillDetail = null
  renderSkillEditor()
  renderSkillsList()
  openDrawer('skill')
}

function resetSkillEditor() {
  if (state.selectedSkillDetail) {
    renderSkillEditor()
  } else {
    state.selectedSkillSlug = null
    state.selectedSkillDetail = null
    renderSkillEditor()
    renderSkillsList()
  }
  setDrawerStatus(t('skills.reset'))
}

async function submitSkill(event) {
  event.preventDefault()
  if (!els.skillName.value.trim()) {
    setDrawerStatus(t('errors.skillNameRequired'), true)
    els.skillName.focus()
    return
  }
  if (!els.skillContent.value.trim()) {
    setDrawerStatus(t('errors.skillContentRequired'), true)
    els.skillContent.focus()
    return
  }
  try {
    const nextSlug = state.selectedSkillSlug || slugify(els.skillName.value)
    const skills = await request('/api/skills', {
      method: 'POST',
      body: JSON.stringify({
        slug: state.selectedSkillSlug || null,
        name: els.skillName.value.trim(),
        description: els.skillDescription.value.trim() || null,
        whenToUse: els.skillWhen.value.trim() || null,
        argumentHint: els.skillArgumentHint.value.trim() || null,
        allowedTools: splitComma(els.skillTools.value),
        paths: splitComma(els.skillPaths.value),
        executionContext: els.skillContext.value || null,
        version: els.skillVersion.value.trim() || null,
        agent: els.skillAgent.value.trim() || null,
        model: els.skillModel.value.trim() || null,
        effort: els.skillEffort.value.trim() || null,
        content: els.skillContent.value.trim(),
      }),
    })
    if (state.bootstrap) state.bootstrap.skills = skills
    await loadBootstrap({ allowAutoSelect: false })
    await selectSkill(nextSlug)
    openDrawer('skill')
    setDrawerStatus(t('skills.saved'))
  } catch (error) {
    setDrawerStatus(error.message || t('skill.save'), true)
  }
}

async function deleteSkill() {
  if (!state.selectedSkillSlug || !state.selectedSkillDetail || !isProjectLocalSkill(state.selectedSkillDetail)) {
    return
  }

  const confirmed = window.confirm(t('skills.deleteConfirm', { name: state.selectedSkillDetail.name }))
  if (!confirmed) return

  try {
    const skills = await request(`/api/skills/${encodeURIComponent(state.selectedSkillSlug)}`, {
      method: 'DELETE',
    })
    if (state.bootstrap) state.bootstrap.skills = skills
    state.selectedSkillSlug = null
    state.selectedSkillDetail = null
    renderSkillEditor()
    renderSkillsList()
    closeDrawer()
    setComposerStatus(t('skills.deleted'))
  } catch (error) {
    setDrawerStatus(error.message || t('common.delete'), true)
  }
}

function newMcp() {
  state.selectedMcpName = null
  renderMcpEditor()
  renderMcpList()
  openDrawer('mcp')
}

function resetMcpEditor() {
  renderMcpEditor()
  setDrawerStatus(t('mcp.reset'))
}

async function submitMcp(event) {
  event.preventDefault()
  const transport = els.mcpTransport.value
  const authType = els.mcpAuthType.value
  if (!els.mcpName.value.trim()) {
    setDrawerStatus(t('errors.mcpNameRequired'), true)
    els.mcpName.focus()
    return
  }
  if (!isNetworkTransport(transport) && !els.mcpCommand.value.trim()) {
    setDrawerStatus(t('errors.mcpCommandRequired'), true)
    els.mcpCommand.focus()
    return
  }
  if (isNetworkTransport(transport) && !els.mcpEndpoint.value.trim()) {
    setDrawerStatus(t('errors.mcpEndpointRequired'), true)
    els.mcpEndpoint.focus()
    return
  }
  if (authType === 'bearer-env' && !els.mcpTokenEnv.value.trim()) {
    setDrawerStatus(t('errors.mcpTokenEnvRequired'), true)
    els.mcpTokenEnv.focus()
    return
  }
  if (authType === 'bearer-file' && !els.mcpTokenPath.value.trim()) {
    setDrawerStatus(t('errors.mcpTokenPathRequired'), true)
    els.mcpTokenPath.focus()
    return
  }
  try {
    const nextName = els.mcpName.value.trim()
    const servers = await request('/api/mcp', {
      method: 'POST',
      body: JSON.stringify({
        originalName: state.selectedMcpName || null,
        name: nextName,
        transport: els.mcpTransport.value,
        command: els.mcpCommand.value.trim() || null,
        args: splitComma(els.mcpArgs.value),
        endpoint: els.mcpEndpoint.value.trim() || null,
        timeoutMs: els.mcpTimeoutMs.value ? Number(els.mcpTimeoutMs.value) : null,
        authType: els.mcpAuthType.value,
        tokenEnv: els.mcpTokenEnv.value.trim() || null,
        tokenPath: els.mcpTokenPath.value.trim() || null,
      }),
    })
    if (state.bootstrap) state.bootstrap.mcpServers = servers
    state.selectedMcpName = nextName
    await loadBootstrap({ allowAutoSelect: false })
    openDrawer('mcp')
    setDrawerStatus(t('mcp.saved'))
  } catch (error) {
    setDrawerStatus(error.message || t('mcp.save'), true)
  }
}

async function deleteMcp() {
  if (!state.selectedMcpName) return
  const confirmed = window.confirm(t('mcp.deleteConfirm', { name: state.selectedMcpName }))
  if (!confirmed) return

  try {
    const servers = await request(`/api/mcp/${encodeURIComponent(state.selectedMcpName)}`, {
      method: 'DELETE',
    })
    if (state.bootstrap) state.bootstrap.mcpServers = servers
    state.selectedMcpName = null
    renderMcpEditor()
    renderMcpList()
    closeDrawer()
    setComposerStatus(t('mcp.deleted'))
  } catch (error) {
    setDrawerStatus(error.message || t('common.delete'), true)
  }
}

// EVENTS
els.sidebarToggleButton.addEventListener('click', toggleSidebar)
els.newSessionButton.addEventListener('click', startNewSession)
els.sessionNewButton.addEventListener('click', startNewSession)
els.sessionCopyIdButton.addEventListener('click', copyCurrentSessionId)
els.localeToggleButton.addEventListener('click', toggleLocale)
els.sessionOverviewLaunchButton.addEventListener('click', () => togglePane('overview'))
els.utilityLaunchButton.addEventListener('click', () => togglePane('utility'))
els.sessionOverviewToggle.addEventListener('click', () => togglePane('overview'))
els.utilityDrawerToggle.addEventListener('click', () => togglePane('utility'))
els.workspacePaneToggle.addEventListener('click', () => togglePane('workspace'))
els.activityPaneToggle.addEventListener('click', () => togglePane('activity'))
els.reloadSessionsButton.addEventListener('click', async () => {
  try {
    await loadBootstrap({ allowAutoSelect: state.currentView === 'chat' })
    setComposerStatus(t('composer.refreshDone'))
  } catch (error) {
    setComposerStatus(error.message || t('composer.refreshFailed'), true)
  }
})
els.sessionSearch.addEventListener('input', (event) => {
  state.sessionFilter = event.target.value || ''
  renderSidebarSessions()
  renderWorkspaceMeta()
})
els.navButtons.forEach((button) => {
  button.addEventListener('click', () => setView(button.dataset.view))
})
els.examplePills.forEach((button) => {
  button.addEventListener('click', () => {
    const promptKey = button.dataset.promptKey
    const prompt = promptKey ? t(promptKey) : button.dataset.prompt || ''
    els.composerInput.value = prompt
    persistComposerDraft()
    els.composerInput.focus()
    syncComposerHeight()
    updateComposerState()
    setComposerStatus(t('composer.exampleApplied'))
  })
})
els.expandAllButton.addEventListener('click', toggleExpandAll)
els.composerForm.addEventListener('submit', submitChat)
els.composerInput.addEventListener('input', () => {
  persistComposerDraft()
  syncComposerHeight()
  updateComposerState()
  updateSlashMenuFromComposer()
})
els.clearInputButton.addEventListener('click', clearComposerInput)
els.composerInput.addEventListener('keydown', (event) => {
  if (event.isComposing) {
    return
  }
  if (state.slashMenu.open) {
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      moveSlashSelection(1)
      return
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault()
      moveSlashSelection(-1)
      return
    }
    if (event.key === 'Escape') {
      event.preventDefault()
      closeSlashMenu()
      renderSlashMenu()
      return
    }
    if (event.key === 'Tab' && state.slashMenu.items.length) {
      event.preventDefault()
      executeSlashItem(state.slashMenu.items[state.slashMenu.activeIndex])
      return
    }
  }
  if (
    event.key === 'Enter'
    && !event.ctrlKey
    && !event.metaKey
    && !event.shiftKey
    && !event.altKey
  ) {
    event.preventDefault()
    if (state.slashMenu.open) {
      const current = slashQuery()
      if (current.trim() !== '/' || !state.slashMenu.items.length) {
        els.composerForm.requestSubmit()
      } else {
        executeSlashItem(state.slashMenu.items[state.slashMenu.activeIndex])
      }
      return
    }
    els.composerForm.requestSubmit()
  }
})
els.providerApiKeyVisibilityButton.addEventListener('click', () => {
  state.providerApiKeyVisible = !state.providerApiKeyVisible
  renderProviderApiKeyState()
})
els.providerApiKey.addEventListener('input', () => {
  if (els.providerApiKey.value.trim() && els.providerClearApiKey?.checked) {
    els.providerClearApiKey.checked = false
  }
  renderProviderApiKeyState()
})
els.providerClearApiKey.addEventListener('change', () => {
  if (els.providerClearApiKey.checked) {
    els.providerApiKey.value = ''
  }
  renderProviderApiKeyState()
})
els.historyRefreshButton.addEventListener('click', async () => {
  await loadBootstrap({ allowAutoSelect: false })
})
els.historyNewSessionButton.addEventListener('click', startNewSession)
els.historySearch.addEventListener('input', (event) => {
  state.historyFilter = event.target.value || ''
  renderHistoryList()
})
els.skillSearch.addEventListener('input', (event) => {
  state.skillFilter = event.target.value || ''
  renderSkillsList()
})
els.skillProjectOnly.addEventListener('change', (event) => {
  state.skillProjectOnly = Boolean(event.target.checked)
  renderSkillsList()
})
els.mcpSearch.addEventListener('input', (event) => {
  state.mcpFilter = event.target.value || ''
  renderMcpList()
})
els.settingsTabs.forEach((button) => {
  button.addEventListener('click', () => setSettingsTab(button.dataset.settingsTab))
})
els.headerNewSkillButton.addEventListener('click', () => {
  setView('settings')
  setSettingsTab('skills')
  newSkill()
})
els.headerNewMcpButton.addEventListener('click', () => {
  setView('settings')
  setSettingsTab('mcp')
  newMcp()
})
els.newProviderProfileButton.addEventListener('click', newProviderProfile)
els.openProviderDrawerButton.addEventListener('click', () => {
  state.selectedProviderProfileId = state.bootstrap?.activeProviderProfileId || null
  setSettingsTab('provider')
  renderProviderProfileEditor()
  renderProviderProfilesList()
  openDrawer('provider')
})
els.savePermissionButton.addEventListener('click', savePermissionMode)
els.overviewProviderCard.addEventListener('click', () => {
  setView('settings')
  setSettingsTab('provider')
})
els.overviewSkillsCard.addEventListener('click', () => {
  setView('settings')
  setSettingsTab('skills')
  els.skillSearch.focus()
})
els.overviewMcpCard.addEventListener('click', () => {
  setView('settings')
  setSettingsTab('mcp')
  els.mcpSearch.focus()
})
els.overviewLocaleCard.addEventListener('click', () => {
  setView('settings')
  setSettingsTab('environment')
})
els.localeCardButton.addEventListener('click', toggleLocale)
els.copySettingsPathButton.addEventListener('click', () => copyProjectPath('config'))
els.copySkillsDirButton.addEventListener('click', () => copyProjectPath('skills'))
els.settingsNewSkillButton.addEventListener('click', () => {
  setSettingsTab('skills')
  newSkill()
})
els.drawerOverlay.addEventListener('click', closeDrawer)
els.closeDrawerButton.addEventListener('click', closeDrawer)
els.blockViewerOverlay.addEventListener('click', closeBlockViewer)
els.blockViewerCloseButton.addEventListener('click', closeBlockViewer)
document.addEventListener('pointerdown', (event) => {
  const target = event.target
  if (!(target instanceof Node)) return
  let changed = false
  if (
    !state.paneState.overview
    && !els.sessionOverviewDrawer?.contains(target)
    && !els.sessionOverviewLaunchButton?.contains(target)
  ) {
    state.paneState.overview = true
    changed = true
  }
  if (
    !state.paneState.utility
    && !els.utilityDrawer?.contains(target)
    && !els.utilityLaunchButton?.contains(target)
  ) {
    state.paneState.utility = true
    changed = true
  }
  if (changed) {
    persistPaneState()
    renderPaneState()
  }
})
els.blockViewerCopyButton.addEventListener('click', async () => {
  try {
    await copyText(state.blockViewer.content || '')
    setComposerStatus(t('composer.blockCopied'))
  } catch {
    setComposerStatus(t('composer.copyFailed'), true)
  }
})
els.providerForm.addEventListener('submit', submitProvider)
els.resetProviderButton.addEventListener('click', resetProvider)
els.deleteProviderProfileButton.addEventListener('click', deleteProviderProfile)
els.newSkillButton.addEventListener('click', newSkill)
els.applySkillTemplateButton.addEventListener('click', applySelectedSkillTemplate)
els.skillForm.addEventListener('submit', submitSkill)
els.resetSkillButton.addEventListener('click', resetSkillEditor)
els.deleteSkillButton.addEventListener('click', deleteSkill)
els.newMcpButton.addEventListener('click', newMcp)
els.applyMcpPresetButton.addEventListener('click', applySelectedMcpPreset)
els.mcpForm.addEventListener('submit', submitMcp)
els.mcpTransport.addEventListener('change', renderMcpFieldState)
els.mcpAuthType.addEventListener('change', renderMcpFieldState)
els.resetMcpButton.addEventListener('click', resetMcpEditor)
els.deleteMcpButton.addEventListener('click', deleteMcp)
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') {
    if (!els.blockViewer.classList.contains('is-hidden')) {
      closeBlockViewer()
      return
    }
    if (!els.settingsDrawer.classList.contains('is-hidden')) {
      closeDrawer()
    }
  }
})

setView(state.currentView)
setSettingsTab(state.settingsTab)
applyStaticLocale()
setDrawerMode('provider')
renderSkillEditor()
renderMcpEditor()

loadBootstrap({ allowAutoSelect: false }).catch((error) => {
  setComposerStatus(error.message || t('composer.refreshFailed'), true)
  setDrawerStatus(error.message || t('composer.refreshFailed'), true)
})
