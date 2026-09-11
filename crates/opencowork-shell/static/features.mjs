import { initWorkflows } from '/workflows.mjs'
export function initFeaturePanels({ state, request, composer, status, send, openSession }) {
  const tr = (zh, en) => state.locale === 'en' ? en : zh
  const el = (tag, text, className = '') => { const node = document.createElement(tag); node.textContent = text; node.className = className; return node }
  const button = (text, action) => { const node = el('button', text, 'ghost-button'); node.type = 'button'; node.onclick = () => Promise.resolve().then(action).catch(e => status(e.message, true)); return node }
  let references = [], files = [], changes = [], selected = '', previewToken = 0, contextToken = 0, currentId, refreshTimer
  const settings = el('article', '', 'settings-card feature-settings')
  const heading = el('h3', tr('电脑控制与后台记忆', 'Computer control and background memory'))
  const computer = document.createElement('input'); computer.type = 'checkbox'; computer.id = 'computer-enabled'
  const memory = document.createElement('input'); memory.type = 'checkbox'; memory.id = 'background-memory-enabled'
  function toggle(input, text) { const label = el('label', '', 'feature-toggle'); label.append(input, el('span', text)); return label }
  const feedback = el('p', '', 'settings-card-copy'); feedback.setAttribute('role', 'status')
  settings.append(heading,
    toggle(computer, tr('开启电脑控制（默认开启）', 'Enable computer control (on by default)')),
    el('p', tr('允许智能体查看窗口、截图和操作鼠标键盘。关闭时会停止当前任务；也可以随时使用聊天中的停止按钮。', 'Let the agent inspect windows, capture screenshots and use the mouse and keyboard. Disabling stops active turns. The chat Stop button is always available.'), 'settings-card-copy'),
    toggle(memory, tr('完成回复后自动提取项目记忆', 'Extract project memories after successful replies')),
    el('p', tr('独立后台请求，最多每分钟一次；输入最多 24000 字符，输出最多 768 token。生成的记忆可在记忆设置中查看、编辑和删除。', 'Separate background request, at most once per minute; 24,000 input characters and 768 output tokens. Review, edit or delete notes in Memory settings.'), 'settings-card-copy'),
    button(tr('保存功能设置', 'Save feature settings'), async () => {
      const result = await request('/api/features', { method: 'POST', body: JSON.stringify({ computerEnabled: computer.checked, backgroundMemoryEnabled: memory.checked }) })
      computer.checked = result.computerEnabled; memory.checked = result.backgroundMemoryEnabled
      feedback.textContent = tr('设置已保存', 'Settings saved')
    }), feedback)
  document.querySelector('#settings-permission-panel .settings-card-list').prepend(settings)
  request('/api/features').then(data => { computer.checked = data.computerEnabled; memory.checked = data.backgroundMemoryEnabled; computer.disabled = !data.computerSupported; if (!data.computerSupported) feedback.textContent = tr('电脑控制目前支持 Windows。', 'Computer control currently supports Windows.') }).catch(e => { feedback.textContent = e.message })

  const tools = el('section', '', 'workspace-tools')
  const browser = document.createElement('details'); browser.className = 'feature-disclosure'
  browser.append(el('summary', tr('工作区文件与更改', 'Workspace files and changes')))
  const toolbar = el('div', '', 'feature-toolbar')
  const search = document.createElement('input'); search.placeholder = tr('按路径筛选文件', 'Filter files by path'); search.setAttribute('aria-label', search.placeholder)
  const onlyChanges = document.createElement('input'); onlyChanges.type = 'checkbox'
  toolbar.append(search, toggle(onlyChanges, tr('只看更改', 'Changed files only')), button(tr('刷新', 'Refresh'), loadFiles))
  const fileList = el('div', '', 'feature-file-list'); const code = el('pre', '', 'feature-file-preview'); code.tabIndex = 0
  const fileActions = el('div', '', 'feature-toolbar'); const fileTitle = el('strong', tr('选择文件查看内容', 'Choose a file'))
  fileActions.append(fileTitle, button(tr('内容', 'Content'), () => showFile(selected, false)), button(tr('差异（只读）', 'Diff (read only)'), () => showFile(selected, true)), button(tr('添加到对话', 'Attach to chat'), () => attach({ kind: 'file', path: selected })))
  const grid = el('div', '', 'feature-file-grid'); const right = el('div', '', 'feature-file-detail'); right.append(fileActions, code); grid.append(fileList, right)
  browser.append(toolbar, grid)
  browser.addEventListener('toggle', () => { if (browser.open) loadFiles().catch(e => status(e.message, true)) })
  search.oninput = renderFiles; onlyChanges.onchange = renderFiles
  const sessionToolbar = el('div', '', 'feature-toolbar'); const sessions = document.createElement('select'); sessions.setAttribute('aria-label', tr('引用历史会话', 'Reference a previous session'))
  sessionToolbar.append(sessions, button(tr('引用会话', 'Attach session'), () => attach({ kind: 'session', path: sessions.value })))
  browser.append(sessionToolbar)
  const chips = el('div', '', 'feature-references'); const refBudget = el('small', '', 'feature-budget')
  const diagnostics = document.createElement('details'); diagnostics.className = 'feature-disclosure'; diagnostics.append(el('summary', tr('本次上下文来源与用量', 'Context sources and usage')))
  const diagBody = el('div', '', 'feature-diagnostics'); diagnostics.append(diagBody)
  const task = document.createElement('details'); task.className = 'feature-disclosure'; const taskSummary = el('summary', tr('任务步骤', 'Task steps')); const taskBody = el('div', '', 'feature-task'); task.append(taskSummary, taskBody)
  task.append(button(tr('规划新任务', 'Plan a task'), () => { composer.value = tr('请先用 UpdatePlan 制定任务步骤和完成检查，再执行以下任务：\n', 'Use UpdatePlan to define steps and completion checks, then execute this task:\n'); composer.focus() }), button(tr('继续未完成步骤', 'Resume unfinished steps'), () => { if (state.sending) return; composer.value = tr('继续当前保存计划中未完成的步骤，先检查已有结果并更新任务进度。', 'Resume unfinished steps in the saved plan, first checking existing results and updating progress.'); send() }))
  tools.append(browser, chips, refBudget, task, diagnostics)
  document.querySelector('#composer-form').before(tools)

  async function loadFiles() { const data = await request('/api/workspace'); files = data.files; changes = data.changes; renderFiles(); if (data.truncated) status(tr('文件列表已限制为 3000 项', 'File list limited to 3,000 entries')) }
  function renderFiles() {
    const items = onlyChanges.checked ? changes.map(c => c.path) : [...new Set([...files, ...changes.map(c => c.path)])].sort()
    const filtered = items.filter(path => path.toLowerCase().includes(search.value.toLowerCase()))
    fileList.replaceChildren(...filtered.slice(0, 500).map(path => button(`${changes.find(c => c.path === path)?.status || '  '}  ${path}`, () => showFile(path, onlyChanges.checked && !changes.find(c => c.path === path)?.status.includes('?')))))
    if (!filtered.length) fileList.append(el('p', tr('没有匹配的文件', 'No matching files')))
  }
  async function showFile(path, diff) {
    if (!path) return; window.dispatchEvent(new CustomEvent("opencowork:file", {detail:path})); selected = path; fileTitle.textContent = path
    const token = ++previewToken; code.textContent = tr('正在读取…', 'Loading…')
    const data = await request(`/api/workspace/${diff ? 'diff' : 'file'}?path=${encodeURIComponent(path)}`)
    if (token !== previewToken) return
    code.textContent = data.content + (data.truncated ? tr('\n\n[内容已截断]', '\n\n[Truncated]') : '')
  }
  async function attach(ref) {
    if (!ref.path) return
    if (state.sending) throw new Error(tr('请等待当前回复结束', 'Wait for the active reply'))
    if (references.some(r => r.path === ref.path && r.kind === ref.kind)) return
    const next = [...references, ref]
    const budget = await request('/api/references/preview', { method: 'POST', body: JSON.stringify(next) })
    references = next; renderReferences(budget)
  }
  function renderReferences(budget) {
    chips.replaceChildren(...references.map((ref, index) => button(`${ref.kind === 'file' ? '📄' : '💬'} ${ref.path} ×`, async () => {
      references.splice(index, 1); renderReferences(await request('/api/references/preview', { method: 'POST', body: JSON.stringify(references) }))
    })))
    refBudget.textContent = references.length ? tr(`已引用 ${references.length}/8 项 · ${budget?.usedCharacters || 0}/24000 字符 · token 为估算值`, `${references.length}/8 references · ${budget?.usedCharacters || 0}/24000 characters · tokens are estimates`) : ''
    for (const ref of budget?.references || []) if (ref.truncated) refBudget.textContent += tr(` · ${ref.path} 已截断`, ` · ${ref.path} truncated`)
  }
  async function refresh() {
    const id = state.currentSessionId
    if (currentId !== id) { currentId = id; references = []; renderReferences(); taskBody.replaceChildren(); diagBody.replaceChildren() }
    sessions.replaceChildren(...(state.bootstrap?.sessions || []).filter(s => s.id !== id).map(s => { const option = el('option', s.title); option.value = s.id; return option }))
    if (!id) return
    const token = ++contextToken
    const [plan, context] = await Promise.all([request(`/api/task-plan/${encodeURIComponent(id)}`), request(`/api/context-diagnostics/${encodeURIComponent(id)}`)])
    if (token !== contextToken || id !== state.currentSessionId) return
    taskBody.replaceChildren()
    const steps = plan?.steps || []; const completed = steps.filter(s => s.status === 'completed').length
    taskSummary.textContent = tr(`任务步骤 ${completed}/${steps.length}`, `Task steps ${completed}/${steps.length}`)
    if (plan?.goal) taskBody.append(el('strong', plan.goal))
    for (const step of steps) {
      const row = el('div', '', 'feature-step'); const statusText = { pending: tr('待开始','Pending'), in_progress: tr('进行中','In progress'), completed: tr('已完成','Completed'), interrupted: tr('已中断，可继续','Interrupted; resumable') }[step.status]
      row.append(el('strong', `${statusText} · ${step.title}`), el('p', `${tr('完成条件：','Completion check: ')}${step.check}`))
      if (step.evidence) row.append(el('p', `${tr('验证结果：','Evidence: ')}${step.evidence}`))
      taskBody.append(row)
    }
    diagBody.replaceChildren()
    if (context) {
      diagBody.append(el('p', tr(`估算提示词 ${context.estimatedPromptTokens} tokens · 窗口 ${context.contextWindowTokens} · 指令预算 ${context.instructionTokenBudget} · ${context.compacted ? '已压缩历史' : '未压缩历史'}`, `Estimated prompt ${context.estimatedPromptTokens} tokens · window ${context.contextWindowTokens} · instruction budget ${context.instructionTokenBudget} · compacted: ${context.compacted}`)))
      for (const layer of context.layers || []) diagBody.append(el('p', `${layer.name} · ~${layer.estimatedTokens} tokens · ${layer.reason || ""}${layer.truncated ? tr(" · 已截断", " · truncated") : ""}`))
      for (const ref of context.attachments?.references || []) diagBody.append(el('p', `${ref.kind}: ${ref.path} · ~${ref.estimatedTokens} tokens${ref.truncated ? tr(' · 已截断',' · truncated') : ''}`))
    } else diagBody.append(el('p', tr('完成一轮对话后显示实际选用的上下文来源。', 'Sources selected for the turn appear after a reply completes.')))
  }
  function scheduleRefresh() { if (refreshTimer) return; refreshTimer = setTimeout(() => { refreshTimer = null; refresh().catch(e => status(e.message, true)) }, 150) }
  const polling = setInterval(() => { if (document.hidden) return; if (state.sending) scheduleRefresh(); if (memory.checked && state.currentView === 'settings') request('/api/background-memory').then(s => {feedback.textContent = tr(`后台记忆：${s.state}${s.notesWritten != null ? `，新增 ${s.notesWritten} 条` : ''}`, `Background memory: ${s.state}${s.notesWritten != null ? `, ${s.notesWritten} notes` : ''}`)}).catch(() => {}) }, 2000)
  window.addEventListener('pagehide', () => clearInterval(polling))
  const workflows = initWorkflows({ state, request, composer, status, send, openSession })
  return { requestExtras: workflows.requestExtras, references: () => [...references], refresh: scheduleRefresh, clear: () => {references=[];renderReferences()} }
}
