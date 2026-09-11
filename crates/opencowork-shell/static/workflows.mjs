export function initWorkflows({ state, request, composer, status, send, openSession }) {
  const el = (tag, text = '', cls = '') => { const n = document.createElement(tag); n.textContent = text; n.className = cls; return n }
  const call = (path, data) => request(path, { method: 'POST', body: JSON.stringify(data) })
  const btn = (text, fn) => { const n = el('button', text, 'ghost-button'); n.type = 'button'; n.onclick = () => Promise.resolve().then(fn).catch(e => status(e.message, true)); return n }
  const input = (label, type = 'text', value = '') => { const n = el('input'); n.type = type; n.value = value; n.setAttribute('aria-label', label); n.placeholder = label; return n }
  const label = (text, n) => { const l = el('label', '', 'feature-toggle'); l.append(n, el('span', text)); return l }
  const detail = title => { const n = el('details', '', 'feature-disclosure'); n.append(el('summary', title)); return n }
  const row = (...nodes) => { const n = el('div', '', 'feature-toolbar'); n.append(...nodes); return n }
  const host = el('section', '', 'workflow-panels'); document.querySelector('.workspace-tools').append(host)
  const goalPanel = detail('目标、补充要求与消息队列')
  const goalNext = input('把本次消息设为目标', 'checkbox')
  const goalState = el('p', '发送目标后，在预算内持续执行并检查完成证据。')
  const steering = el('textarea'); steering.placeholder = '运行中补充要求，或排入下一轮'; steering.setAttribute('aria-label', steering.placeholder); steering.rows = 2
  const queue = el('div')
  const requireId = () => { if (!state.currentSessionId) throw new Error('请先发送一条消息创建会话'); return encodeURIComponent(state.currentSessionId) }
  const workflow = v => call(`/api/workflows/${requireId()}`, v)
  async function enqueue(mode) {
    if (!steering.value.trim()) return
    await workflow({ action: mode, id: crypto.randomUUID(), text: steering.value }); steering.value = ''; await refreshGoal()
    if (!state.sending) { composer.value = '处理已保存的待办消息，先检查已有结果。'; send() }
  }
  goalPanel.append(label('把本次消息设为目标并自动继续', goalNext), goalState, row(
    btn('暂停目标', async () => { await workflow({ action: 'pause' }); await refreshGoal() }),
    btn('恢复目标', async () => { if (state.sending) return; await workflow({ action: 'resume' }); composer.value = '继续已保存目标，检查已有结果，按缺失证据执行。'; send() })), steering,
    row(btn('补充当前任务', () => enqueue('steer')), btn('排入下一轮', () => enqueue('queue'))), queue)
  async function refreshGoal() {
    if (!state.currentSessionId) return
    const id = state.currentSessionId, data = await request(`/api/workflows/${encodeURIComponent(id)}`); if (id !== state.currentSessionId) return
    const g = data.goal; goalState.textContent = g?.text ? `${g.status} · ${g.rounds}/${g.maxRounds} 轮 · ${g.tokens}/${g.maxTokens} tokens\n${g.text}\n${g.reason || ''}` : '尚未设置目标'
    queue.replaceChildren(...(data.messages || []).filter(m => m.status === 'pending').map(m => row(el('span', `${m.mode === 'steer' ? '补充' : '排队'}：${m.text}`), btn('移除', async () => { await workflow({ action: 'discard', id: m.id }); await refreshGoal() }))))
  }
  const takeover = el('p'); takeover.append(btn('接管电脑并停止任务', async () => { await call('/api/computer/takeover', { paused: true }); status('已暂停电脑操作，可以手动接管。') }), btn('允许继续电脑操作', async () => { await call('/api/computer/takeover', { paused: false }); status('已恢复电脑操作权限；从新观察继续。') }))
  host.append(takeover, goalPanel)

  const settings = el('article', '', 'settings-card workflow-settings'); settings.append(el('h3', '自动执行与模型能力'))
  const advanced = detail('高级设置：预算与后台模型')
  const fields = { maxIterations: ['每轮最多迭代', 80], maxTokens: ['每轮 token 上限', 250000], maxSeconds: ['每轮秒数上限', 900], repeatedResults: ['重复结果停止阈值', 3], maxOutputTokens: ['单次最大输出 token', 8192] }
  const values = {}
  for (const [key, [name, value]] of Object.entries(fields)) { values[key] = input(name, 'number', value); values[key].min = '1'; advanced.append(label(name, values[key])) }
  const background = input('后台模型名称（留空跟随当前模型）'), reasoning = el('select'); reasoning.setAttribute('aria-label', '推理强度')
  for (const [value, name] of [['', '推理强度：提供方默认'], ['none', '无'], ['minimal', '最少'], ['low', '低'], ['medium', '中'], ['high', '高']]) { const n = el('option', name); n.value = value; reasoning.append(n) }
  const browser = input('浏览器工具', 'checkbox'), learn = input('生成技能候选', 'checkbox'), auto = input('自动采用技能', 'checkbox'); browser.checked = true
  const settingsState = el('p', '', 'settings-card-copy'), probe = el('pre', '', 'feature-file-preview')
  advanced.append(label('后台模型', background), label('推理强度', reasoning))
  settings.append(label('启用独立浏览器工具（需安装 Chrome 或 Edge）', browser), label('成功流程生成技能候选（额外后台请求）', learn), label('自动采用候选技能（关闭时先审阅）', auto), advanced, row(btn('保存执行设置', async () => {
    const execution = Object.fromEntries(Object.entries(values).map(([k, n]) => [k, Number(n.value)])); execution.backgroundModel = background.value.trim(); execution.reasoningEffort = reasoning.value
    await call('/api/execution-settings', { execution, browser: { enabled: browser.checked }, learning: { enabled: learn.checked, autoAdopt: auto.checked } }); settingsState.textContent = '已保存，新回合生效。'
  }), btn('检测模型连接、工具、图像和流式能力', async () => { probe.textContent = '正在检测（最多 4 次小请求，可能产生用量）…'; const data = await call('/api/provider-probe', {}); probe.textContent = `${data.model} · ${data.protocol}\n` + data.checks.map(c => `${c.kind}: ${c.status} · ${c.elapsedMs} ms · ${c.tokens ?? '?'} tokens ${c.error || ''}`).join('\n') + '\n推理强度仅验证参数兼容性，不代表任务能力。' })), settingsState, probe)
  document.querySelector('#settings-permission-panel .settings-card-list').append(settings)
  request('/api/execution-settings').then(d => { for (const [k, n] of Object.entries(values)) n.value = d.execution?.[k] ?? (k === 'maxOutputTokens' ? d.maxOutputTokens : fields[k][1]); background.value = d.execution?.backgroundModel || ''; reasoning.value = d.execution?.reasoningEffort || ''; browser.checked = d.browser?.enabled !== false; learn.checked = d.learning?.enabled === true; auto.checked = d.learning?.autoAdopt === true }).catch(e => { settingsState.textContent = e.message })

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

  const scheduler = detail('定时任务与结果收件箱'), jobText = el('textarea'); jobText.placeholder = '到时执行的任务'; jobText.setAttribute('aria-label', jobText.placeholder)
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
  const timer = setInterval(async () => { if (document.hidden || polling) return; polling = true; try { await Promise.allSettled([...(goalPanel.open ? [refreshGoal()] : []), ...(scheduler.open ? [refreshJobs()] : [])]) } finally { polling = false } }, 3000)
  window.addEventListener('pagehide', () => clearInterval(timer))
  return { panels: { goalPanel, review, search, memories, learning, scheduler, trees }, takeover, host, requestExtras: text => { const enabled = goalNext.checked; goalNext.checked = false; return enabled ? { goal: text } : {} } }
}
