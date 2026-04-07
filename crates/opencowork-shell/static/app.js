const state = {
  bootstrap: null,
  currentView: 'chat',
  currentSessionId: null,
  currentSession: null,
  sending: false,
  sessionFilter: '',
  lastEvents: [],
  expandedBlocks: new Set(),
  sidebarCollapsed: false,
  settingsTab: 'api',
  drawerMode: 'provider',
  selectedSkillSlug: null,
  selectedSkillDetail: null,
  selectedMcpName: null,
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
  sessionBanner: document.querySelector('#session-banner'),
  metricsGrid: document.querySelector('#metrics-grid'),
  messageToolbar: document.querySelector('#message-toolbar'),
  chatEmptyState: document.querySelector('#chat-empty-state'),
  chatTitle: document.querySelector('#chat-title'),
  chatSubtitle: document.querySelector('#chat-subtitle'),
  currentSessionChip: document.querySelector('#current-session-chip'),
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
  eventCount: document.querySelector('#event-count'),
  turnIterations: document.querySelector('#turn-iterations'),
  turnPromptTokens: document.querySelector('#turn-prompt-tokens'),
  turnCompacted: document.querySelector('#turn-compacted'),
  turnEventTotal: document.querySelector('#turn-event-total'),
  eventList: document.querySelector('#event-list'),
  composerForm: document.querySelector('#composer-form'),
  composerInput: document.querySelector('#composer-input'),
  composerStatus: document.querySelector('#composer-status'),
  sendButton: document.querySelector('#send-button'),
  historyRefreshButton: document.querySelector('#history-refresh-button'),
  historyNewSessionButton: document.querySelector('#history-new-session-button'),
  historyList: document.querySelector('#history-list'),
  settingsWorkspaceChip: document.querySelector('#settings-workspace-chip'),
  settingsTabs: Array.from(document.querySelectorAll('.settings-tab')),
  settingsApiPanel: document.querySelector('#settings-api-panel'),
  settingsSkillsPanel: document.querySelector('#settings-skills-panel'),
  settingsMcpPanel: document.querySelector('#settings-mcp-panel'),
  openProviderDrawerButton: document.querySelector('#open-provider-drawer-button'),
  apiEditButton: document.querySelector('#api-edit-button'),
  providerSummaryModel: document.querySelector('#provider-summary-model'),
  providerSummaryName: document.querySelector('#provider-summary-name'),
  providerSummaryBaseUrl: document.querySelector('#provider-summary-base-url'),
  providerSummaryTimeout: document.querySelector('#provider-summary-timeout'),
  providerSummaryPermission: document.querySelector('#provider-summary-permission'),
  providerSummaryPersisted: document.querySelector('#provider-summary-persisted'),
  providerSummarySessionCount: document.querySelector('#provider-summary-session-count'),
  providerSummarySkillCount: document.querySelector('#provider-summary-skill-count'),
  skillList: document.querySelector('#skill-list'),
  mcpList: document.querySelector('#mcp-list'),
  newSkillButton: document.querySelector('#new-skill-button'),
  newMcpButton: document.querySelector('#new-mcp-button'),
  drawerOverlay: document.querySelector('#drawer-overlay'),
  settingsDrawer: document.querySelector('#settings-drawer'),
  closeDrawerButton: document.querySelector('#close-drawer-button'),
  drawerTitle: document.querySelector('#drawer-title'),
  drawerStatus: document.querySelector('#drawer-status'),
  drawerProviderPanel: document.querySelector('#drawer-provider-panel'),
  drawerSkillPanel: document.querySelector('#drawer-skill-panel'),
  drawerMcpPanel: document.querySelector('#drawer-mcp-panel'),
  providerForm: document.querySelector('#provider-form'),
  providerModel: document.querySelector('#provider-model'),
  providerName: document.querySelector('#provider-name'),
  providerApiKeyEnv: document.querySelector('#provider-api-key-env'),
  providerBaseUrl: document.querySelector('#provider-base-url'),
  providerBaseUrlEnv: document.querySelector('#provider-base-url-env'),
  providerTimeoutMs: document.querySelector('#provider-timeout-ms'),
  resetProviderButton: document.querySelector('#reset-provider-button'),
  skillEditorState: document.querySelector('#skill-editor-state'),
  skillForm: document.querySelector('#skill-form'),
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
  mcpForm: document.querySelector('#mcp-form'),
  mcpName: document.querySelector('#mcp-name'),
  mcpTransport: document.querySelector('#mcp-transport'),
  mcpCommand: document.querySelector('#mcp-command'),
  mcpArgs: document.querySelector('#mcp-args'),
  mcpEndpoint: document.querySelector('#mcp-endpoint'),
  mcpTimeoutMs: document.querySelector('#mcp-timeout-ms'),
  mcpAuthType: document.querySelector('#mcp-auth-type'),
  mcpTokenEnv: document.querySelector('#mcp-token-env'),
  mcpTokenPath: document.querySelector('#mcp-token-path'),
  resetMcpButton: document.querySelector('#reset-mcp-button'),
  deleteMcpButton: document.querySelector('#delete-mcp-button'),
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

function toLocaleTimestamp(value) {
  if (!value) return 'Just now'
  return new Date(Number(value)).toLocaleString('zh-CN', {
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

function setComposerStatus(message, isError = false) {
  els.composerStatus.textContent = message
  els.composerStatus.dataset.tone = isError ? 'error' : 'default'
}

function setDrawerStatus(message, isError = false) {
  els.drawerStatus.textContent = message
  els.drawerStatus.dataset.tone = isError ? 'error' : 'default'
}

function shouldCollapseBlock(content) {
  const text = String(content || '')
  return text.length > 500 || text.split('\n').length > 12
}

function blockLabel(block) {
  switch (block.type) {
    case 'text':
      return 'text'
    case 'tool_use':
      return `tool_use / ${getValue(block, 'name') || 'unknown'}`
    case 'tool_result':
      return `tool_result / ${getValue(block, 'tool_name', 'toolName') || 'unknown'}`
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
      return 'Assistant Output'
    case 'tool_call':
      return getValue(event, 'name') || 'Tool Call'
    case 'tool_result':
      return getValue(event, 'tool_name', 'toolName') || 'Tool Result'
    case 'usage':
      return 'Token Usage'
    case 'message_stop':
      return 'Message Stop'
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
      return `input ${inputTokens} / output ${outputTokens} / cache read ${cacheRead} / cache create ${cacheCreate}`
    }
    case 'message_stop':
      return 'Current assistant message finished.'
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
  els.settingsApiPanel.classList.toggle('is-hidden', tab !== 'api')
  els.settingsSkillsPanel.classList.toggle('is-hidden', tab !== 'skills')
  els.settingsMcpPanel.classList.toggle('is-hidden', tab !== 'mcp')
}

function setDrawerMode(mode) {
  state.drawerMode = mode
  const titles = {
    provider: 'Edit Provider',
    skill: state.selectedSkillDetail ? `Edit ${state.selectedSkillDetail.name}` : 'Create Skill',
    mcp: state.selectedMcpName ? `Edit ${state.selectedMcpName}` : 'Create MCP Server',
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
  if (!teamMemory) return 'Not configured'
  if (getValue(teamMemory, 'last_error')) return 'Error'
  if (getValue(teamMemory, 'running')) {
    return getValue(teamMemory, 'pending_changes') ? 'Syncing' : 'Running'
  }
  return getValue(teamMemory, 'endpoint') ? 'Configured' : 'Not configured'
}

function teamMemoryDetail(teamMemory) {
  if (!teamMemory) return 'No activity'
  const lastError = getValue(teamMemory, 'last_error')
  if (lastError) return compactText(lastError, 52)
  const pulled = Number(getValue(teamMemory, 'files_pulled') || 0)
  const pushed = Number(getValue(teamMemory, 'files_pushed') || 0)
  return getValue(teamMemory, 'endpoint') ? `pull ${pulled} / push ${pushed}` : 'No activity'
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
  metrics.hasMemory = Boolean(session.currentSessionMemory)

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
  els.runtimeModel.textContent = provider.model || 'model'
  els.runtimePermission.textContent = state.bootstrap?.permissionMode || 'permission'
  els.providerPersisted.textContent = provider.persisted ? 'provider: saved' : 'provider: default'
  els.teamMemoryState.textContent = teamMemoryLabel(state.bootstrap?.teamMemorySync)
  els.settingsWorkspaceChip.textContent = compactText(state.bootstrap?.cwd || '-', 40)
}

function renderSidebarSessions() {
  const sessions = state.bootstrap?.sessions || []
  const query = state.sessionFilter.trim().toLowerCase()
  const visible = query
    ? sessions.filter((session) => session.id.toLowerCase().includes(query))
    : sessions

  els.sessionList.innerHTML = ''
  if (!visible.length) {
    els.sessionList.innerHTML = query
      ? '<div class="sidebar-empty">No sessions match the current filter.</div>'
      : '<div class="sidebar-empty">No saved sessions yet.</div>'
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
    title.textContent = session.id

    const meta = document.createElement('span')
    meta.className = 'conversation-meta'
    meta.textContent = `${session.messageCount} msgs / ${toLocaleTimestamp(session.updatedAtUnixMs)}`

    infoButton.appendChild(title)
    infoButton.appendChild(meta)

    const deleteButton = document.createElement('button')
    deleteButton.type = 'button'
    deleteButton.className = 'conversation-delete'
    deleteButton.textContent = '×'
    deleteButton.title = 'Delete session'
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
  const metrics = computeSessionMetrics(state.currentSession)

  els.summaryMessages.textContent = String(metrics.messageCount)
  els.summaryTools.textContent = String(metrics.toolBlockCount)
  els.summaryTokens.textContent = String(metrics.totalTokens)
  els.summaryMemory.textContent = metrics.hasMemory ? 'Loaded' : 'Not loaded'

  if (!state.currentSessionId || !state.currentSession) {
    els.chatEmptyState.classList.remove('is-hidden')
    els.sessionBanner.classList.add('is-hidden')
    els.metricsGrid.classList.add('is-hidden')
    els.messageToolbar.classList.add('is-hidden')
    els.chatTitle.textContent = 'OpenClaw Session'
    els.chatSubtitle.textContent = 'Start a new session or load one from the sidebar.'
    els.currentSessionChip.textContent = 'No active session'
    els.sessionUpdatedChip.textContent = 'Waiting for first turn'
    return
  }

  els.chatEmptyState.classList.add('is-hidden')
  els.sessionBanner.classList.remove('is-hidden')
  els.metricsGrid.classList.remove('is-hidden')
  els.messageToolbar.classList.remove('is-hidden')
  els.chatTitle.textContent = `OpenClaw / ${state.currentSessionId}`
  els.chatSubtitle.textContent = `Continue the same backend session with ${metrics.messageCount} stored message(s).`
  els.currentSessionChip.textContent = state.currentSessionId
  els.sessionUpdatedChip.textContent = descriptor
    ? `Updated ${toLocaleTimestamp(descriptor.updatedAtUnixMs)}`
    : 'Loaded from current state'
}

function renderWorkspaceMeta() {
  els.workspaceCwd.textContent = compactText(state.bootstrap?.cwd || '-')
  els.workspaceCwd.title = state.bootstrap?.cwd || ''
  els.workspaceSettings.textContent = compactText(state.bootstrap?.settingsFile || '-')
  els.workspaceSettings.title = state.bootstrap?.settingsFile || ''
  const detail = teamMemoryDetail(state.bootstrap?.teamMemorySync)
  els.teamMemoryDetail.textContent = detail
  els.teamMemoryDetail.title = detail
  const sessions = state.bootstrap?.sessions || []
  const query = state.sessionFilter.trim().toLowerCase()
  const visible = query
    ? sessions.filter((session) => session.id.toLowerCase().includes(query))
    : sessions
  els.sessionMeta.textContent = `${visible.length} visible`
}

function renderTurnStats() {
  els.turnIterations.textContent = state.turnStats.iterations === null ? '-' : String(state.turnStats.iterations)
  els.turnPromptTokens.textContent =
    state.turnStats.estimatedPromptTokens === null ? '-' : String(state.turnStats.estimatedPromptTokens)
  els.turnCompacted.textContent =
    state.turnStats.compacted === null ? '-' : state.turnStats.compacted ? 'Yes' : 'No'
}

function renderMessages() {
  const messages = state.currentSession?.messages || []
  const collapsibleKeys = []
  els.messageList.innerHTML = ''
  els.messageCount.textContent = String(messages.length)

  if (!messages.length) {
    els.expandAllButton.disabled = true
    els.expandAllButton.textContent = 'Expand All'
    return
  }

  messages.forEach((message, messageIndex) => {
    const article = document.createElement('article')
    article.className = `message-card message-card--${String(message.role || '').toLowerCase()}`

    const header = document.createElement('div')
    header.className = 'message-header'

    const role = document.createElement('div')
    role.className = 'message-role'
    role.textContent = String(message.role || 'unknown')

    const meta = document.createElement('div')
    meta.className = 'message-card-meta'
    meta.textContent = `${safeCount(message.blocks)} blocks`

    header.appendChild(role)
    header.appendChild(meta)
    article.appendChild(header)

    const body = document.createElement('div')
    body.className = 'message-body'

    ;(message.blocks || []).forEach((block, blockIndex) => {
      const content = blockContent(block)
      const key = `${messageIndex}:${blockIndex}`
      const collapseCandidate = shouldCollapseBlock(content)
      const isExpanded = state.expandedBlocks.has(key)
      if (collapseCandidate) collapsibleKeys.push(key)

      const blockNode = document.createElement('section')
      blockNode.className = 'message-block'

      const blockHeader = document.createElement('div')
      blockHeader.className = 'message-block-header'

      const tag = document.createElement('span')
      tag.className = 'message-block-tag'
      tag.textContent = blockLabel(block)

      const actions = document.createElement('div')
      actions.className = 'inline-actions'

      const length = document.createElement('span')
      length.className = 'message-block-length'
      length.textContent = `${String(content).length} chars`
      actions.appendChild(length)

      if (collapseCandidate) {
        const button = document.createElement('button')
        button.type = 'button'
        button.className = 'inline-ghost'
        button.textContent = isExpanded ? 'Collapse' : 'Expand'
        button.addEventListener('click', () => {
          if (state.expandedBlocks.has(key)) {
            state.expandedBlocks.delete(key)
          } else {
            state.expandedBlocks.add(key)
          }
          renderMessages()
        })
        actions.appendChild(button)
      }

      blockHeader.appendChild(tag)
      blockHeader.appendChild(actions)

      const contentNode = document.createElement('pre')
      contentNode.className = 'message-block-content'
      if (collapseCandidate && !isExpanded) {
        contentNode.classList.add('is-collapsed')
      }
      contentNode.textContent = content

      blockNode.appendChild(blockHeader)
      blockNode.appendChild(contentNode)
      body.appendChild(blockNode)
    })

    article.appendChild(body)
    els.messageList.appendChild(article)
  })

  const allExpanded =
    collapsibleKeys.length > 0 && collapsibleKeys.every((key) => state.expandedBlocks.has(key))
  els.expandAllButton.disabled = collapsibleKeys.length === 0
  els.expandAllButton.textContent = allExpanded ? 'Collapse All' : 'Expand All'
  els.messageList.scrollTop = els.messageList.scrollHeight
}

function renderEvents() {
  const events = normalizeEvents(state.lastEvents)
  els.eventList.innerHTML = ''
  els.eventCount.textContent = String(events.length)
  els.turnEventTotal.textContent = String(events.length)

  if (!events.length) {
    els.eventList.innerHTML = '<div class="pane-empty">No turn activity yet.</div>'
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
  const sessions = state.bootstrap?.sessions || []
  els.historyList.innerHTML = ''

  if (!sessions.length) {
    els.historyList.innerHTML =
      '<div class="history-empty">No saved sessions yet. Start a conversation and it will appear here.</div>'
    return
  }

  sessions.forEach((session) => {
    const card = document.createElement('article')
    card.className = `history-card${state.currentSessionId === session.id ? ' is-active' : ''}`

    const header = document.createElement('div')
    header.className = 'history-card-header'

    const title = document.createElement('div')
    title.className = 'history-card-title'
    title.innerHTML = `<strong>${session.id}</strong><span>${session.messageCount} messages</span>`

    const actions = document.createElement('div')
    actions.className = 'page-actions'

    const openButton = document.createElement('button')
    openButton.type = 'button'
    openButton.className = 'ghost-button'
    openButton.textContent = 'Open'
    openButton.addEventListener('click', async () => {
      await loadSession(session.id)
      setView('chat')
    })

    const deleteButton = document.createElement('button')
    deleteButton.type = 'button'
    deleteButton.className = 'ghost-button ghost-button--danger'
    deleteButton.textContent = 'Delete'
    deleteButton.addEventListener('click', async () => {
      await deleteSession(session.id)
      renderHistoryList()
    })

    actions.appendChild(openButton)
    actions.appendChild(deleteButton)
    header.appendChild(title)
    header.appendChild(actions)

    const body = document.createElement('div')
    body.className = 'history-card-body'
    body.innerHTML = `
      <div><span>Updated</span><strong>${toLocaleTimestamp(session.updatedAtUnixMs)}</strong></div>
      <div><span>Status</span><strong>${state.currentSessionId === session.id ? 'Active' : 'Saved'}</strong></div>
    `

    card.appendChild(header)
    card.appendChild(body)
    els.historyList.appendChild(card)
  })
}

function renderProviderSummary() {
  const provider = state.bootstrap?.provider || {}
  els.providerModel.value = provider.model || ''
  els.providerName.value = provider.name || ''
  els.providerApiKeyEnv.value = provider.apiKeyEnv || ''
  els.providerBaseUrl.value = provider.baseUrl || ''
  els.providerBaseUrlEnv.value = provider.baseUrlEnv || ''
  els.providerTimeoutMs.value = provider.timeoutMs || 90000
  els.providerSummaryModel.textContent = provider.model || '-'
  els.providerSummaryName.textContent = provider.name || '-'
  els.providerSummaryBaseUrl.textContent = compactText(provider.baseUrl || '-')
  els.providerSummaryBaseUrl.title = provider.baseUrl || ''
  els.providerSummaryTimeout.textContent = provider.timeoutMs ? `${provider.timeoutMs} ms` : '-'
  els.providerSummaryPermission.textContent = state.bootstrap?.permissionMode || '-'
  els.providerSummaryPersisted.textContent = provider.persisted ? 'Saved in project' : 'Using defaults'
  els.providerSummarySessionCount.textContent = String(safeCount(state.bootstrap?.sessions))
  els.providerSummarySkillCount.textContent = String(safeCount(state.bootstrap?.skills))
}

function renderSkillEditor() {
  const detail = state.selectedSkillDetail
  const editable = detail ? isProjectLocalSkill(detail) : true

  if (!detail) {
    els.skillEditorState.textContent = 'Create a new project skill.'
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
    ? `Editing ${detail.name}.`
    : `${detail.name} is discovered from another location and is read-only here.`
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
}

function renderSkillsList() {
  const skills = state.bootstrap?.skills || []
  els.skillList.innerHTML = ''

  if (!skills.length) {
    els.skillList.innerHTML =
      '<div class="settings-empty">No skills yet. Create a project skill from the button above.</div>'
    return
  }

  skills.forEach((skill) => {
    const slug = deriveSkillSlug(skill)
    const editable = isProjectLocalSkill(skill)
    const card = document.createElement('article')
    card.className = `settings-list-card${state.selectedSkillSlug === slug ? ' is-selected' : ''}`

    const header = document.createElement('div')
    header.className = 'settings-list-header'

    const titleBlock = document.createElement('div')
    titleBlock.className = 'settings-list-copy'
    titleBlock.innerHTML = `
      <h4>${skill.name}</h4>
      <p>${skill.description || skill.whenToUse || 'No description.'}</p>
    `

    const actions = document.createElement('div')
    actions.className = 'page-actions'

    const editButton = document.createElement('button')
    editButton.type = 'button'
    editButton.className = 'ghost-button'
    editButton.textContent = editable ? 'Edit' : 'Inspect'
    editButton.addEventListener('click', async () => {
      await selectSkill(slug)
      openDrawer('skill')
    })

    actions.appendChild(editButton)
    header.appendChild(titleBlock)
    header.appendChild(actions)

    const meta = document.createElement('div')
    meta.className = 'settings-chip-row'
    ;[editable ? 'project' : skill.origin || 'readonly', skill.executionContext, ...(skill.allowedTools || []).slice(0, 3)]
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

  if (!server) {
    els.mcpEditorState.textContent = 'Create a new MCP server entry.'
    els.mcpForm.reset()
    els.mcpTransport.value = 'stdio'
    els.mcpAuthType.value = 'none'
    els.deleteMcpButton.disabled = true
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
  els.mcpEditorState.textContent = `Editing ${server.name}.`
  els.deleteMcpButton.disabled = false
}

function renderMcpList() {
  const servers = state.bootstrap?.mcpServers || []
  els.mcpList.innerHTML = ''

  if (!servers.length) {
    els.mcpList.innerHTML =
      '<div class="settings-empty">No MCP servers yet. Create one from the button above.</div>'
    return
  }

  servers.forEach((server) => {
    const card = document.createElement('article')
    card.className = `settings-list-card${state.selectedMcpName === server.name ? ' is-selected' : ''}`

    const header = document.createElement('div')
    header.className = 'settings-list-header'

    const titleBlock = document.createElement('div')
    titleBlock.className = 'settings-list-copy'
    titleBlock.innerHTML = `
      <h4>${server.name}</h4>
      <p>${server.command || server.endpoint || 'No command or endpoint.'}</p>
    `

    const actions = document.createElement('div')
    actions.className = 'page-actions'

    const editButton = document.createElement('button')
    editButton.type = 'button'
    editButton.className = 'ghost-button'
    editButton.textContent = 'Edit'
    editButton.addEventListener('click', () => {
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
  renderHistoryList()
  renderProviderSummary()
  renderSkillEditor()
  renderSkillsList()
  renderMcpEditor()
  renderMcpList()
}

// DATA
async function loadBootstrap({ allowAutoSelect = false } = {}) {
  state.bootstrap = await request('/api/bootstrap')

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
}

async function loadSession(sessionId, rerender = true) {
  state.currentSessionId = sessionId
  state.currentSession = await request(`/api/sessions/${encodeURIComponent(sessionId)}`)
  state.lastEvents = []
  state.turnStats = {
    iterations: null,
    estimatedPromptTokens: null,
    compacted: null,
  }
  if (rerender) renderAll()
}

// ACTIONS
async function submitChat(event) {
  event.preventDefault()
  if (state.sending) return

  const input = els.composerInput.value.trim()
  if (!input) {
    setComposerStatus('Enter some input first.', true)
    return
  }

  state.sending = true
  els.sendButton.disabled = true
  setComposerStatus('Calling the existing runtime ...')

  try {
    const response = await request('/api/chat', {
      method: 'POST',
      body: JSON.stringify({
        sessionId: state.currentSessionId,
        input,
        model: els.providerModel.value.trim() || undefined,
      }),
    })

    state.currentSessionId = response.sessionId
    state.currentSession = response.session
    state.lastEvents = response.events || []
    state.turnStats = {
      iterations: response.iterations ?? null,
      estimatedPromptTokens: response.estimatedPromptTokens ?? null,
      compacted: response.compacted ?? null,
    }
    state.expandedBlocks.clear()
    els.composerInput.value = ''

    await loadBootstrap({ allowAutoSelect: false })
    setView('chat')
    setComposerStatus(
      `Done. ${response.iterations} iteration(s), prompt estimate ${response.estimatedPromptTokens}.`,
    )
  } catch (error) {
    setComposerStatus(error.message || 'Send failed.', true)
  } finally {
    state.sending = false
    els.sendButton.disabled = false
  }
}

function startNewSession() {
  state.currentSessionId = null
  state.currentSession = null
  state.lastEvents = []
  state.turnStats = {
    iterations: null,
    estimatedPromptTokens: null,
    compacted: null,
  }
  state.expandedBlocks.clear()
  els.composerInput.value = ''
  renderAll()
  setComposerStatus('New session ready.')
  setView('chat')
}

async function deleteSession(sessionId) {
  const confirmed = window.confirm(`Delete session ${sessionId}?`)
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
    setComposerStatus(`Deleted ${sessionId}.`)
  } catch (error) {
    setComposerStatus(error.message || 'Deleting session failed.', true)
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
  try {
    const provider = await request('/api/provider', {
      method: 'POST',
      body: JSON.stringify({
        model: els.providerModel.value.trim() || null,
        name: els.providerName.value.trim(),
        apiKeyEnv: els.providerApiKeyEnv.value.trim(),
        baseUrl: els.providerBaseUrl.value.trim(),
        baseUrlEnv: els.providerBaseUrlEnv.value.trim() || null,
        timeoutMs: Number(els.providerTimeoutMs.value || 90000),
      }),
    })
    if (state.bootstrap) state.bootstrap.provider = provider
    renderShellMeta()
    renderProviderSummary()
    setDrawerStatus('Provider settings saved.')
  } catch (error) {
    setDrawerStatus(error.message || 'Saving provider failed.', true)
  }
}

async function resetProvider() {
  try {
    const provider = await request('/api/provider', { method: 'DELETE' })
    if (state.bootstrap) state.bootstrap.provider = provider
    renderShellMeta()
    renderProviderSummary()
    setDrawerStatus('Provider settings reset to defaults.')
  } catch (error) {
    setDrawerStatus(error.message || 'Resetting provider failed.', true)
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
  setDrawerStatus('Skill form reset.')
}

async function submitSkill(event) {
  event.preventDefault()
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
    setDrawerStatus('Skill saved.')
  } catch (error) {
    setDrawerStatus(error.message || 'Saving skill failed.', true)
  }
}

async function deleteSkill() {
  if (!state.selectedSkillSlug || !state.selectedSkillDetail || !isProjectLocalSkill(state.selectedSkillDetail)) {
    return
  }

  const confirmed = window.confirm(`Delete skill ${state.selectedSkillDetail.name}?`)
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
    setComposerStatus('Skill deleted.')
  } catch (error) {
    setDrawerStatus(error.message || 'Deleting skill failed.', true)
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
  setDrawerStatus('MCP form reset.')
}

async function submitMcp(event) {
  event.preventDefault()
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
    setDrawerStatus('MCP saved.')
  } catch (error) {
    setDrawerStatus(error.message || 'Saving MCP failed.', true)
  }
}

async function deleteMcp() {
  if (!state.selectedMcpName) return
  const confirmed = window.confirm(`Delete MCP server ${state.selectedMcpName}?`)
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
    setComposerStatus('MCP server deleted.')
  } catch (error) {
    setDrawerStatus(error.message || 'Deleting MCP failed.', true)
  }
}

// EVENTS
els.sidebarToggleButton.addEventListener('click', toggleSidebar)
els.newSessionButton.addEventListener('click', startNewSession)
els.reloadSessionsButton.addEventListener('click', async () => {
  try {
    await loadBootstrap({ allowAutoSelect: state.currentView === 'chat' })
    setComposerStatus('Session list refreshed.')
  } catch (error) {
    setComposerStatus(error.message || 'Refreshing failed.', true)
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
els.expandAllButton.addEventListener('click', toggleExpandAll)
els.composerForm.addEventListener('submit', submitChat)
els.composerInput.addEventListener('keydown', (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
    event.preventDefault()
    els.composerForm.requestSubmit()
  }
})
els.historyRefreshButton.addEventListener('click', async () => {
  await loadBootstrap({ allowAutoSelect: false })
})
els.historyNewSessionButton.addEventListener('click', startNewSession)
els.settingsTabs.forEach((button) => {
  button.addEventListener('click', () => setSettingsTab(button.dataset.settingsTab))
})
els.openProviderDrawerButton.addEventListener('click', () => openDrawer('provider'))
els.apiEditButton.addEventListener('click', () => openDrawer('provider'))
els.drawerOverlay.addEventListener('click', closeDrawer)
els.closeDrawerButton.addEventListener('click', closeDrawer)
els.providerForm.addEventListener('submit', submitProvider)
els.resetProviderButton.addEventListener('click', resetProvider)
els.newSkillButton.addEventListener('click', newSkill)
els.skillForm.addEventListener('submit', submitSkill)
els.resetSkillButton.addEventListener('click', resetSkillEditor)
els.deleteSkillButton.addEventListener('click', deleteSkill)
els.newMcpButton.addEventListener('click', newMcp)
els.mcpForm.addEventListener('submit', submitMcp)
els.resetMcpButton.addEventListener('click', resetMcpEditor)
els.deleteMcpButton.addEventListener('click', deleteMcp)

setView(state.currentView)
setSettingsTab(state.settingsTab)
setDrawerMode('provider')
renderSkillEditor()
renderMcpEditor()

loadBootstrap({ allowAutoSelect: false }).catch((error) => {
  setComposerStatus(error.message || 'Initialization failed.', true)
  setDrawerStatus(error.message || 'Initialization failed.', true)
})
