export function initSubagents({ ui, request, state, tools }) {
  const { tr, el, button, bind } = ui
  const settings = el('article', '', 'settings-card')
  const concurrency = document.createElement('input')
  concurrency.type = 'number'
  concurrency.min = '1'
  concurrency.step = '1'
  concurrency.value = '6'
  concurrency.id = 'subagent-concurrency'
  const depth = document.createElement('input')
  depth.type = 'number'
  depth.min = '1'
  depth.max = '16'
  depth.value = '1'
  depth.id = 'subagent-depth'
  const field = (text, input) => { const label = el('label'); label.append(el('span', text), input); return label }
  const feedback = el('p', '', 'settings-card-copy')
  feedback.setAttribute('role', 'status')
  settings.append(el('h3', tr('子智能体', 'Subagents')),
    field(tr('并发子智能体数量', 'Concurrent subagents'), concurrency),
    field(tr('最大委派深度', 'Maximum delegation depth'), depth),
    el('p', tr('主智能体单独计数，超出并发额度的任务排队。子任务共享用量与时间预算。', 'The main agent is counted separately. Extra tasks queue, and children share the usage and time budget.'), 'settings-card-copy'),
    button(tr('保存子智能体设置', 'Save subagent settings'), async () => {
      if (!concurrency.reportValidity() || !depth.reportValidity()) return
      const data = await request('/api/subagents', { method: 'POST', body: JSON.stringify({ maxConcurrent: Number(concurrency.value), maxDepth: Number(depth.value) }) })
      feedback.textContent = tr('设置已保存', 'Settings saved')
      render(data)
    }), feedback)
  document.querySelector('#settings-permission-panel .settings-card-list').append(settings)
  const panel = document.createElement('details')
  panel.className = 'feature-disclosure'
  const summary = el('summary', tr('子智能体任务', 'Subagent tasks'))
  const list = el('div', '', 'feature-file-list')
  const createForm=el('div','','settings-card')
  const name=document.createElement('input');name.placeholder=tr('任务名称（英文、数字、短横线）','Task name (letters, numbers, dashes)');name.maxLength=32;name.id='subagent-task-name'
  const prompt=document.createElement('textarea');prompt.placeholder=tr('任务、约束与完成检查','Task, constraints and acceptance checks');prompt.id='subagent-task-prompt'
  const model=document.createElement('input');model.placeholder=tr('模型（留空继承）','Model (inherit if empty)');model.id='subagent-task-model'
  const effort=document.createElement('select');for(const value of ['', 'low','medium','high','xhigh']){const option=el('option',value||tr('继承推理强度','Inherit reasoning effort'));option.value=value;effort.append(option)}
  const fork=document.createElement('input');fork.type='checkbox';fork.id='subagent-task-fork'
  const writable=document.createElement('input');writable.type='checkbox';writable.id='subagent-task-writable'
  createForm.append(el('h3',tr('委派新任务','Delegate a task')),field(tr('名称','Name'),name),field(tr('任务要求','Task requirements'),prompt),field(tr('模型','Model'),model),field(tr('推理强度','Reasoning effort'),effort),field(tr('继承当前会话','Fork current session'),fork),field(tr('允许写入（需当前权限允许）','Allow writes (requires workspace permission)'),writable),button(tr('创建子任务','Create child task'),async()=>{
    await request('/api/subagent-tasks',{method:'POST',body:JSON.stringify({name:name.value.trim(),task:prompt.value.trim(),model:model.value.trim()||null,reasoningEffort:effort.value||null,forkFrom:fork.checked?state.currentSessionId:null,writable:writable.checked})});prompt.value='';await refresh()
  }))
  const processes=el('div','','feature-file-list')
  const labels = { queued: ['排队', 'Queued'], running: ['运行', 'Running'], waiting: ['等待子任务', 'Waiting'], ready: ['等待执行额度', 'Waiting for capacity'], completed: ['完成', 'Completed'], failed: ['失败', 'Failed'], interrupted: ['中断', 'Interrupted'], cancelled: ['已取消', 'Cancelled'] }
  panel.append(summary, button(tr('刷新', 'Refresh'), refresh), createForm,list,el('h3',tr('后台进程与终端','Background processes and terminals')),processes)
  tools.append(panel)
  let inFlight = false
  let localized = state.locale
  function render(data) {
    const running = data.tasks.filter(task => task.status === 'running').length
    const queued = data.tasks.filter(task => task.status === 'queued' || task.status === 'ready').length
    summary.textContent = `${tr('子智能体任务', 'Subagent tasks')} · ${tr('运行', 'Running')} ${running} · ${tr('排队', 'Queued')} ${queued}`
    if(list.contains(document.activeElement)) return
    const focused = document.activeElement
    if (focused !== concurrency && focused !== depth) {
      concurrency.value = data.settings.maxConcurrent ?? 6
      depth.value = data.settings.maxDepth ?? 1
    }
    const open = new Set([...list.querySelectorAll('details[open]')].map(node => node.dataset.id))
    list.replaceChildren()
    const tasks = data.tasks.slice().sort((a, b) => b.createdAt - a.createdAt).slice(0, 100)
    if (!tasks.length) list.append(el('p', tr('暂无子任务', 'No child tasks')))
    for (const task of tasks) {
      const item = document.createElement('details')
      item.dataset.id = task.id
      item.open = open.has(task.id)
      const label = labels[task.status] || [task.status, task.status]
      item.append(el('summary', `${'↳ '.repeat(Math.min(task.depth, 4))}${task.name} · ${state.locale === 'zh' ? label[0] : label[1]}`))
      item.append(el('p', `${task.model || tr('继承模型', 'Inherited model')} · ${task.usage ? Object.values(task.usage).reduce((sum, n) => sum + n, 0) : 0} tokens`))
      item.append(el('p',`${tr('耗时','Elapsed')} ${(( (task.finishedAt||Date.now())-(task.startedAt||task.createdAt))/1000).toFixed(1)}s${task.reasoningEffort?` · ${task.reasoningEffort}`:''}${task.resumedFrom?` · ${tr('续办自','Continued from')} ${task.resumedFrom}`:''}`))
      if(task.affectedPaths?.length)item.append(el('pre',task.affectedPaths.join('\n')))
      if(task.evidence?.length)item.append(el('pre',task.evidence.join('\n')))
      const text = el('pre', task.result || task.error || task.prompt, 'feature-file-preview')
      item.append(text)
      if (['queued', 'running', 'waiting', 'ready'].includes(task.status)) item.append(button(tr('取消', 'Cancel'), async () => {
        await request(`/api/subagents/${encodeURIComponent(task.id)}/cancel`, { method: 'POST' })
        await refresh()
      }))
      if(['failed','cancelled','interrupted'].includes(task.status))item.append(button(tr('检查后恢复任务','Resume after inspection'),async()=>{await request(`/api/subagents/${encodeURIComponent(task.id)}/resume`,{method:'POST'});await refresh()}))
      if(['queued','running','waiting','ready'].includes(task.status)){
        const steer=document.createElement('textarea');steer.placeholder=tr('补充要求','Additional instructions');
        item.append(steer,button(tr('发送补充要求','Send instructions'),async()=>{await request(`/api/subagents/${encodeURIComponent(task.id)}/message`,{method:'POST',body:JSON.stringify({text:steer.value})});steer.value='';steer.blur();await refresh()}))
      }
      list.append(item)
    }
  }
  async function refresh() {
    if (inFlight) return
    inFlight = true
    try {
      const [data,records]=await Promise.all([request('/api/subagents'),request('/api/process-sessions')]);render(data)
      processes.replaceChildren(...records.map(record=>{const item=document.createElement('details');item.append(el('summary',`${record.id} · ${record.pty?'PTY':'pipe'} · ${record.status}`),el('pre',record.command));const output=el('pre','','feature-file-preview');item.append(output,button(tr('读取日志','Read output'),async()=>{const data=await request(`/api/process-sessions/${encodeURIComponent(record.id)}`);output.textContent=data.output}));if(['queued','running'].includes(record.status))item.append(button(tr('取消进程','Cancel process'),async()=>{await request(`/api/process-sessions/${encodeURIComponent(record.id)}/cancel`,{method:'POST'});await refresh()}));return item}))
    } catch (error) { feedback.textContent = error.message } finally { inFlight = false }
  }
  panel.addEventListener('toggle', () => { if (panel.open) void refresh() })
  const timer = setInterval(() => {
    if (!document.hidden && panel.open) void refresh()
    if (localized !== state.locale) { localized = state.locale; bind(summary, tr('子智能体任务', 'Subagent tasks')); void refresh() }
  }, 1500)
  window.addEventListener('pagehide', () => clearInterval(timer), { once: true })
  void refresh()
  return panel
}
