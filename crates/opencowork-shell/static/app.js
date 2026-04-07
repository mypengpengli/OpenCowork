const state = {
  bootstrap: null,
  currentSessionId: null,
  currentSession: null,
  currentTab: 'api',
  sending: false,
  lastEvents: [],
}

const els = {
  workspaceCwd: document.querySelector('#workspace-cwd'),
  workspaceSettings: document.querySelector('#workspace-settings'),
  teamMemoryState: document.querySelector('#team-memory-state'),
  teamMemoryDetail: document.querySelector('#team-memory-detail'),
  sessionCount: document.querySelector('#session-count'),
  sessionMeta: document.querySelector('#session-meta'),
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
  composerForm: document.querySelector('#composer-form'),
  composerInput: document.querySelector('#composer-input'),
  composerStatus: document.querySelector('#composer-status'),
  sendButton: document.querySelector('#send-button'),
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
  if (!value) return '刚刚'
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
  return `${text.slice(0, head)} … ${text.slice(-tail)}`
}

function safeCount(value) {
  return Array.isArray(value) ? value.length : 0
}

function currentSessionDescriptor() {
  const sessions = state.bootstrap?.sessions || []
  return sessions.find((session) => session.id === state.currentSessionId) || null
}

function renderWorkspaceMeta() {
  const bootstrap = state.bootstrap
  if (!bootstrap) return

  const sessions = bootstrap.sessions || []
  els.sessionCount.textContent = String(sessions.length)
  els.sessionMeta.textContent = `${sessions.length} 条`

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
    els.teamMemoryState.textContent = '异常'
    els.teamMemoryDetail.textContent = compactText(lastError, 48)
    els.teamMemoryDetail.title = String(lastError)
  } else if (running) {
    els.teamMemoryState.textContent = pending ? '同步中' : '运行中'
    els.teamMemoryDetail.textContent = `pull ${filesPulled} / push ${filesPushed}`
    els.teamMemoryDetail.title = els.teamMemoryDetail.textContent
  } else if (bootstrap.teamMemorySync?.endpoint) {
    els.teamMemoryState.textContent = '已配置'
    els.teamMemoryDetail.textContent = '尚未启动'
    els.teamMemoryDetail.title = '尚未启动'
  } else {
    els.teamMemoryState.textContent = '未配置'
    els.teamMemoryDetail.textContent = '暂无活动'
    els.teamMemoryDetail.title = '暂无活动'
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
  els.sessionList.innerHTML = ''

  if (!sessions.length) {
    els.sessionList.innerHTML =
      '<div class="empty-state">还没有保存的会话。直接在中间输入内容，第一条消息会自动创建新会话。</div>'
    return
  }

  sessions.forEach((session) => {
    const button = document.createElement('button')
    button.type = 'button'
    button.className = `session-button${state.currentSessionId === session.id ? ' is-active' : ''}`
    const title = document.createElement('span')
    title.className = 'session-title'
    title.textContent = session.id

    const meta = document.createElement('span')
    meta.className = 'session-meta'
    meta.textContent = `${session.messageCount} 条消息 · ${toLocaleTimestamp(session.updatedAtUnixMs)}`

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

  if (!messages.length) {
    els.messageList.innerHTML =
      '<div class="empty-state">这里显示会话消息。左侧切换会话，中间继续对话，右侧维护 API、Skill 和 MCP。</div>'
    return
  }

  messages.forEach((message) => {
    const card = document.createElement('article')
    card.className = 'message-card'
    card.dataset.role = String(message.role || '').toLowerCase()

    const header = document.createElement('div')
    header.className = 'message-header'
    header.innerHTML = `
      <p class="message-role">${String(message.role || 'unknown')}</p>
      <span class="message-block-count">${safeCount(message.blocks)} blocks</span>
    `
    card.appendChild(header)

    const body = document.createElement('div')
    body.className = 'message-body'

    ;(message.blocks || []).forEach((block) => {
      const blockNode = document.createElement('div')
      blockNode.className = 'message-block'

      const tag = document.createElement('span')
      tag.className = 'message-block-tag'
      tag.textContent = blockLabel(block)
      blockNode.appendChild(tag)

      const content = document.createElement('div')
      content.className = 'message-block-content'
      content.textContent = blockContent(block)
      blockNode.appendChild(content)

      body.appendChild(blockNode)
    })

    card.appendChild(body)
    els.messageList.appendChild(card)
  })

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
      return 'Assistant 输出'
    case 'tool_call':
      return getValue(event, 'name') || '工具调用'
    case 'tool_result':
      return getValue(event, 'tool_name', 'toolName') || '工具结果'
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
      return (getValue(event, 'text') || '').trim() || '模型正在增量输出文本。'
    case 'tool_call':
      return getValue(event, 'input') || '未提供输入'
    case 'tool_result':
      return getValue(event, 'output') || '未提供输出'
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
      return '这一轮消息已经结束。'
    default:
      return JSON.stringify(event, null, 2)
  }
}

function renderEvents() {
  const events = normalizeEvents(state.lastEvents)
  els.eventCount.textContent = String(events.length)
  els.eventList.innerHTML = ''

  if (!events.length) {
    els.eventList.innerHTML =
      '<div class="empty-state">这里显示最近一轮的工具调用、增量输出和 usage 事件。</div>'
    return
  }

  events.forEach((event) => {
    const card = document.createElement('article')
    card.className = 'event-card'
    card.innerHTML = `
      <div class="event-title-row">
        <h3 class="event-title">${escapeHtml(eventTitle(event))}</h3>
        <span class="event-type">${escapeHtml(event.type || 'event')}</span>
      </div>
      <div class="event-body">${escapeHtml(eventBody(event))}</div>
    `
    els.eventList.appendChild(card)
  })
}

function renderSkills() {
  const skills = state.bootstrap?.skills || []
  els.skillCount.textContent = String(skills.length)
  els.skillList.innerHTML = ''

  if (!skills.length) {
    els.skillList.innerHTML = '<div class="empty-state">还没有自定义 Skill。可以直接从右侧表单写入。</div>'
    return
  }

  skills.forEach((skill) => {
    const tools = skill.allowedTools?.length
      ? `<span>${escapeHtml(skill.allowedTools.join(', '))}</span>`
      : ''
    const paths = skill.paths?.length ? `<span>${escapeHtml(skill.paths.join(', '))}</span>` : ''
    const card = document.createElement('article')
    card.className = 'mini-card'
    card.innerHTML = `
      <h3>${escapeHtml(skill.name)}</h3>
      <p>${escapeHtml(skill.description || '没有描述')}</p>
      <div class="mini-meta">
        <span>${escapeHtml(skill.origin || 'skills')}</span>
        ${tools}
        ${paths}
      </div>
    `
    els.skillList.appendChild(card)
  })
}

function renderMcp() {
  const servers = state.bootstrap?.mcpServers || []
  els.mcpCount.textContent = String(servers.length)
  els.mcpList.innerHTML = ''

  if (!servers.length) {
    els.mcpList.innerHTML = '<div class="empty-state">还没有 MCP 服务。可以先从 http 或 stdio 开始。</div>'
    return
  }

  servers.forEach((server) => {
    const card = document.createElement('article')
    card.className = 'mini-card'
    card.innerHTML = `
      <h3>${escapeHtml(server.name)}</h3>
      <p>${escapeHtml(server.command || server.endpoint || '未配置 command / endpoint')}</p>
      <div class="mini-meta">
        <span>${escapeHtml(server.transport || 'unknown')}</span>
        <span>${escapeHtml(server.authType || 'none')}</span>
        ${server.timeoutMs ? `<span>${escapeHtml(`${server.timeoutMs} ms`)}</span>` : ''}
      </div>
    `
    els.mcpList.appendChild(card)
  })
}

function renderSessionSummary() {
  const descriptor = currentSessionDescriptor()
  const session = state.currentSession

  if (!state.currentSessionId || !session) {
    els.chatTitle.textContent = 'OpenClaw 会话'
    els.chatSubtitle.textContent = '准备创建新会话。输入第一条消息后，系统会自动分配会话 ID。'
    els.currentSessionChip.textContent = '未开始'
    els.sessionUpdatedChip.textContent = '等待第一条消息'
    els.messageCount.textContent = '0'
    return
  }

  els.chatTitle.textContent = `OpenClaw 会话 · ${state.currentSessionId}`
  els.chatSubtitle.textContent = `当前会话包含 ${session.messages.length} 条消息，继续对话时仍走原有 runtime。`
  els.currentSessionChip.textContent = state.currentSessionId
  els.sessionUpdatedChip.textContent = descriptor
    ? `更新于 ${toLocaleTimestamp(descriptor.updatedAtUnixMs)}`
    : '已加载当前会话'
  els.messageCount.textContent = String(session.messages.length)
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
  renderMessages()
  renderEvents()
}

async function loadSession(sessionId, rerenderSessions = true) {
  state.currentSession = await request(`/api/sessions/${encodeURIComponent(sessionId)}`)
  state.currentSessionId = sessionId
  state.lastEvents = []

  if (rerenderSessions) {
    renderSessions()
  }
  renderSessionSummary()
  renderMessages()
  renderEvents()
}

async function submitChat(event) {
  event.preventDefault()

  if (state.sending) return

  const input = els.composerInput.value.trim()
  if (!input) {
    setStatus('先输入一点内容。', true)
    return
  }

  state.sending = true
  els.sendButton.disabled = true
  setStatus('正在调用现有 runtime …')

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
    els.composerInput.value = ''

    await loadBootstrap({ allowAutoSelect: false })
    renderSessionSummary()
    renderMessages()
    renderEvents()
    setStatus(`完成，${response.iterations} 轮。估算 prompt ${response.estimatedPromptTokens} tokens。`)
  } catch (error) {
    setStatus(error.message || '发送失败。', true)
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
    setStatus('API 设置已保存。')
  } catch (error) {
    setStatus(error.message || '保存 API 设置失败。', true)
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
    setStatus('Skill 已写入 .opencowork/skills。')
  } catch (error) {
    setStatus(error.message || '保存 Skill 失败。', true)
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
    setStatus('MCP 配置已保存。')
  } catch (error) {
    setStatus(error.message || '保存 MCP 失败。', true)
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
  els.composerInput.value = ''
  renderSessions()
  renderSessionSummary()
  renderMessages()
  renderEvents()
  setStatus('新会话已就绪。')
}

function escapeHtml(value) {
  return String(value || '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')
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
    setStatus('会话列表已刷新。')
  } catch (error) {
    setStatus(error.message || '刷新失败。', true)
  }
})
els.tabButtons.forEach((button) => {
  button.addEventListener('click', () => setActiveTab(button.dataset.tab))
})

setActiveTab(state.currentTab)

loadBootstrap({ allowAutoSelect: true }).catch((error) => {
  setStatus(error.message || '初始化失败。', true)
})
