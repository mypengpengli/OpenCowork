const state = {
  bootstrap: null,
  currentSessionId: null,
  currentSession: null,
  sending: false,
}

const els = {
  sessionList: document.querySelector('#session-list'),
  messageList: document.querySelector('#message-list'),
  chatTitle: document.querySelector('#chat-title'),
  runtimeModel: document.querySelector('#runtime-model'),
  runtimePermission: document.querySelector('#runtime-permission'),
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

function setStatus(message, isError = false) {
  els.composerStatus.textContent = message
  els.composerStatus.style.color = isError ? 'var(--danger)' : 'var(--muted)'
}

function sessionTimestamp(value) {
  if (!value) return '刚刚'
  return new Date(Number(value)).toLocaleString('zh-CN', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function renderSessions() {
  const sessions = state.bootstrap?.sessions || []
  els.sessionList.innerHTML = ''
  if (!sessions.length) {
    els.sessionList.innerHTML =
      '<div class="empty-state">还没有保存的会话。直接在中间输入，第一条消息会自动建会话。</div>'
    return
  }

  sessions.forEach((session) => {
    const button = document.createElement('button')
    button.type = 'button'
    button.className = `session-button${state.currentSessionId === session.id ? ' is-active' : ''}`
    button.innerHTML = `
      <span class="session-title">${session.id}</span>
      <span class="session-meta">${session.messageCount} 条消息 · ${sessionTimestamp(session.updatedAtUnixMs)}</span>
    `
    button.addEventListener('click', () => loadSession(session.id))
    els.sessionList.appendChild(button)
  })
}

function renderMessages() {
  const messages = state.currentSession?.messages || []
  els.messageList.innerHTML = ''
  if (!messages.length) {
    els.messageList.innerHTML =
      '<div class="empty-state">这里显示会话消息。左边切换会话，中间继续对话，右边调整 API / Skill / MCP 入口。</div>'
    return
  }

  messages.forEach((message) => {
    const card = document.createElement('article')
    card.className = 'message-card'
    card.dataset.role = String(message.role || '').toLowerCase()

    const role = document.createElement('p')
    role.className = 'message-role'
    role.textContent = String(message.role || 'unknown')
    card.appendChild(role)

    ;(message.blocks || []).forEach((block) => {
      const blockNode = document.createElement('div')
      blockNode.className = 'message-block'
      if (block.type === 'text') {
        blockNode.textContent = block.text
      } else if (block.type === 'tool_use') {
        blockNode.textContent = `[tool_use] ${block.name}\n${block.input}`
      } else if (block.type === 'tool_result') {
        blockNode.textContent = `[tool_result] ${block.toolName || block.tool_name}\n${block.output}`
      } else {
        blockNode.textContent = JSON.stringify(block, null, 2)
      }
      card.appendChild(blockNode)
    })

    els.messageList.appendChild(card)
  })

  els.messageList.scrollTop = els.messageList.scrollHeight
}

function renderSkills() {
  const skills = state.bootstrap?.skills || []
  els.skillCount.textContent = String(skills.length)
  els.skillList.innerHTML = ''
  if (!skills.length) {
    els.skillList.innerHTML = '<div class="empty-state">还没有自定义 Skill。右侧直接新增。</div>'
    return
  }

  skills.forEach((skill) => {
    const card = document.createElement('article')
    card.className = 'mini-card'
    const tools = skill.allowedTools?.length ? `<span>${skill.allowedTools.join(', ')}</span>` : ''
    const paths = skill.paths?.length ? `<span>${skill.paths.join(', ')}</span>` : ''
    card.innerHTML = `
      <h3>${skill.name}</h3>
      <p>${skill.description || '没有描述'}</p>
      <div class="mini-meta">
        <span>${skill.origin}</span>
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
    els.mcpList.innerHTML = '<div class="empty-state">还没有 MCP 服务器。可以先从 stdio 或 http 加一个。</div>'
    return
  }

  servers.forEach((server) => {
    const card = document.createElement('article')
    card.className = 'mini-card'
    card.innerHTML = `
      <h3>${server.name}</h3>
      <p>${server.command || server.endpoint || '未配置 command / endpoint'}</p>
      <div class="mini-meta">
        <span>${server.transport}</span>
        <span>${server.authType}</span>
        ${server.timeoutMs ? `<span>${server.timeoutMs} ms</span>` : ''}
      </div>
    `
    els.mcpList.appendChild(card)
  })
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
}

function resetComposer(sessionId = null) {
  state.currentSessionId = sessionId
  state.currentSession = sessionId ? state.currentSession : null
  els.composerInput.value = ''
}

async function loadBootstrap() {
  state.bootstrap = await request('/api/bootstrap')
  if (!state.currentSessionId && state.bootstrap.sessions.length) {
    state.currentSessionId = state.bootstrap.sessions[0].id
    await loadSession(state.currentSessionId, false)
  }
  renderSessions()
  renderSkills()
  renderMcp()
  renderProvider()
  if (!state.currentSession) {
    renderMessages()
  }
}

async function loadSession(sessionId, rerender = true) {
  state.currentSession = await request(`/api/sessions/${encodeURIComponent(sessionId)}`)
  state.currentSessionId = sessionId
  els.chatTitle.textContent = `OpenClaw 会话 · ${sessionId}`
  if (rerender) {
    renderSessions()
  }
  renderMessages()
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
    els.chatTitle.textContent = `OpenClaw 会话 · ${response.sessionId}`
    els.composerInput.value = ''
    await loadBootstrap()
    renderMessages()
    setStatus(`完成，${response.iterations} 轮。`)
  } catch (error) {
    setStatus(error.message || '发送失败', true)
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
    setStatus(error.message || '保存 API 设置失败', true)
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
    setStatus(error.message || '保存 Skill 失败', true)
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
    setStatus(error.message || '保存 MCP 失败', true)
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
  els.chatTitle.textContent = 'OpenClaw 会话'
  renderSessions()
  renderMessages()
  setStatus('新会话已就绪。')
}

els.composerForm.addEventListener('submit', submitChat)
els.providerForm.addEventListener('submit', submitProvider)
els.skillForm.addEventListener('submit', submitSkill)
els.mcpForm.addEventListener('submit', submitMcp)
els.newSessionButton.addEventListener('click', startNewSession)
els.reloadSessionsButton.addEventListener('click', async () => {
  try {
    await loadBootstrap()
    setStatus('会话列表已刷新。')
  } catch (error) {
    setStatus(error.message || '刷新失败', true)
  }
})

loadBootstrap().catch((error) => {
  setStatus(error.message || '初始化失败', true)
})
