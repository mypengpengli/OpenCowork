import { createUi, executionRanges } from '/ui-controls.mjs'
export function initWorkflows({ state, request, composer, status, prepareTask, openSession }) {
  const ui = createUi(state, status), { el, tr, bind } = ui
  const call = (path, data) => request(path, { method: 'POST', body: JSON.stringify(data) })
  const btn = ui.button
  const input = (label, type = 'text', value = '') => { const n = el('input'); n.type = type; n.value = value; bind(n, label, 'aria-label'); bind(n, label, 'placeholder'); return n }
  const label = (text, n) => { const l = el('label', '', 'feature-toggle'); l.append(n, el('span', text)); return l }
  const detail = title => { const n = el('details', '', 'feature-disclosure'); n.append(el('summary', title)); return n }
  const row = (...nodes) => { const n = el('div', '', 'feature-toolbar'); n.append(...nodes); return n }
  const host = el('section', '', 'workflow-panels'); document.querySelector('.workspace-tools').append(host)
  const goalPanel = detail('目标、补充要求与消息队列')
  const goalNext = input('把本次消息设为目标', 'checkbox')
  const goalState = el('p', '发送目标后，在预算内持续执行并检查完成证据。')
  const steering = el('textarea'); bind(steering, '运行中补充要求，或排入下一轮', 'placeholder'); bind(steering, '运行中补充要求，或排入下一轮', 'aria-label'); steering.rows = 2
  const queue = el('div')
  const summary = el('span', '', 'workflow-status-summary')
  let observedSession = state.currentSessionId, goalData = null, workflowPending = false, paused = null, computerFeatures = null, controlVersion = 0
  const requireId = () => { if (!state.currentSessionId) throw new Error('请先发送一条消息创建会话'); return encodeURIComponent(state.currentSessionId) }
  const workflow = v => call(`/api/workflows/${requireId()}`, v)
  async function enqueue(mode) {
    if (!steering.value.trim()) return
    const sessionId = state.currentSessionId, text = steering.value
    await workflow({ action: mode, id: crypto.randomUUID(), text }); if (sessionId !== state.currentSessionId) return
    if (steering.value === text) steering.value = ''; await refreshGoal()
    if (!state.sending) prepareTask(tr('处理已保存的待办消息，先检查已有结果。', 'Handle the saved pending messages, checking existing results first.'), { submit: true })
  }
  const pauseGoal = btn('暂停目标', async () => { await workflow({ action: 'pause' }); await refreshGoal() })
  const resumeGoal = btn('恢复目标', async () => { if (state.sending) return; const id = state.currentSessionId; await workflow({ action: 'resume' }); if (id !== state.currentSessionId) return; prepareTask(tr('继续已保存目标，检查已有结果，按缺失证据执行。', 'Resume the saved goal, inspect existing results and collect missing evidence.'), { submit: true }); await refreshGoal() })
  const steerButton = btn('补充当前任务', () => enqueue('steer')), queueButton = btn('排入下一轮', () => enqueue('queue'))
  goalPanel.append(label('把本次消息设为目标并自动继续', goalNext), goalState, row(pauseGoal, resumeGoal), steering, row(steerButton, queueButton), queue)
  function renderGoal() {
    const g = goalData?.goal, messages = (goalData?.messages || []).filter(m => m.status === 'pending')
    const states = { running: tr('执行中', 'Running'), paused: tr('已暂停', 'Paused'), completed: tr('已完成', 'Completed'), blocked: tr('待处理', 'Blocked') }
    goalState.textContent = g?.text ? `${states[g.status] || g.status} · ${g.rounds}/${g.maxRounds} ${tr('轮', 'rounds')} · ${g.tokens}/${g.maxTokens} tokens\n${g.text}\n${g.reason || ''}` : tr('尚未设置目标')
    summary.textContent = [goalNext.checked ? tr('下条消息作为目标', 'Next message is a goal') : g?.text ? `${tr('目标', 'Goal')}: ${states[g.status] || g.status}` : '', messages.length ? tr(`待处理 ${messages.length} 条`, `${messages.length} pending`) : ''].filter(Boolean).join(' · ')
    queue.replaceChildren(...messages.map(m => row(el('span', `${m.mode === 'steer' ? tr('补充', 'Steering') : tr('排队', 'Queued')}：${m.text}`), btn('移除', async () => { await workflow({ action: 'discard', id: m.id }); await refreshGoal() }))))
  }
  async function refreshGoal() {
    updateControls()
    if (!state.currentSessionId || workflowPending) return
    const id = state.currentSessionId; workflowPending = true
    try { const data = await request(`/api/workflows/${encodeURIComponent(id)}`); if (id !== state.currentSessionId) return; goalData = data; renderGoal(); updateControls() }
    finally { workflowPending = false }
  }
  const takeover = el('p'), computerState = el('span', '', 'computer-control-state')
  const controlButton = btn('', async () => {
    ++controlVersion
    window.dispatchEvent(new Event('opencowork:composer-state'))
    try {
      const enabled = !(computerFeatures?.computerEnabled && !paused)
      computerFeatures = await call('/api/features', { computerEnabled: enabled })
      paused = computerFeatures.computerPaused
      window.dispatchEvent(new CustomEvent('opencowork:features-changed', { detail: computerFeatures }))
      renderTakeover()
      status(enabled ? tr('电脑控制已启用。', 'Computer control enabled.') : tr('电脑控制已关闭，已停止当前宿主的活动任务。', 'Computer control disabled; active tasks in this host have been stopped.'))
    } finally { window.dispatchEvent(new Event('opencowork:composer-state')) }
  })
  controlButton.id = 'computer-control-toggle'; controlButton.setAttribute('role', 'switch')
  function renderTakeover() {
    const ready = computerFeatures !== null && paused !== null
    const enabled = ready && computerFeatures.computerEnabled && !paused
    const supported = computerFeatures?.computerSupported !== false
    controlButton.textContent = !ready ? tr('电脑控制：读取中…', 'Computer control: loading…')
      : !supported ? tr('电脑控制：不支持', 'Computer control: unsupported')
      : enabled ? tr('电脑控制：已启用', 'Computer control: on') : tr('电脑控制：未启用', 'Computer control: off')
    controlButton.setAttribute('aria-label', controlButton.textContent)
    controlButton.setAttribute('aria-checked', String(Boolean(enabled && supported)))
    controlButton.title = enabled
      ? tr('点击关闭电脑控制，并停止当前宿主的活动任务', 'Disable computer control and stop active tasks in this host')
      : tr('点击启用电脑控制', 'Enable computer control')
    controlButton.disabled = !ready || !supported || Boolean(controlButton.dataset.busy)
    computerState.textContent = controlButton.textContent
  }
  async function refreshTakeover() {
    const version = ++controlVersion
    const features = await request('/api/features')
    if (version === controlVersion && !controlButton.dataset.busy) { computerFeatures = features; paused = features.computerPaused; renderTakeover() }
  }
  takeover.addEventListener('ui:action-done', () => { renderTakeover(); window.dispatchEvent(new Event('opencowork:composer-state')) })
  window.addEventListener('opencowork:features-changed', event => { ++controlVersion; computerFeatures = event.detail; paused = event.detail.computerPaused; renderTakeover() })
  takeover.append(computerState, controlButton); renderTakeover()
  refreshTakeover().catch(error => { controlButton.textContent = tr('电脑控制：读取失败', 'Computer control: unavailable'); controlButton.title = error.message })
  host.append(takeover, goalPanel)

  const settings = el('article', '', 'settings-card workflow-settings'); settings.append(el('h3', '自动执行与模型能力'))
  const advanced = detail('高级设置：预算与后台模型')
  const fields = { maxIterations: ['每轮最多迭代', 80], maxTokens: ['每轮 token 上限', 250000], maxSeconds: ['每轮秒数上限', 900], repeatedResults: ['重复结果停止阈值', 3], maxOutputTokens: ['单次最大输出 token', 8192] }
  const values = {}
  for (const [key, [name, value]] of Object.entries(fields)) { values[key] = input(name, 'number', value); const [min, max] = executionRanges[key]; values[key].min = String(min); values[key].max = String(max); values[key].step = '1'; values[key].required = true; advanced.append(label(name, values[key])) }
  const background = input('后台模型名称（留空跟随当前模型）'), reasoning = el('select'); bind(reasoning, '推理强度', 'aria-label'); background.maxLength = 200
  for (const [value, name] of [['', '推理强度：提供方默认'], ['none', '无'], ['minimal', '最少'], ['low', '低'], ['medium', '中'], ['high', '高']]) { const n = el('option', name); n.value = value; reasoning.append(n) }
  const quickReasoning = document.querySelector('#composer-reasoning')
  bind(quickReasoning, '推理强度', 'aria-label'); bind(quickReasoning, '推理强度', 'title')
  for (const [value, name] of [['', tr('默认', 'Default')], ['none', '无'], ['minimal', '最少'], ['low', '低'], ['medium', '中'], ['high', '高']]) { const n = el('option', name); n.value = value; quickReasoning.append(n) }
  let executionReady = false, savedReasoning = ''
  quickReasoning.addEventListener('change', async () => {
    if (state.sending || quickReasoning.dataset.busy) return
    quickReasoning.dataset.busy = 'true'; updateControls(); window.dispatchEvent(new Event('opencowork:composer-state'))
    try {
      // Fetch the current budgets so changing effort never resets them.
      const current = await request('/api/execution-settings')
      const effective = await call('/api/execution-settings', { execution: { ...current.execution, reasoningEffort: quickReasoning.value } })
      savedReasoning = effective.reasoningEffort || ''; reasoning.value = savedReasoning
      status(tr('已保存，新回合生效。'))
    } catch (e) { status(e.message, true) }
    finally { quickReasoning.value = savedReasoning; delete quickReasoning.dataset.busy; updateControls(); window.dispatchEvent(new Event('opencowork:composer-state')); composer.focus() }
  })
  const browser = input('浏览器工具', 'checkbox'), learn = input('生成技能候选', 'checkbox'), auto = input('自动采用技能', 'checkbox'); browser.checked = true
  const settingsState = el('p', '', 'settings-card-copy'), probe = el('pre', '', 'feature-file-preview')
  advanced.append(label('后台模型', background), label('推理强度', reasoning))
  function applyExecution(execution) {
    for (const [k, n] of Object.entries(values)) n.value = execution?.[k] ?? fields[k][1]
    background.value = execution?.backgroundModel || ''; reasoning.value = execution?.reasoningEffort || ''
    savedReasoning = reasoning.value; quickReasoning.value = savedReasoning
  }
  const saveSettings = btn('保存执行设置', async () => {
    for (const [key, n] of Object.entries(values)) {
      if (!n.checkValidity()) { advanced.open = true; n.focus(); n.reportValidity(); const [min, max] = executionRanges[key]; throw new Error(`${tr(fields[key][0])}: ${tr('请输入范围内的整数', 'Enter a whole number in range')} ${min}–${max}`) }
    }
    if (new TextEncoder().encode(background.value.trim()).length > 200) { advanced.open = true; background.focus(); throw new Error(tr('后台模型名称过长，请缩短后重试', 'Background model name is too long. Shorten it and retry.')) }
    const execution = Object.fromEntries(Object.entries(values).map(([k, n]) => [k, Number(n.value)])); execution.backgroundModel = background.value.trim(); execution.reasoningEffort = reasoning.value
    const effective = await call('/api/execution-settings', { execution, browser: { enabled: browser.checked }, learning: { enabled: learn.checked, autoAdopt: auto.checked } }); applyExecution(effective); settingsState.textContent = tr('已保存，新回合生效。')
  })
  saveSettings.disabled = true
  settings.append(label('启用独立浏览器工具（需安装 Chrome 或 Edge）', browser), label('成功流程生成技能候选（额外后台请求）', learn), label('自动采用候选技能（关闭时先审阅）', auto), advanced, row(saveSettings, btn('检测模型连接、工具、图像和流式能力', async () => { probe.textContent = tr('正在检测（最多 4 次小请求，可能产生用量）…', 'Checking (up to 4 small requests; usage may be charged)…'); try { const data = await call('/api/provider-probe', {}); probe.textContent = `${data.model} · ${data.protocol}\n` + data.checks.map(c => `${c.kind}: ${c.status} · ${c.elapsedMs} ms · ${c.tokens ?? '?'} tokens ${c.error || ''}`).join('\n') } catch (e) { probe.textContent = e.message; throw e } })), settingsState, probe)
  document.querySelector('#settings-permission-panel .settings-card-list').append(settings)
  request('/api/execution-settings').then(d => { applyExecution(d.execution); browser.checked = d.browser?.enabled !== false; learn.checked = d.learning?.enabled === true; auto.checked = d.learning?.autoAdopt === true; saveSettings.disabled = false; executionReady = true; updateControls() }).catch(e => { settingsState.textContent = e.message })

  const syncCard = el('article', '', 'settings-card team-sync-card'), syncEnabled = input('启用团队记忆同步', 'checkbox')
  const syncEndpoint = input('同步服务地址'), syncRepo = input('仓库标识，例如 owner/project'), syncEnv = input('令牌环境变量名'), syncFile = input('令牌文件完整路径')
  const syncStatus = el('p'), syncConflicts = el('div')
  async function loadSyncConflicts() {
    const data = await request('/api/team-memory-conflicts')
    syncConflicts.replaceChildren(...data.conflicts.map(conflict => {
      const card = detail(conflict.path), local = el('pre', conflict.local, 'feature-file-preview'), remote = el('pre', conflict.remote, 'feature-file-preview')
      const resolve = async choice => { await call('/api/team-memory-conflicts', { id: conflict.id, choice, remoteHash: conflict.remoteHash, localHash: conflict.localHash }); await loadSyncConflicts() }
      card.append(el('h4', tr('本地版本', 'Local version')), local, el('h4', tr('远端版本', 'Remote version')), remote, row(btn(tr('保留本地版本', 'Keep local'), () => resolve('local')), btn(tr('使用远端版本', 'Use remote'), () => resolve('remote')))); return card
    }))
    if (!data.conflicts.length) syncConflicts.append(el('p', tr('没有待处理的同步冲突', 'No pending sync conflicts')))
  }
  syncCard.append(el('h3', tr('团队记忆同步', 'Team memory sync')), el('p', tr('可选功能。令牌选择环境变量或本地文件；冲突会保留双方内容，处理后再上传。', 'Optional. Use a token environment variable or local file. Conflicts preserve both versions and pause uploads.')), label(tr('启用同步', 'Enable sync'), syncEnabled), label(tr('服务地址', 'Endpoint'), syncEndpoint), label(tr('仓库标识', 'Repository'), syncRepo), label(tr('令牌环境变量', 'Token environment variable'), syncEnv), label(tr('令牌文件', 'Token file'), syncFile), row(btn(tr('保存同步设置', 'Save sync settings'), async () => { await call('/api/team-memory-settings', { enabled: syncEnabled.checked, endpoint: syncEndpoint.value, repo: syncRepo.value, tokenEnv: syncEnv.value, tokenFile: syncFile.value }); syncStatus.textContent = tr('设置已保存，重新启动应用后生效。', 'Saved. Restart the app to apply.'); }), btn(tr('查看同步冲突', 'Review sync conflicts'), loadSyncConflicts)), syncStatus, syncConflicts)
  document.querySelector('#settings-memory-panel .settings-card-list').append(syncCard)
  request('/api/team-memory-settings').then(data => { syncEnabled.checked = data.enabled; syncEndpoint.value = data.endpoint || ''; syncRepo.value = data.repo || ''; syncEnv.value = data.tokenEnv || ''; syncFile.value = data.tokenFile || '' }).catch(e => { syncStatus.textContent = e.message })

  const review = detail('审阅更改与恢复'), file = input('选择或输入工作区文件路径'), reviewBody = el('div'), snapshots = el('div'); let revision
  async function refreshReview() { if (!file.value.trim()) return; revision = await request(`/api/review?path=${encodeURIComponent(file.value.trim())}`); renderReview() }
  async function changeReview(action, hunk) { if (!revision) return; const r = await call('/api/review', { action, hunk, path: revision.path, version: revision.version }); if (r.snapshotId) status(`更改已恢复，恢复点：${r.snapshotId}`); await refreshReview() }
  function renderReview() {
    reviewBody.replaceChildren(row(btn('暂存整个文件', () => changeReview('stage')), btn('取消整个文件暂存', () => changeReview('unstage')), btn('撤销工作区文本更改', () => changeReview('revert'))))
    for (const [key, title] of [['unstagedHunks', '未暂存'], ['stagedHunks', '已暂存']]) (revision[key] || []).forEach((h, i) => { const section = detail(`${title} · 分块 ${i + 1}`); section.append(el('pre', h, 'feature-file-preview'), row(btn(key === 'stagedHunks' ? '取消暂存此块' : '暂存此块', () => changeReview(key === 'stagedHunks' ? 'unstage' : 'stage', i)), ...(key === 'unstagedHunks' ? [btn('撤销此块', () => changeReview('revert', i))] : []))); reviewBody.append(section) })
  }
  async function history() { const data = await request('/api/review/history'); snapshots.replaceChildren(...data.map(s => row(el('span', `${new Date(s.createdAt).toLocaleString()} · ${s.path}`), btn('恢复此版本', async () => { await call('/api/review', { action: 'restore', snapshotId: s.id }); status('已恢复；较新的并发修改会被拒绝覆盖。'); await history() })))) }
  review.append(row(file, btn('读取差异', refreshReview), btn('查看恢复点', history)), reviewBody, snapshots); host.append(review)
  window.addEventListener('opencowork:file', e => { file.value = e.detail; if (review.open) refreshReview().catch(e => status(e.message, true)) })

  const search = detail('历史全文搜索'), query = input('搜索正文、工具结果或归档'), all = input('所有项目', 'checkbox'), results = el('div')
  async function searchHistory() { const d = await request(`/api/history-search?q=${encodeURIComponent(query.value)}&all=${all.checked}`); results.replaceChildren(...d.results.map(r => { const section = el('article'); section.append(btn(`${r.sessionId} · ${r.archived ? '归档' : '消息'} ${r.messageIndex + 1}`, () => openSession(r.sessionId)), el('small', r.workspace || '旧会话：项目未知'), el('p', r.snippet), btn('查看匹配消息原文', async () => { const session = await request(`/api/sessions/${encodeURIComponent(r.sessionId)}`); const message = (r.archived ? session.context_collapse_archive : session.messages)?.[r.messageIndex]; if (!message) throw new Error('历史已变化，请重新搜索'); const text = (message.blocks || []).map(b => b.text || b.output || b.input || '').join('\n'); const original = el('pre', text.slice(0, 24000), 'feature-file-preview'); section.append(original) })); return section })); if (!d.results.length) results.append(el('p', '没有匹配结果')); }
  query.onkeydown = e => { if (e.key === 'Enter') { e.preventDefault(); searchHistory().catch(e => status(e.message, true)) } }
  search.append(row(query, label('包含其他项目及旧会话', all), btn('搜索', searchHistory)), results); host.append(search)

  const memories = detail('记忆来源与冲突'), memoryBody = el('div')
  async function refreshMemories() {
    const d = await request('/api/memory-provenance'); memoryBody.replaceChildren()
    for (const m of d.sources) memoryBody.append(row(el('span', m.path), btn(`来源会话 ${m.sourceSession}`, () => openSession(m.sourceSession))))
    for (const c of d.conflicts.filter(c => c.status === 'needs_review')) { const item = detail(c.title); item.append(el('p', c.fact), btn('查看新信息来源', () => openSession(c.sourceSession)), row(btn('保留原记忆', async () => { await call('/api/memory-provenance', { id: c.id, action: 'keep' }); await refreshMemories() }), btn('采用新事实', async () => { await call('/api/memory-provenance', { id: c.id, action: 'replace' }); await refreshMemories() }))); memoryBody.append(item) }
    if (!memoryBody.children.length) memoryBody.append(el('p', '暂无自动记忆或待处理冲突'))
  }
  memories.append(btn('查看来源与冲突', refreshMemories), memoryBody); host.append(memories)

  const learning = detail('可复用技能候选'), candidates = el('div')
  async function refreshLearning() {
    const d = await request('/api/learning'); candidates.replaceChildren(el('p', `最近提取：${d.state?.state || 'idle'} · ${d.state?.tokens ?? '?'} tokens`))
    for (const c of d.candidates) { const item = detail(`${c.title} · ${c.status}`); item.append(el('h4', '当前版本'), el('pre', c.previous || '无', 'feature-file-preview'), el('h4', '候选版本'), el('pre', c.markdown, 'feature-file-preview'), btn(`查看来源会话 ${c.sessionId}`, () => openSession(c.sessionId))); for (const action of c.status === 'pending' ? ['adopt', 'reject'] : c.status === 'adopted' ? ['disable'] : []) item.append(btn({ adopt: '采用', reject: '拒绝', disable: '停用' }[action], async () => { await call('/api/learning', { id: c.id, action }); await refreshLearning() })); candidates.append(item) }
  }
  learning.append(btn('刷新候选', refreshLearning), candidates); host.append(learning)

  const scheduler = detail('定时任务与结果收件箱'), jobText = el('textarea'); bind(jobText, '到时执行的任务', 'placeholder'); bind(jobText, '到时执行的任务', 'aria-label')
  const minutes = input('间隔分钟；填 0 使用每日时间', 'number', 60), daily = input('每日执行时间', 'time', '09:00'), timezone = input('IANA 时区', 'text', Intl.DateTimeFormat().resolvedOptions().timeZone || 'Asia/Shanghai'), catchup = input('错过后补一次', 'checkbox'), jobs = el('div')
  async function refreshJobs() {
    const d = await request('/api/schedules'); jobs.replaceChildren()
    for (const j of d.jobs) jobs.append(row(el('span', `${j.status} · ${j.text} · 下次 ${new Date(j.nextRunAt * 1000).toLocaleString()} (${j.timezone})`), btn(j.status === 'enabled' ? '暂停' : j.status === 'running' ? '取消当前执行' : '恢复', async () => { await call('/api/schedules', { id: j.id, action: j.status === 'enabled' ? 'pause' : j.status === 'running' ? 'cancel' : 'resume' }); await refreshJobs() })))
    jobs.append(el('h4', '本地收件箱')); for (const r of d.inbox) jobs.append(row(btn(`${r.status} · ${new Date(r.at * 1000).toLocaleString()} · 查看结果`, () => openSession(r.sessionId)), el('span', r.error || '')))
  }
  scheduler.append(el('p', '程序运行时执行。默认跳过错过的任务；异常中断后暂停，先检查结果再恢复。'), jobText, row(minutes, daily, timezone), label('错过执行后补一次', catchup), row(btn('创建定时任务', async () => { await call('/api/schedules', { text: jobText.value, intervalSeconds: Number(minutes.value) > 0 ? Number(minutes.value) * 60 : undefined, dailyTime: daily.value, timezone: timezone.value, missedPolicy: catchup.checked ? 'once' : 'skip' }); jobText.value = ''; await refreshJobs() }), btn('刷新任务与收件箱', refreshJobs)), jobs); host.append(scheduler)

  const trees = detail('隔离工作区'), treeName = input('任务名称（英文、数字、短横线）'), treeContent = el('pre', '', 'feature-file-preview')
  async function refreshTrees() { treeContent.textContent = (await request('/api/worktrees')).content }
  trees.append(el('p', '从已提交版本创建独立 Git worktree；当前未提交更改保留在原工作区。新路径可作为独立任务的工作目录。'), row(treeName, btn('创建独立工作区', async () => { const r = await call('/api/worktrees', { name: treeName.value }); await refreshTrees(); status(`已创建 ${r.branch}：${r.path}`) }), btn('刷新列表', refreshTrees)), treeContent); host.append(trees)
  for (const [panel, refresh] of [[goalPanel, refreshGoal], [learning, refreshLearning], [scheduler, refreshJobs], [trees, refreshTrees]]) panel.addEventListener('toggle', () => { if (panel.open) refresh().catch(e => status(e.message, true)) })
  let polling = false
  const timer = setInterval(async () => { if (document.hidden || polling) return; polling = true; try { await Promise.allSettled([refreshGoal(), refreshTakeover(), ...(scheduler.open ? [refreshJobs()] : [])]) } finally { polling = false } }, 3000)
  window.addEventListener('pagehide', () => clearInterval(timer))
  function updateControls() {
    quickReasoning.disabled = !executionReady || state.sending || Boolean(document.querySelector('.composer-toolbar [data-busy]'))
    if (observedSession !== state.currentSessionId) {
      observedSession = state.currentSessionId; goalData = null; goalNext.checked = false; steering.value = ''; renderGoal()
    }
    const disable = (n, unavailable) => { n.disabled = Boolean(unavailable || n.dataset.busy) }
    const goal = goalData?.goal
    disable(goalNext, state.sending)
    disable(pauseGoal, !state.currentSessionId || goal?.status !== 'running')
    disable(resumeGoal, state.sending || !state.currentSessionId || !['paused', 'blocked'].includes(goal?.status))
    disable(steerButton, !state.sending || !state.currentSessionId || !steering.value.trim())
    disable(queueButton, !state.currentSessionId || !steering.value.trim())
  }
  steering.addEventListener('input', updateControls); goalNext.addEventListener('change', renderGoal)
  document.querySelector('.workspace-tools').addEventListener('ui:action-done', () => { updateControls(); renderTakeover() })
  updateControls(); renderGoal()
  return { panels: { goalPanel, review, search, memories, learning, scheduler, trees }, takeover, summary, host, updateControls,
    localize: () => { ui.localize(document); renderGoal(); renderTakeover() },
    requestExtras: text => { const enabled = goalNext.checked; goalNext.checked = false; renderGoal(); return enabled ? { goal: text } : {} } }
}
