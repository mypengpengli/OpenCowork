const state = {
  bootstrap: null,
  currentSessionId: null,
  currentSession: null,
  currentTab: 'api',
  sending: false,
  lastEvents: [],
  sessionFilter: '',
  expandedBlocks: new Set(),
  turnStats: {
    iterations: null,
    estimatedPromptTokens: null,
    compacted: null,
  },
}

const els = {
  workspaceCwd: document.querySelector('#workspace-cwd'),
  workspaceSettings: document.querySelector('#workspace-settings'),
  teamMemoryState: document.querySelector('#team-memory-state'),
  teamMemoryDetail: document.querySelector('#team-memory-detail'),
  sessionCount: document.querySelector('#session-count'),
  sessionMeta: document.querySelector('#session-meta'),
  sessionSearch: document.querySelector('#session-search'),
  sessionList: document.querySelector('#session-list'),
  messageList: document.querySelector('#message-list'),
  messageCount: document.querySelector('#message-count'),
  eventList: document.querySelector('#event-list'),
  eventCount: document.querySelector('#event-count'),
  chatTitle: document.querySelector('#chat-title'),
  chatSubtitle: document.querySelector('#chat-subtitle'),
  runtimeModel: document.querySelector('#runtime-model'),
  runtimePermission: document.querySelector('#runtime-permission'),
  providerPersisted: document.querySelector('#provider-persisted'),
  currentSessionChip: document.querySelector('#current-session-chip'),
  sessionUpdatedChip: document.querySelector('#session-updated-chip'),
  summaryMessages: document.querySelector('#summary-messages'),
  summaryTools: document.querySelector('#summary-tools'),
  summaryTokens: document.querySelector('#summary-tokens'),
  summaryMemory: document.querySelector('#summary-memory'),
  turnIterations: document.querySelector('#turn-iterations'),
  turnPromptTokens: document.querySelector('#turn-prompt-tokens'),
  turnCompacted: document.querySelector('#turn-compacted'),
  turnEventTotal: document.querySelector('#turn-event-total'),
  composerForm: document.querySelector('#composer-form'),
  composerInput: document.querySelector('#composer-input'),
  composerStatus: document.querySelector('#composer-status'),
  sendButton: document.querySelector('#send-button'),
  expandAllButton: document.querySelector('#expand-all-button'),
  newSessionButton: document.querySelector('#new-session-button'),
  reloadSessionsButton: document.querySelector('#reload-sessions-button'),
  providerForm: document.querySelector('#provider-form'),
  providerModel: document.querySelector('#provider-model'),
  providerName: document.querySelector('#provider-name'),
  providerApiKeyEnv: document.querySelector('#provider-api-key-env'),
  providerBaseUrl: document.querySelector('#provider-base-url'),
  providerBaseUrlEnv: document.querySelector('#provider-base-url-env'),
  providerTimeoutMs: document.querySelector('#provider-timeout-ms'),
  skillForm: document.querySelector('#skill-form'),
  skillCount: document.querySelector('#skill-count'),
  skillList: document.querySelector('#skill-list'),
  mcpForm: document.querySelector('#mcp-form'),
  mcpCount: document.querySelector('#mcp-count'),
  mcpList: document.querySelector('#mcp-list'),
  activeTabLabel: document.querySelector('#active-tab-label'),
  tabButtons: Array.from(document.querySelectorAll('.tab-button')),
  tabPanels: Array.from(document.querySelectorAll('.tab-panel')),
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

function setStatus(message, isError = false) {
  els.composerStatus.textContent = message
  els.composerStatus.style.color = isError ? 'var(--danger)' : 'var(--muted)'
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

function compactText(value, maxLength = 54) {
  const text = String(value || '')
  if (!text) return '-'
  if (text.length <= maxLength) return text
  const head = Math.max(18, Math.floor(maxLength / 2) - 2)
  const tail = Math.max(14, Math.floor(maxLength / 2) - 4)
  return `${text.slice(0, head)} ... ${text.slice(-tail)}`
}

function safeCount(value) {
  return Array.isArray(value) ? value.length : 0
}

function currentSessionDescriptor() {
  const sessions = state.bootstrap?.sessions || []
  return sessions.find((session) => session.id === state.currentSessionId) || null
}

function computeSessionMetrics(session) {
  const metrics = {
    messageCount: 0,
    toolBlockCount: 0,
    totalTokens: 0,
    hasMemory: false,
  }

  if (!session) {
    return metrics
  }

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

function shouldCollapseBlock(content) {
  const text = String(content || '')
  const lineCount = text.split('\n').length
  return text.length > 500 || lineCount > 12
}

function renderWorkspaceMeta() {
  const bootstrap = state.bootstrap
  if (!bootstrap) return

  const sessions = bootstrap.sessions || []
  els.sessionCount.textContent = String(sessions.length)
  els.sessionMeta.textContent = `${sessions.length} items`

  els.workspaceCwd.textContent = compactText(bootstrap.cwd)
  els.workspaceCwd.title = bootstrap.cwd || ''
  els.workspaceSettings.textContent = compactText(bootstrap.settingsFile)
  els.workspaceSettings.title = bootstrap.settingsFile || ''

  const teamMemory = bootstrap.teamMemorySync || {}
  const running = Boolean(getValue(teamMemory, 'running'))
  const pending = Boolean(getValue(teamMemory, 'pending_changes'))
  const lastError = getValue(teamMemory, 'last_error')
  const filesPulled = Number(getValue(teamMemory, 'files_pulled') || 0)
  const filesPushed = Number(getValue(teamMemory, 'files_pushed') || 0)

  if (lastError) {
    els.teamMemoryState.textContent = 'Error'
    els.teamMemoryDetail.textContent = compactText(lastError, 48)
    els.teamMemoryDetail.title = String(lastError)
  } else if (running) {
    els.teamMemoryState.textContent = pending ? 'Syncing' : 'Running'
    els.teamMemoryDetail.textContent = `pull ${filesPulled} / push ${filesPushed}`
    els.teamMemoryDetail.title = els.teamMemoryDetail.textContent
  } else if (bootstrap.teamMemorySync?.endpoint) {
    els.teamMemoryState.textContent = 'Configured'
    els.teamMemoryDetail.textContent = 'Idle'
    els.teamMemoryDetail.title = 'Idle'
  } else {
    els.teamMemoryState.textContent = 'Not configured'
    els.teamMemoryDetail.textContent = 'No activity'
    els.teamMemoryDetail.title = 'No activity'
  }
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

  els.runtimeModel.textContent = provider.model || 'model'
  els.runtimePermission.textContent = state.bootstrap.permissionMode || 'permission'
  els.providerPersisted.textContent = provider.persisted ? 'provider: saved' : 'provider: default'
}

function renderSessions() {
  const sessions = state.bootstrap?.sessions || []
  const query = state.sessionFilter.trim().toLowerCase()
  const visibleSessions = query
    ? sessions.filter((session) => session.id.toLowerCase().includes(query))
    : sessions

  els.sessionList.innerHTML = ''
  els.sessionMeta.textContent = `${visibleSessions.length} visible`

  if (!visibleSessions.length) {
    els.sessionList.innerHTML = query
      ? '<div class="empty-state">No sessions match the current filter.</div>'
      : '<div class="empty-state">No saved sessions yet. The first message will create one automatically.</div>'
    return
  }

  visibleSessions.forEach((session) => {
    const button = document.createElement('button')
    button.type = 'button'
    button.className = `session-button${state.currentSessionId === session.id ? ' is-active' : ''}`

    const title = document.createElement('span')
    title.className = 'session-title'
    title.textContent = session.id

    const meta = document.createElement('span')
    meta.className = 'session-meta'
    meta.textContent = `${session.messageCount} messages · ${toLocaleTimestamp(session.updatedAtUnixMs)}`

    button.appendChild(title)
    button.appendChild(meta)
    button.addEventListener('click', () => loadSession(session.id))
    els.sessionList.appendChild(button)
  })
}

function blockLabel(block) {
  switch (block.type) {
    case 'text':
      return 'text'
    case 'tool_use':
      return `tool_use · ${getValue(block, 'name') || 'unknown'}`
    case 'tool_result':
      return `tool_result · ${getValue(block, 'tool_name', 'toolName') || 'unknown'}`
    default:
      return block.type || 'block'
  }
}

function blockContent(block) {
  if (block.type === 'text') {
    return getValue(block, 'text') || ''
  }
  if (block.type === 'tool_use') {
    return getValue(block, 'input') || ''
  }
  if (block.type === 'tool_result') {
    return getValue(block, 'output') || ''
  }
  return JSON.stringify(block, null, 2)
}

function renderMessages() {
  const messages = state.currentSession?.messages || []
  els.messageCount.textContent = String(messages.length)
  els.messageList.innerHTML = ''
  const collapsibleKeys = []

  if (!messages.length) {
    els.expandAllButton.disabled = true
    els.expandAllButton.textContent = 'Expand All'
    els.messageList.innerHTML =
      '<div class="empty-state">Messages for the current session will appear here. Use the left rail to switch sessions and the right rail to manage API, skills and MCP.</div>'
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
      const contentId = `${messageIndex}:${blockIndex}`
      const collapseCandidate = shouldCollapseBlock(content)
      const isExpanded = state.expandedBlocks.has(contentId)
      if (collapseCandidate) {
        collapsibleKeys.push(contentId)
      }

      const meta = document.createElement('span')
      meta.className = 'message-block-meta'
      meta.textContent = `${String(content).length} chars`

      blockHeader.appendChild(tag)

      const rightSide = document.createElement('div')
      rightSide.className = 'inline-actions'
      rightSide.appendChild(meta)

      if (collapseCandidate) {
        const toggle = document.createElement('button')
        toggle.type = 'button'
        toggle.className = 'message-expand-button'
        toggle.textContent = isExpanded ? 'Collapse' : 'Expand'
        toggle.addEventListener('click', () => {
          if (state.expandedBlocks.has(contentId)) {
            state.expandedBlocks.delete(contentId)
          } else {
            state.expandedBlocks.add(contentId)
          }
          renderMessages()
        })
        rightSide.appendChild(toggle)
      }

      blockHeader.appendChild(rightSide)
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
      return getValue(event, 'input') || 'No tool input.'
    case 'tool_result':
      return getValue(event, 'output') || 'No tool output.'
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
      const total = inputTokens + outputTokens + cacheRead + cacheCreate
      return `input ${inputTokens} · output ${outputTokens} · cache read ${cacheRead} · cache create ${cacheCreate} · total ${total}`
    }
    case 'message_stop':
      return 'Current assistant message finished.'
    default:
      return JSON.stringify(event, null, 2)
  }
}

function renderEvents() {
  const events = normalizeEvents(state.lastEvents)
  els.eventCount.textContent = String(events.length)
  els.turnEventTotal.textContent = String(events.length)
  els.eventList.innerHTML = ''

  if (!events.length) {
    els.eventList.innerHTML =
      '<div class="empty-state">Recent turn events, tool calls and usage updates will appear here.</div>'
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

function renderSkills() {
  const skills = state.bootstrap?.skills || []
  els.skillCount.textContent = String(skills.length)
  els.skillList.innerHTML = ''

  if (!skills.length) {
    els.skillList.innerHTML =
      '<div class="empty-state">No custom skills yet. Create one from the form above.</div>'
    return
  }

  skills.forEach((skill) => {
    const card = document.createElement('article')
    card.className = 'mini-card'

    const title = document.createElement('h3')
    title.textContent = skill.name

    const description = document.createElement('p')
    description.textContent = skill.description || 'No description'

    const meta = document.createElement('div')
    meta.className = 'mini-meta'

    const chips = [skill.origin || 'skills']
      .concat(skill.allowedTools || [])
      .concat(skill.paths || [])
      .slice(0, 6)

    chips.forEach((chip) => {
      const span = document.createElement('span')
      span.textContent = chip
      meta.appendChild(span)
    })

    card.appendChild(title)
    card.appendChild(description)
    card.appendChild(meta)
    els.skillList.appendChild(card)
  })
}

function renderMcp() {
  const servers = state.bootstrap?.mcpServers || []
  els.mcpCount.textContent = String(servers.length)
  els.mcpList.innerHTML = ''

  if (!servers.length) {
    els.mcpList.innerHTML =
      '<div class="empty-state">No MCP servers yet. Start with a simple http or stdio setup.</div>'
    return
  }

  servers.forEach((server) => {
    const card = document.createElement('article')
    card.className = 'mini-card'

    const title = document.createElement('h3')
    title.textContent = server.name

    const description = document.createElement('p')
    description.textContent = server.command || server.endpoint || 'No command or endpoint'

    const meta = document.createElement('div')
    meta.className = 'mini-meta'

    ;[server.transport || 'unknown', server.authType || 'none', server.timeoutMs ? `${server.timeoutMs} ms` : null]
      .filter(Boolean)
      .forEach((value) => {
        const span = document.createElement('span')
        span.textContent = value
        meta.appendChild(span)
      })

    card.appendChild(title)
    card.appendChild(description)
    card.appendChild(meta)
    els.mcpList.appendChild(card)
  })
}

function renderSessionSummary() {
  const descriptor = currentSessionDescriptor()
  const session = state.currentSession
  const metrics = computeSessionMetrics(session)

  els.summaryMessages.textContent = String(metrics.messageCount)
  els.summaryTools.textContent = String(metrics.toolBlockCount)
  els.summaryTokens.textContent = String(metrics.totalTokens)
  els.summaryMemory.textContent = metrics.hasMemory ? 'Loaded' : 'Not loaded'

  if (!state.currentSessionId || !session) {
    els.chatTitle.textContent = 'OpenClaw Session'
    els.chatSubtitle.textContent =
      'Ready to create a new session. The first prompt will allocate a session id.'
    els.currentSessionChip.textContent = 'No active session'
    els.sessionUpdatedChip.textContent = 'Waiting for first turn'
    return
  }

  els.chatTitle.textContent = `OpenClaw Session · ${state.currentSessionId}`
  els.chatSubtitle.textContent = `This session has ${metrics.messageCount} messages and continues on the existing runtime.`
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

function setActiveTab(tab) {
  state.currentTab = tab
  els.activeTabLabel.textContent = tab.toUpperCase()

  els.tabButtons.forEach((button) => {
    button.classList.toggle('is-active', button.dataset.tab === tab)
  })

  els.tabPanels.forEach((panel) => {
    panel.classList.toggle('is-hidden', panel.dataset.panel !== tab)
  })
}

async function loadBootstrap({ allowAutoSelect = true } = {}) {
  state.bootstrap = await request('/api/bootstrap')

  renderWorkspaceMeta()
  renderProvider()
  renderSkills()
  renderMcp()
  renderSessions()

  const sessions = state.bootstrap.sessions || []
  const stillExists = state.currentSessionId
    ? sessions.some((session) => session.id === state.currentSessionId)
    : false

  if (!stillExists) {
    state.currentSession = null
  }

  if (allowAutoSelect && !state.currentSession && sessions.length) {
    await loadSession(sessions[0].id, false)
    return
  }

  renderSessionSummary()
  renderTurnStats()
  renderMessages()
  renderEvents()
}

async function loadSession(sessionId, rerenderSessions = true) {
  state.currentSession = await request(`/api/sessions/${encodeURIComponent(sessionId)}`)
  state.currentSessionId = sessionId
  state.lastEvents = []
  state.turnStats = {
    iterations: null,
    estimatedPromptTokens: null,
    compacted: null,
  }

  if (rerenderSessions) {
    renderSessions()
  }

  renderSessionSummary()
  renderTurnStats()
  renderMessages()
  renderEvents()
}

async function submitChat(event) {
  event.preventDefault()

  if (state.sending) return

  const input = els.composerInput.value.trim()
  if (!input) {
    setStatus('Enter some input first.', true)
    return
  }

  state.sending = true
  els.sendButton.disabled = true
  setStatus('Calling the existing runtime ...')

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
    els.composerInput.value = ''

    await loadBootstrap({ allowAutoSelect: false })
    renderSessionSummary()
    renderTurnStats()
    renderMessages()
    renderEvents()
    setStatus(
      `Done. ${response.iterations} iteration(s), prompt estimate ${response.estimatedPromptTokens}.`,
    )
  } catch (error) {
    setStatus(error.message || 'Send failed.', true)
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
        model: els.providerModel.value.trim(),
        name: els.providerName.value.trim(),
        apiKeyEnv: els.providerApiKeyEnv.value.trim(),
        baseUrl: els.providerBaseUrl.value.trim(),
        baseUrlEnv: els.providerBaseUrlEnv.value.trim(),
        timeoutMs: Number(els.providerTimeoutMs.value || 90000),
      }),
    })

    state.bootstrap.provider = provider
    renderProvider()
    setStatus('API settings saved.')
  } catch (error) {
    setStatus(error.message || 'Saving API settings failed.', true)
  }
}

async function submitSkill(event) {
  event.preventDefault()

  try {
    const skills = await request('/api/skills', {
      method: 'POST',
      body: JSON.stringify({
        name: document.querySelector('#skill-name').value.trim(),
        description: document.querySelector('#skill-description').value.trim() || null,
        whenToUse: document.querySelector('#skill-when').value.trim() || null,
        allowedTools: splitComma(document.querySelector('#skill-tools').value),
        paths: splitComma(document.querySelector('#skill-paths').value),
        content: document.querySelector('#skill-content').value.trim(),
      }),
    })

    state.bootstrap.skills = skills
    renderSkills()
    els.skillForm.reset()
    setStatus('Skill saved into .opencowork/skills.')
  } catch (error) {
    setStatus(error.message || 'Saving skill failed.', true)
  }
}

async function submitMcp(event) {
  event.preventDefault()

  try {
    const servers = await request('/api/mcp', {
      method: 'POST',
      body: JSON.stringify({
        name: document.querySelector('#mcp-name').value.trim(),
        transport: document.querySelector('#mcp-transport').value,
        command: document.querySelector('#mcp-command').value.trim() || null,
        args: splitComma(document.querySelector('#mcp-args').value),
        endpoint: document.querySelector('#mcp-endpoint').value.trim() || null,
        authType: document.querySelector('#mcp-auth-type').value,
        tokenEnv: document.querySelector('#mcp-token-env').value.trim() || null,
        tokenPath: document.querySelector('#mcp-token-path').value.trim() || null,
      }),
    })

    state.bootstrap.mcpServers = servers
    renderMcp()
    els.mcpForm.reset()
    setStatus('MCP settings saved.')
  } catch (error) {
    setStatus(error.message || 'Saving MCP failed.', true)
  }
}

function splitComma(value) {
  return String(value || '')
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean)
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
  setStatus('New session ready.')
}

function toggleExpandAll() {
  const session = state.currentSession
  if (!session) return

  const keys = []
  ;(session.messages || []).forEach((message, messageIndex) => {
    ;(message.blocks || []).forEach((block, blockIndex) => {
      const key = `${messageIndex}:${blockIndex}`
      if (shouldCollapseBlock(blockContent(block))) {
        keys.push(key)
      }
    })
  })

  const allExpanded = keys.length > 0 && keys.every((key) => state.expandedBlocks.has(key))
  if (allExpanded) {
    keys.forEach((key) => state.expandedBlocks.delete(key))
  } else {
    keys.forEach((key) => state.expandedBlocks.add(key))
  }

  els.expandAllButton.textContent = allExpanded ? 'Expand All' : 'Collapse All'
  renderMessages()
}

els.composerForm.addEventListener('submit', submitChat)
els.composerInput.addEventListener('keydown', (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
    event.preventDefault()
    els.composerForm.requestSubmit()
  }
})
els.providerForm.addEventListener('submit', submitProvider)
els.skillForm.addEventListener('submit', submitSkill)
els.mcpForm.addEventListener('submit', submitMcp)
els.newSessionButton.addEventListener('click', startNewSession)
els.reloadSessionsButton.addEventListener('click', async () => {
  try {
    await loadBootstrap({ allowAutoSelect: true })
    setStatus('Session list refreshed.')
  } catch (error) {
    setStatus(error.message || 'Refreshing failed.', true)
  }
})
els.expandAllButton.addEventListener('click', toggleExpandAll)
els.sessionSearch.addEventListener('input', (event) => {
  state.sessionFilter = event.target.value || ''
  renderSessions()
})
els.tabButtons.forEach((button) => {
  button.addEventListener('click', () => setActiveTab(button.dataset.tab))
})

setActiveTab(state.currentTab)

loadBootstrap({ allowAutoSelect: true }).catch((error) => {
  setStatus(error.message || 'Initialization failed.', true)
})
