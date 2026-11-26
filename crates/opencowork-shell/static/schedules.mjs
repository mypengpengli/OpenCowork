import { createUi } from '/ui-controls.mjs'

export function initSchedules({ state, request, openSession }) {
  const root = document.querySelector('#schedules-view')
  const feedback = document.createElement('p'); feedback.setAttribute('role', 'status'); feedback.className = 'schedule-feedback'
  const ui = createUi(state, (message, error) => { feedback.textContent = message; feedback.dataset.error = String(Boolean(error)) })
  const { tr, el, button, bind } = ui
  const heading = el('header', '', 'page-header'), title = el('div')
  title.append(el('h2', tr('定时任务', 'Scheduled tasks')), el('p', tr('安排智能体自动执行，集中查看每次运行的结果。', 'Schedule agent work and review the results in one place.')))
  const content = el('div', '', 'schedule-content'), list = el('div', '', 'schedule-list'), inbox = el('div', '', 'schedule-list')
  const form = el('form', '', 'schedule-editor settings-card'); form.hidden = true
  const formTitle = el('h3'), fields = el('div', '', 'schedule-fields')
  function field(cn, en, input) {
    const label = el('label'); label.append(el('span', tr(cn, en)), input); fields.append(label); return input
  }
  function input(id, type = 'text') { const n = el('input'); n.id = id; n.type = type; return n }
  const name = field('任务名称', 'Task name', input('schedule-name')); name.maxLength = 120; name.required = true
  const text = field('任务内容', 'Instructions', el('textarea')); text.id = 'schedule-text'; text.rows = 4; text.required = true
  const mode = el('select'); mode.id = 'schedule-mode'
  for (const [value, cn, en] of [['interval', '按间隔执行', 'Repeat at an interval'], ['daily', '每天指定时间', 'Daily at a set time']]) { const option = el('option', tr(cn, en)); option.value = value; mode.append(option) }
  field('执行方式', 'Schedule', mode)
  const minutes = field('间隔（分钟）', 'Interval (minutes)', input('schedule-minutes', 'number')); minutes.min = 1; minutes.max = 525600; minutes.step = 1
  const daily = field('每天执行时间', 'Daily time', input('schedule-daily', 'time'))
  const timezone = field('时区', 'Time zone', input('schedule-timezone'))
  const catchup = field('错过后补执行一次', 'Run once after a missed occurrence', input('schedule-catchup', 'checkbox'))
  const actions = el('div', '', 'schedule-actions'), save = el('button', tr('保存任务', 'Save task'), 'primary-button'); save.type = 'submit'
  let editing = null, data = { jobs: [], inbox: [] }, busy = false, generation = 0
  actions.append(save, button(tr('取消', 'Cancel'), () => { form.hidden = true; editing = null }))
  form.append(formTitle, fields, actions)
  function updateMode() {
    const interval = mode.value === 'interval'
    minutes.parentElement.hidden = !interval; minutes.required = interval; minutes.disabled = !interval
    daily.parentElement.hidden = interval; daily.required = !interval; daily.disabled = interval
    timezone.parentElement.hidden = interval; timezone.required = !interval; timezone.disabled = interval
  }
  mode.addEventListener('change', updateMode)
  function edit(job = null) {
    if (busy) return
    editing = job?.id || null; form.hidden = false
    formTitle.textContent = job ? tr('编辑定时任务', 'Edit scheduled task') : tr('新建定时任务', 'New scheduled task')
    name.value = job?.name || ''; text.value = job?.text || ''
    mode.value = job?.dailyTime && !job?.intervalSeconds ? 'daily' : 'interval'
    minutes.value = String((job?.intervalSeconds || 3600) / 60); daily.value = job?.dailyTime || '09:00'
    timezone.value = job?.timezone || Intl.DateTimeFormat().resolvedOptions().timeZone || 'Asia/Shanghai'
    catchup.checked = job?.missedPolicy === 'once'; updateMode(); name.focus()
  }
  const call = payload => request('/api/schedules', { method: 'POST', body: JSON.stringify(payload) })
  form.addEventListener('submit', async event => {
    event.preventDefault(); if (busy || !form.reportValidity()) return
    busy = true; save.disabled = true
    // Lock the draft while the request is in flight; failures retain all inputs.
    const controls = [...form.querySelectorAll('input, textarea, select, button')]
    const disabled = controls.map(n => n.disabled); controls.forEach(n => { n.disabled = true })
    try {
      await call({ action: editing ? 'update' : 'create', ...(editing ? { id: editing } : {}), name: name.value.trim(), text: text.value,
        intervalSeconds: mode.value === 'interval' ? Number(minutes.value) * 60 : null,
        dailyTime: mode.value === 'daily' ? daily.value : null, timezone: timezone.value, missedPolicy: catchup.checked ? 'once' : 'skip' })
      form.hidden = true; editing = null; feedback.textContent = tr('任务已保存', 'Task saved'); feedback.dataset.error = 'false'; await refresh()
    } catch (error) { feedback.textContent = error.message; feedback.dataset.error = 'true' }
    finally { busy = false; controls.forEach((n, i) => { n.disabled = disabled[i] }); save.disabled = false; updateMode() }
  })
  heading.append(title, button(tr('新建任务', 'New task'), () => edit()))
  content.append(el('p', tr('需要保持程序运行。默认跳过错过的执行；失败或异常中断后暂停，可检查结果后恢复。', 'Keep the app running. Missed occurrences are skipped by default. Failed or interrupted tasks pause until you review and resume them.'), 'schedule-notice'), feedback, form,
    el('div', '', 'schedule-actions'), list, el('h3', tr('执行记录', 'Run history')), inbox)
  content.children[3].append(el('h3', tr('我的任务', 'My tasks')), button(tr('刷新', 'Refresh'), refresh))
  root.append(heading, content)
  const date = value => value ? new Date(value * 1000).toLocaleString(state.locale === 'en' ? 'en-US' : 'zh-CN') : '—'
  const status = value => ({ enabled: tr('已启用', 'Enabled'), paused: tr('已暂停', 'Paused'), running: tr('执行中', 'Running'), completed: tr('已完成', 'Completed'), failed: tr('失败', 'Failed'), cancelled: tr('已取消', 'Cancelled'), unknown: tr('结果未知', 'Unknown'), interrupted: tr('已中断', 'Interrupted') })[value] || value
  async function change(job, action) { await call({ id: job.id, action }); await refresh() }
  function render() {
    list.replaceChildren(); inbox.replaceChildren()
    if (!data.jobs.length) list.append(el('p', tr('还没有定时任务，点击“新建任务”添加。', 'No scheduled tasks yet. Choose New task to add one.'), 'schedule-empty'))
    for (const job of data.jobs) {
      const card = el('article', '', 'schedule-card'); card.dataset.jobId = job.id
      const top = el('div', '', 'schedule-actions'); top.append(el('h3', job.name || job.text.slice(0, 40)), el('span', status(job.status), 'status-tag'))
      const rule = job.intervalSeconds ? tr(`每 ${job.intervalSeconds / 60} 分钟`, `Every ${job.intervalSeconds / 60} minutes`) : `${tr('每天', 'Daily')} ${job.dailyTime} · ${job.timezone}`
      card.append(top, el('p', job.text, 'schedule-instructions'), el('p', `${rule} · ${tr('下次执行', 'Next run')}: ${job.status === 'enabled' ? date(job.nextRunAt) : '—'}`, 'schedule-meta'))
      if (job.lastRunAt) card.append(el('p', `${tr('最近执行', 'Last run')}: ${date(job.lastRunAt)} · ${status(job.lastStatus)}`, 'schedule-meta'))
      if (job.lastError || job.error) card.append(el('p', job.lastError || job.error, 'schedule-error'))
      const row = el('div', '', 'schedule-actions')
      if (job.status === 'running') row.append(button(tr('取消当前执行', 'Cancel current run'), () => change(job, 'cancel')))
      else row.append(button(tr('编辑', 'Edit'), () => edit(job)), button(job.status === 'enabled' ? tr('暂停', 'Pause') : tr('恢复', 'Resume'), () => change(job, job.status === 'enabled' ? 'pause' : 'resume')),
        button(tr('立即执行', 'Run now'), async () => {
          if (confirm(tr('现在执行此任务？将使用当前模型、工具权限和执行预算；执行后会继续按计划运行。', 'Run this task now using the current model, tool permissions and budget? Its schedule will remain enabled.'))) {
            await change(job, 'runNow'); feedback.textContent = tr('已安排执行，请稍候查看运行状态。', 'Queued. Check the run status shortly.')
          }
        }), button(tr('删除', 'Delete'), async () => { if (confirm(tr('删除这个定时任务？已有执行记录会保留。', 'Delete this schedule? Existing run history will be kept.'))) await change(job, 'delete') }))
      card.append(row); list.append(card)
    }
    if (!data.inbox.length) inbox.append(el('p', tr('暂无执行记录。', 'No runs yet.'), 'schedule-empty'))
    for (const run of data.inbox) {
      const card = el('article', '', 'schedule-card'), job = data.jobs.find(j => j.id === run.jobId)
      card.append(el('strong', run.jobName || job?.name || tr('定时任务', 'Scheduled task')), el('p', `${date(run.at)} · ${status(run.status)}`, 'schedule-meta'))
      if (run.error) card.append(el('p', run.error, 'schedule-error'))
      if (run.sessionId) card.append(button(tr('查看结果', 'View result'), () => openSession(run.sessionId)))
      inbox.append(card)
    }
  }
  async function refresh() {
    const token = ++generation
    try { const result = await request('/api/schedules'); if (token === generation) { data = result; render() } }
    catch (error) { if (token === generation) { feedback.textContent = error.message; feedback.dataset.error = 'true' } }
  }
  const timer = setInterval(() => { if (!document.hidden && state.currentView === 'schedules' && !busy && !root.querySelector('[data-busy]')) refresh() }, 3000)
  window.addEventListener('pagehide', () => clearInterval(timer), { once: true })
  window.addEventListener('opencowork:locale-changed', () => { ui.localize(root); if (!form.hidden) formTitle.textContent = editing ? tr('编辑定时任务', 'Edit scheduled task') : tr('新建定时任务', 'New scheduled task'); render() })
  bind(root, tr('定时任务', 'Scheduled tasks'), 'aria-label')
  return { refresh }
}
