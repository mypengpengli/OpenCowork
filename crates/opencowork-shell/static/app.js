const state = {
  bootstrap: null,
  currentView: 'landing',
  currentSessionId: null,
  currentSession: null,
  currentTab: 'api',
  drawerOpen: false,
  sending: false,
  lastEvents: [],
  sessionFilter: '',
  expandedBlocks: new Set(),
  turnStats: {
    iterations: null,
    estimatedPromptTokens: null,
    compacted: null,
  },
  selectedSkillSlug: null,
  selectedSkillDetail: null,
  selectedMcpName: null,
}

const els = {
  body: document.body,
  landingView: document.querySelector('#landing-view'),
  workspaceView: document.querySelector('#workspace-view'),
  brandHomeButton: document.querySelector('#brand-home-button'),
  topbarModel: document.querySelector('#topbar-model'),
  topbarPermission: document.querySelector('#topbar-permission'),
  topbarProvider: document.querySelector('#topbar-provider'),
  topbarSettingsButton: document.querySelector('#topbar-settings-button'),
  topbarWorkspaceButton: document.querySelector('#topbar-workspace-button'),
  landingOpenWorkspaceButton: document.querySelector('#landing-open-workspace-button'),
  landingNewSessionButton: document.querySelector('#landing-new-session-button'),
  landingSettingsButton: document.querySelector('#landing-settings-button'),
  landingCopy: document.querySelector('#landing-copy'),
  landingSessionCount: document.querySelector('#landing-session-count'),
  landingWorkspaceCwd: document.querySelector('#landing-workspace-cwd'),
  landingWorkspaceSettings: document.querySelector('#landing-workspace-settings'),
  landingTeamMemoryState: document.querySelector('#landing-team-memory-state'),
  landingProviderPersisted: document.querySelector('#landing-provider-persisted'),
  landingProviderModel: document.querySelector('#landing-provider-model'),
  landingProviderName: document.querySelector('#landing-provider-name'),
  landingProviderBaseUrl: document.querySelector('#landing-provider-base-url'),
  landingSkillCount: document.querySelector('#landing-skill-count'),
  landingSkillsSummary: document.querySelector('#landing-skills-summary'),
  landingMcpSummary: document.querySelector('#landing-mcp-summary'),
  landingLastSession: document.querySelector('#landing-last-session'),
  workspaceCwd: document.querySelector('#workspace-cwd'),
  workspaceSettings: document.querySelector('#workspace-settings'),
  teamMemoryState: document.querySelector('#team-memory-state'),
  teamMemoryDetail: document.querySelector('#team-memory-detail'),
  sessionCountMeta: document.querySelector('#session-meta'),
  sessionSearch: document.querySelector('#session-search'),
  sessionList: document.querySelector('#session-list'),
  newSessionButton: document.querySelector('#new-session-button'),
  reloadSessionsButton: document.querySelector('#reload-sessions-button'),
  railSettingsButton: document.querySelector('#rail-settings-button'),
  chatTitle: document.querySelector('#chat-title'),
  chatSubtitle: document.querySelector('#chat-subtitle'),
  currentSessionChip: document.querySelector('#current-session-chip'),
  sessionUpdatedChip: document.querySelector('#session-updated-chip'),
  summaryMessages: document.querySelector('#summary-messages'),
  summaryTools: document.querySelector('#summary-tools'),
  summaryTokens: document.querySelector('#summary-tokens'),
  summaryMemory: document.querySelector('#summary-memory'),
  messageCount: document.querySelector('#message-count'),
  expandAllButton: document.querySelector('#expand-all-button'),
  messageList: document.querySelector('#message-list'),
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
  drawerOverlay: document.querySelector('#drawer-overlay'),
  settingsDrawer: document.querySelector('#settings-drawer'),
  drawerTitle: document.querySelector('#drawer-title'),
  drawerHomeButton: document.querySelector('#drawer-home-button'),
  closeDrawerButton: document.querySelector('#close-drawer-button'),
  drawerProviderModel: document.querySelector('#drawer-provider-model'),
  drawerSkillCount: document.querySelector('#drawer-skill-count'),
  drawerMcpCount: document.querySelector('#drawer-mcp-count'),
  drawerStatus: document.querySelector('#drawer-status'),
  tabButtons: Array.from(document.querySelectorAll('.tab-button')),
  tabPanels: Array.from(document.querySelectorAll('.tab-panel')),
  providerForm: document.querySelector('#provider-form'),
  providerModel: document.querySelector('#provider-model'),
  providerName: document.querySelector('#provider-name'),
  providerApiKeyEnv: document.querySelector('#provider-api-key-env'),
  providerBaseUrl: document.querySelector('#provider-base-url'),
  providerBaseUrlEnv: document.querySelector('#provider-base-url-env'),
  providerTimeoutMs: document.querySelector('#provider-timeout-ms'),
  resetProviderButton: document.querySelector('#reset-provider-button'),
  skillCount: document.querySelector('#skill-count'),
  newSkillButton: document.querySelector('#new-skill-button'),
  skillList: document.querySelector('#skill-list'),
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
  mcpCount: document.querySelector('#mcp-count'),
  newMcpButton: document.querySelector('#new-mcp-button'),
  mcpList: document.querySelector('#mcp-list'),
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

function compactText(value, maxLength = 58) {
  const text = String(value || '')
  if (!text) return '-'
  if (text.length <= maxLength) return text
  const head = Math.max(20, Math.floor(maxLength / 2) - 2)
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
  const sessions = state.bootstrap?.sessions || []
  return sessions.find((session) => session.id === state.currentSessionId) || null
}

function setComposerStatus(message, isError = false) {
  els.composerStatus.textContent = message
  els.composerStatus.dataset.tone = isError ? 'error' : 'default'
}

function setDrawerStatus(message, isError = false) {
  els.drawerStatus.textContent = message
  els.drawerStatus.dataset.tone = isError ? 'error' : 'default'
}

function setView(view) {
  state.currentView = view
  els.landingView.classList.toggle('is-hidden', view !== 'landing')
  els.workspaceView.classList.toggle('is-hidden', view !== 'workspace')
}

function setActiveTab(tab) {
  state.currentTab = tab
  const titles = {
    api: 'API Settings',
    skills: 'Skill Library',
    mcp: 'MCP Configuration',
  }
  els.drawerTitle.textContent = titles[tab] || 'Settings'

  els.tabButtons.forEach((button) => {
    button.classList.toggle('is-active', button.dataset.tab === tab)
  })

  els.tabPanels.forEach((panel) => {
    panel.classList.toggle('is-hidden', panel.dataset.panel !== tab)
  })
}

function openDrawer(tab = state.currentTab) {
  setActiveTab(tab)
  state.drawerOpen = true
  els.body.classList.add('is-drawer-open')
  els.drawerOverlay.classList.remove('is-hidden')
  els.settingsDrawer.classList.remove('is-hidden')
  els.settingsDrawer.setAttribute('aria-hidden', 'false')
}

function closeDrawer() {
  state.drawerOpen = false
  els.body.classList.remove('is-drawer-open')
  els.drawerOverlay.classList.add('is-hidden')
  els.settingsDrawer.classList.add('is-hidden')
  els.settingsDrawer.setAttribute('aria-hidden', 'true')
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
  if (lastError) return compactText(lastError, 48)
  const pulled = Number(getValue(teamMemory, 'files_pulled') || 0)
  const pushed = Number(getValue(teamMemory, 'files_pushed') || 0)
  return getValue(teamMemory, 'endpoint') ? `pull ${pulled} / push ${pushed}` : 'No activity'
}

function renderTopbar() {
  const provider = state.bootstrap?.provider
  els.topbarModel.textContent = provider?.model || 'model'
  els.topbarPermission.textContent = state.bootstrap?.permissionMode || 'permission'
  els.topbarProvider.textContent = provider?.persisted ? 'provider: saved' : 'provider: default'
}

function renderLanding() {
  const bootstrap = state.bootstrap
  if (!bootstrap) return

  const provider = bootstrap.provider || {}
  const sessions = bootstrap.sessions || []
  const skills = bootstrap.skills || []
  const servers = bootstrap.mcpServers || []
  const lastSession = sessions[0]

  els.landingSessionCount.textContent = String(sessions.length)
  els.landingWorkspaceCwd.textContent = compactText(bootstrap.cwd)
  els.landingWorkspaceCwd.title = bootstrap.cwd || ''
  els.landingWorkspaceSettings.textContent = compactText(bootstrap.settingsFile)
  els.landingWorkspaceSettings.title = bootstrap.settingsFile || ''
  els.landingTeamMemoryState.textContent = teamMemoryLabel(bootstrap.teamMemorySync)
  els.landingProviderPersisted.textContent = provider.persisted ? 'saved' : 'default'
  els.landingProviderModel.textContent = provider.model || '-'
  els.landingProviderName.textContent = provider.name || '-'
  els.landingProviderBaseUrl.textContent = compactText(provider.baseUrl)
  els.landingProviderBaseUrl.title = provider.baseUrl || ''
  els.landingSkillCount.textContent = String(skills.length)
  els.landingSkillsSummary.textContent = `${skills.length} installed`
  els.landingMcpSummary.textContent = `${servers.length} configured`
  els.landingLastSession.textContent = lastSession
    ? `${compactText(lastSession.id, 30)} / ${toLocaleTimestamp(lastSession.updatedAtUnixMs)}`
    : 'Waiting for first turn'
  els.landingCopy.textContent = `Use a calmer shell around the existing runtime. ${sessions.length} saved session(s), ${skills.length} skill(s) and ${servers.length} MCP server(s) are available.`
}

function renderWorkspaceMeta() {
  const bootstrap = state.bootstrap
  if (!bootstrap) return

  els.workspaceCwd.textContent = compactText(bootstrap.cwd)
  els.workspaceCwd.title = bootstrap.cwd || ''
  els.workspaceSettings.textContent = compactText(bootstrap.settingsFile)
  els.workspaceSettings.title = bootstrap.settingsFile || ''
  els.teamMemoryState.textContent = teamMemoryLabel(bootstrap.teamMemorySync)

  const detail = teamMemoryDetail(bootstrap.teamMemorySync)
  els.teamMemoryDetail.textContent = detail
  els.teamMemoryDetail.title = detail
}

function renderProvider() {
  const provider = state.bootstrap?.provider
  if (!provider) return

  els.providerModel.value = provider.model || ''
  els.providerName.value = provider.name || ''
  els.providerApiKeyEnv.value = provider.apiKeyEnv || ''
  els.providerBaseUrl.value = provider.baseUrl || ''
  els.providerBaseUrlEnv.value = provider.baseUrlEnv || ''
  els.providerTimeoutMs.value = provider.timeoutMs || 90000
  els.drawerProviderModel.textContent = provider.model || '-'
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
    metrics.totalTokens += Number(
      getValue(usage, 'cache_creation_input_tokens', 'cacheCreationInputTokens') || 0,
    )
    metrics.totalTokens += Number(
      getValue(usage, 'cache_read_input_tokens', 'cacheReadInputTokens') || 0,
    )
  })

  return metrics
}

function renderSessionSummary() {
  const descriptor = currentSessionDescriptor()
  const metrics = computeSessionMetrics(state.currentSession)

  els.summaryMessages.textContent = String(metrics.messageCount)
  els.summaryTools.textContent = String(metrics.toolBlockCount)
  els.summaryTokens.textContent = String(metrics.totalTokens)
  els.summaryMemory.textContent = metrics.hasMemory ? 'Loaded' : 'Not loaded'

  if (!state.currentSessionId || !state.currentSession) {
    els.chatTitle.textContent = 'OpenClaw Session'
    els.chatSubtitle.textContent =
      'Choose an existing session or start a fresh one. The backend orchestration stays untouched.'
    els.currentSessionChip.textContent = 'No active session'
    els.sessionUpdatedChip.textContent = 'Waiting for first turn'
    return
  }

  els.chatTitle.textContent = `OpenClaw / ${state.currentSessionId}`
  els.chatSubtitle.textContent = `Continue the same backend session with ${metrics.messageCount} message(s) already stored.`
  els.currentSessionChip.textContent = state.currentSessionId
  els.sessionUpdatedChip.textContent = descriptor
    ? `Updated ${toLocaleTimestamp(descriptor.updatedAtUnixMs)}`
    : 'Loaded from current state'
}

function renderTurnStats() {
  els.turnIterations.textContent =
    state.turnStats.iterations === null ? '-' : String(state.turnStats.iterations)
  els.turnPromptTokens.textContent =
    state.turnStats.estimatedPromptTokens === null
      ? '-'
      : String(state.turnStats.estimatedPromptTokens)
  els.turnCompacted.textContent =
    state.turnStats.compacted === null ? '-' : state.turnStats.compacted ? 'Yes' : 'No'
}

function renderDrawerSummary() {
  els.drawerProviderModel.textContent = state.bootstrap?.provider?.model || '-'
  els.drawerSkillCount.textContent = String(safeCount(state.bootstrap?.skills))
  els.drawerMcpCount.textContent = String(safeCount(state.bootstrap?.mcpServers))
}

function renderSessions() {
  const sessions = state.bootstrap?.sessions || []
  const query = state.sessionFilter.trim().toLowerCase()
  const visible = query
    ? sessions.filter((session) => session.id.toLowerCase().includes(query))
    : sessions

  els.sessionCountMeta.textContent = `${visible.length} visible`
  els.sessionList.innerHTML = ''

  if (!visible.length) {
    els.sessionList.innerHTML = query
      ? '<div class="empty-state">No sessions match the current filter.</div>'
      : '<div class="empty-state">No saved sessions yet. Start a new one from the landing page or workspace rail.</div>'
    return
  }

  visible.forEach((session) => {
    const button = document.createElement('button')
    button.type = 'button'
    button.className = `session-button${state.currentSessionId === session.id ? ' is-active' : ''}`

    const title = document.createElement('span')
    title.className = 'session-title'
    title.textContent = session.id

    const meta = document.createElement('span')
    meta.className = 'session-meta'
    meta.textContent = `${session.messageCount} messages / ${toLocaleTimestamp(session.updatedAtUnixMs)}`

    button.appendChild(title)
    button.appendChild(meta)
    button.addEventListener('click', async () => {
      await loadSession(session.id)
      setView('workspace')
    })
    els.sessionList.appendChild(button)
  })
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
  if (block.type === 'text') {
    return getValue(block, 'text') || ''
  }
  if (block.type === 'tool_use') {
    return JSON.stringify(getValue(block, 'input') || {}, null, 2)
  }
  if (block.type === 'tool_result') {
    const output = getValue(block, 'output')
    return typeof output === 'string' ? output : JSON.stringify(output || {}, null, 2)
  }
  return JSON.stringify(block, null, 2)
}

function renderMessages() {
  const messages = state.currentSession?.messages || []
  const collapsibleKeys = []
  els.messageList.innerHTML = ''
  els.messageCount.textContent = String(messages.length)

  if (!messages.length) {
    els.expandAllButton.disabled = true
    els.expandAllButton.textContent = 'Expand All'
    els.messageList.innerHTML =
      '<div class="empty-state">Messages for the selected session will appear here.</div>'
    return
  }

  messages.forEach((message, messageIndex) => {
    const card = document.createElement('article')
    card.className = 'message-card'
    card.dataset.role = String(message.role || '').toLowerCase()

    const header = document.createElement('div')
    header.className = 'message-header'

    const role = document.createElement('p')
    role.className = 'message-role'
    role.textContent = String(message.role || 'unknown')

    const blockCount = document.createElement('span')
    blockCount.className = 'message-block-count'
    blockCount.textContent = `${safeCount(message.blocks)} blocks`

    header.appendChild(role)
    header.appendChild(blockCount)
    card.appendChild(header)

    const body = document.createElement('div')
    body.className = 'message-body'

    ;(message.blocks || []).forEach((block, blockIndex) => {
      const blockNode = document.createElement('div')
      blockNode.className = 'message-block'

      const blockHeader = document.createElement('div')
      blockHeader.className = 'message-block-header'

      const tag = document.createElement('span')
      tag.className = 'message-block-tag'
      tag.textContent = blockLabel(block)

      const content = blockContent(block)
      const key = `${messageIndex}:${blockIndex}`
      const collapseCandidate = shouldCollapseBlock(content)
      const isExpanded = state.expandedBlocks.has(key)
      if (collapseCandidate) {
        collapsibleKeys.push(key)
      }

      const meta = document.createElement('span')
      meta.className = 'message-block-meta'
      meta.textContent = `${String(content).length} chars`

      const actions = document.createElement('div')
      actions.className = 'inline-actions'
      actions.appendChild(meta)

      if (collapseCandidate) {
        const toggle = document.createElement('button')
        toggle.type = 'button'
        toggle.className = 'message-expand-button'
        toggle.textContent = isExpanded ? 'Collapse' : 'Expand'
        toggle.addEventListener('click', () => {
          if (state.expandedBlocks.has(key)) {
            state.expandedBlocks.delete(key)
          } else {
            state.expandedBlocks.add(key)
          }
          renderMessages()
        })
        actions.appendChild(toggle)
      }

      blockHeader.appendChild(tag)
      blockHeader.appendChild(actions)
      blockNode.appendChild(blockHeader)

      const contentNode = document.createElement('div')
      contentNode.className = 'message-block-content'
      if (collapseCandidate && !isExpanded) {
        contentNode.classList.add('is-collapsed')
      }
      contentNode.textContent = content

      blockNode.appendChild(contentNode)
      body.appendChild(blockNode)
    })

    card.appendChild(body)
    els.messageList.appendChild(card)
  })

  const allExpanded =
    collapsibleKeys.length > 0 && collapsibleKeys.every((key) => state.expandedBlocks.has(key))
  els.expandAllButton.disabled = collapsibleKeys.length === 0
  els.expandAllButton.textContent = allExpanded ? 'Collapse All' : 'Expand All'
  els.messageList.scrollTop = els.messageList.scrollHeight
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
      const cacheRead = Number(
        getValue(usage, 'cache_read_input_tokens', 'cacheReadInputTokens') || 0,
      )
      const cacheCreate = Number(
        getValue(usage, 'cache_creation_input_tokens', 'cacheCreationInputTokens') || 0,
      )
      return `input ${inputTokens} / output ${outputTokens} / cache read ${cacheRead} / cache create ${cacheCreate}`
    }
    case 'message_stop':
      return 'Current assistant message finished.'
    default:
      return JSON.stringify(event, null, 2)
  }
}

function renderEvents() {
  const events = normalizeEvents(state.lastEvents)
  els.eventList.innerHTML = ''
  els.eventCount.textContent = String(events.length)
  els.turnEventTotal.textContent = String(events.length)

  if (!events.length) {
    els.eventList.innerHTML =
      '<div class="empty-state">Recent turn activity, tool calls and usage events will appear here.</div>'
    return
  }

  events.forEach((event) => {
    const card = document.createElement('article')
    card.className = 'event-card'

    const titleRow = document.createElement('div')
    titleRow.className = 'event-title-row'

    const title = document.createElement('h3')
    title.className = 'event-title'
    title.textContent = eventTitle(event)

    const type = document.createElement('span')
    type.className = 'event-type'
    type.textContent = event.type || 'event'

    titleRow.appendChild(title)
    titleRow.appendChild(type)

    const body = document.createElement('div')
    body.className = 'event-body'
    body.textContent = eventBody(event)

    card.appendChild(titleRow)
    card.appendChild(body)
    els.eventList.appendChild(card)
  })
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

function renderSkills() {
  const skills = state.bootstrap?.skills || []
  els.skillCount.textContent = String(skills.length)
  els.skillList.innerHTML = ''

  if (!skills.length) {
    els.skillList.innerHTML =
      '<div class="empty-state">No skills yet. Create a local project skill from the editor.</div>'
  } else {
    skills.forEach((skill) => {
      const slug = deriveSkillSlug(skill)
      const editable = isProjectLocalSkill(skill)
      const card = document.createElement('article')
      card.className = `mini-card selectable-card${state.selectedSkillSlug === slug ? ' is-selected' : ''}`

      const titleRow = document.createElement('div')
      titleRow.className = 'event-title-row'

      const title = document.createElement('h3')
      title.textContent = skill.name

      const badge = document.createElement('span')
      badge.className = 'event-type'
      badge.textContent = editable ? 'project' : skill.origin || 'readonly'

      titleRow.appendChild(title)
      titleRow.appendChild(badge)

      const description = document.createElement('p')
      description.textContent = skill.description || skill.whenToUse || 'No description.'

      const meta = document.createElement('div')
      meta.className = 'mini-meta'
      ;[skill.executionContext, skill.version, ...(skill.allowedTools || []).slice(0, 3)]
        .filter(Boolean)
        .forEach((value) => {
          const chip = document.createElement('span')
          chip.textContent = value
          meta.appendChild(chip)
        })

      const actionRow = document.createElement('div')
      actionRow.className = 'card-actions'
      const button = document.createElement('button')
      button.type = 'button'
      button.className = 'ghost-button ghost-button--small'
      button.textContent = editable ? 'Edit' : 'Inspect'
      button.addEventListener('click', async () => {
        await selectSkill(slug)
        openDrawer('skills')
      })
      actionRow.appendChild(button)

      card.appendChild(titleRow)
      card.appendChild(description)
      if (meta.childNodes.length) card.appendChild(meta)
      card.appendChild(actionRow)
      els.skillList.appendChild(card)
    })
  }

  renderSkillEditor()
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

function renderMcp() {
  const servers = state.bootstrap?.mcpServers || []
  els.mcpCount.textContent = String(servers.length)
  els.mcpList.innerHTML = ''

  if (!servers.length) {
    els.mcpList.innerHTML =
      '<div class="empty-state">No MCP servers yet. Add one from the editor.</div>'
  } else {
    servers.forEach((server) => {
      const card = document.createElement('article')
      card.className = `mini-card selectable-card${state.selectedMcpName === server.name ? ' is-selected' : ''}`

      const titleRow = document.createElement('div')
      titleRow.className = 'event-title-row'

      const title = document.createElement('h3')
      title.textContent = server.name

      const badge = document.createElement('span')
      badge.className = 'event-type'
      badge.textContent = server.transport || 'unknown'

      titleRow.appendChild(title)
      titleRow.appendChild(badge)

      const description = document.createElement('p')
      description.textContent = server.command || server.endpoint || 'No command or endpoint.'

      const meta = document.createElement('div')
      meta.className = 'mini-meta'
      ;[server.authType, server.timeoutMs ? `${server.timeoutMs} ms` : null]
        .filter(Boolean)
        .forEach((value) => {
          const chip = document.createElement('span')
          chip.textContent = value
          meta.appendChild(chip)
        })

      const actionRow = document.createElement('div')
      actionRow.className = 'card-actions'
      const button = document.createElement('button')
      button.type = 'button'
      button.className = 'ghost-button ghost-button--small'
      button.textContent = 'Edit'
      button.addEventListener('click', () => {
        state.selectedMcpName = server.name
        renderMcp()
        openDrawer('mcp')
      })
      actionRow.appendChild(button)

      card.appendChild(titleRow)
      card.appendChild(description)
      if (meta.childNodes.length) card.appendChild(meta)
      card.appendChild(actionRow)
      els.mcpList.appendChild(card)
    })
  }

  renderMcpEditor()
}

// RENDERERS
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

  renderTopbar()
  renderLanding()
  renderWorkspaceMeta()
  renderProvider()
  renderDrawerSummary()
  renderSkills()
  renderMcp()
  renderSessions()

  const sessions = state.bootstrap.sessions || []
  const stillExists = state.currentSessionId
    ? sessions.some((session) => session.id === state.currentSessionId)
    : false

  if (!stillExists) {
    state.currentSessionId = null
    state.currentSession = null
  }

  if (state.currentSessionId && !state.currentSession) {
    await loadSession(state.currentSessionId, false)
    return
  }

  if (allowAutoSelect && !state.currentSessionId && sessions.length) {
    await loadSession(sessions[0].id, false)
    return
  }

  renderSessionSummary()
  renderTurnStats()
  renderMessages()
  renderEvents()
}

async function loadSession(sessionId, rerenderSessions = true) {
  state.currentSessionId = sessionId
  state.currentSession = await request(`/api/sessions/${encodeURIComponent(sessionId)}`)
  state.lastEvents = []
  state.turnStats = {
    iterations: null,
    estimatedPromptTokens: null,
    compacted: null,
  }

  if (rerenderSessions) renderSessions()
  renderSessionSummary()
  renderTurnStats()
  renderMessages()
  renderEvents()
}

async function openWorkspace(startFresh = false) {
  setView('workspace')
  if (startFresh) {
    startNewSession()
    return
  }
  if (!state.currentSessionId) {
    const first = state.bootstrap?.sessions?.[0]
    if (first) {
      await loadSession(first.id, false)
      renderSessions()
    }
  }
}

function resetSkillEditor() {
  state.selectedSkillSlug = state.selectedSkillDetail ? state.selectedSkillSlug : null
  renderSkillEditor()
  setDrawerStatus('Skill form reset.')
}

async function selectSkill(slug) {
  state.selectedSkillSlug = slug
  state.selectedSkillDetail = await request(`/api/skills/${encodeURIComponent(slug)}`)
  renderSkills()
}

function newSkill() {
  state.selectedSkillSlug = null
  state.selectedSkillDetail = null
  renderSkills()
  openDrawer('skills')
}

function resetMcpEditor() {
  renderMcpEditor()
  setDrawerStatus('MCP form reset.')
}

function newMcp() {
  state.selectedMcpName = null
  renderMcp()
  openDrawer('mcp')
}

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
    setView('workspace')
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
    renderTopbar()
    renderLanding()
    renderProvider()
    renderDrawerSummary()
    setDrawerStatus('API settings saved.')
  } catch (error) {
    setDrawerStatus(error.message || 'Saving API settings failed.', true)
  }
}

async function resetProvider() {
  try {
    const provider = await request('/api/provider', { method: 'DELETE' })
    if (state.bootstrap) state.bootstrap.provider = provider
    renderTopbar()
    renderLanding()
    renderProvider()
    renderDrawerSummary()
    setDrawerStatus('Provider settings reset to defaults.')
  } catch (error) {
    setDrawerStatus(error.message || 'Resetting provider failed.', true)
  }
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
    openDrawer('skills')
    setDrawerStatus('Skill saved into .opencowork/skills.')
  } catch (error) {
    setDrawerStatus(error.message || 'Saving skill failed.', true)
  }
}

async function deleteSkill() {
  if (!state.selectedSkillSlug || !state.selectedSkillDetail || !isProjectLocalSkill(state.selectedSkillDetail)) {
    return
  }

  try {
    const skills = await request(`/api/skills/${encodeURIComponent(state.selectedSkillSlug)}`, {
      method: 'DELETE',
    })
    if (state.bootstrap) state.bootstrap.skills = skills
    state.selectedSkillSlug = null
    state.selectedSkillDetail = null
    await loadBootstrap({ allowAutoSelect: false })
    renderSkills()
    openDrawer('skills')
    setDrawerStatus('Skill deleted.')
  } catch (error) {
    setDrawerStatus(error.message || 'Deleting skill failed.', true)
  }
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
    setDrawerStatus('MCP settings saved.')
  } catch (error) {
    setDrawerStatus(error.message || 'Saving MCP failed.', true)
  }
}

async function deleteMcp() {
  if (!state.selectedMcpName) return

  try {
    const servers = await request(`/api/mcp/${encodeURIComponent(state.selectedMcpName)}`, {
      method: 'DELETE',
    })
    if (state.bootstrap) state.bootstrap.mcpServers = servers
    state.selectedMcpName = null
    await loadBootstrap({ allowAutoSelect: false })
    renderMcp()
    openDrawer('mcp')
    setDrawerStatus('MCP server deleted.')
  } catch (error) {
    setDrawerStatus(error.message || 'Deleting MCP failed.', true)
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
  renderSessions()
  renderSessionSummary()
  renderTurnStats()
  renderMessages()
  renderEvents()
  setComposerStatus('New session ready.')
  setView('workspace')
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

els.brandHomeButton.addEventListener('click', () => setView('landing'))
els.topbarWorkspaceButton.addEventListener('click', () => {
  openWorkspace(false).catch((error) => setComposerStatus(error.message || 'Loading workspace failed.', true))
})
els.topbarSettingsButton.addEventListener('click', () => openDrawer('api'))
els.landingOpenWorkspaceButton.addEventListener('click', () => {
  openWorkspace(false).catch((error) => setComposerStatus(error.message || 'Loading workspace failed.', true))
})
els.landingNewSessionButton.addEventListener('click', startNewSession)
els.landingSettingsButton.addEventListener('click', () => openDrawer('api'))
els.newSessionButton.addEventListener('click', startNewSession)
els.reloadSessionsButton.addEventListener('click', async () => {
  try {
    await loadBootstrap({ allowAutoSelect: state.currentView === 'workspace' })
    setComposerStatus('Session list refreshed.')
  } catch (error) {
    setComposerStatus(error.message || 'Refreshing failed.', true)
  }
})
els.railSettingsButton.addEventListener('click', () => openDrawer('api'))
els.expandAllButton.addEventListener('click', toggleExpandAll)
els.drawerOverlay.addEventListener('click', closeDrawer)
els.closeDrawerButton.addEventListener('click', closeDrawer)
els.drawerHomeButton.addEventListener('click', () => setView('landing'))
els.sessionSearch.addEventListener('input', (event) => {
  state.sessionFilter = event.target.value || ''
  renderSessions()
})
els.composerForm.addEventListener('submit', submitChat)
els.composerInput.addEventListener('keydown', (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
    event.preventDefault()
    els.composerForm.requestSubmit()
  }
})
els.providerForm.addEventListener('submit', submitProvider)
els.resetProviderButton.addEventListener('click', resetProvider)
els.skillForm.addEventListener('submit', submitSkill)
els.newSkillButton.addEventListener('click', newSkill)
els.resetSkillButton.addEventListener('click', resetSkillEditor)
els.deleteSkillButton.addEventListener('click', deleteSkill)
els.mcpForm.addEventListener('submit', submitMcp)
els.newMcpButton.addEventListener('click', newMcp)
els.resetMcpButton.addEventListener('click', resetMcpEditor)
els.deleteMcpButton.addEventListener('click', deleteMcp)
els.tabButtons.forEach((button) => {
  button.addEventListener('click', () => openDrawer(button.dataset.tab))
})

setActiveTab(state.currentTab)
renderSkillEditor()
renderMcpEditor()

loadBootstrap({ allowAutoSelect: false }).catch((error) => {
  setComposerStatus(error.message || 'Initialization failed.', true)
  setDrawerStatus(error.message || 'Initialization failed.', true)
})
