export function initTasks({ page, request, refresh }) {
  const list = page.querySelector('#assistant-cases')
  const grants = page.querySelector('#assistant-grants')
  const feedback = page.querySelector('#assistant-case-status')
  const heading = page.querySelector('#assistant-cases-title')
  let signature = ''
  let grantSignature = ''
  const tr = (zh, en) => document.documentElement.lang.startsWith('en') ? en : zh
  const base = id => '/api/assistant/cases/' + encodeURIComponent(id)

  function node(tag, text, className) {
    const el = document.createElement(tag)
    if (text) el.textContent = text
    if (className) el.className = className
    return el
  }

  function action(zh, en, run, className = 'ghost-button') {
    const el = node('button', tr(zh, en), className)
    el.type = 'button'
    el.addEventListener('click', async () => {
      feedback.textContent = ''
      el.disabled = true
      try {
        await run()
        await refresh()
      } catch (error) {
        feedback.textContent = error.message
      } finally {
        el.disabled = false
      }
    })
    return el
  }

  const post = (path, body) => request(path, {
    method: 'POST',
    ...(body ? { body: JSON.stringify(body) } : {}),
  })

  function stateText(state) {
    return ({
      working: tr('办理中', 'In progress'), waiting: tr('等回复', 'Waiting for information'),
      done: tr('已完成', 'Done'), cancelled: tr('已关闭', 'Closed'),
      waitingTarget: tr('等待目标窗口空闲', 'Waiting for the target window'),
      running: tr('助手正在办理', 'Assistant working'), stopped: tr('已停止', 'Stopped'),
      unverified: tr('结果待核实', 'Needs verification'),
    })[state] ?? state
  }

  function nameSelect(items, prompt) {
    const select = node('select')
    select.setAttribute('aria-label', prompt)
    const blank = node('option', prompt)
    blank.value = ''
    select.append(blank)
    for (const entry of items) {
      const option = node('option', entry.title)
      option.value = entry.id
      select.append(option)
    }
    return select
  }

  function addEdit(row, item, availableCases) {
    const controls = node('div', '', 'assistant-row')
    const input = node('input')
    input.value = item.title
    input.maxLength = 160
    input.setAttribute('aria-label', tr('事项名称', 'Case title'))
    controls.append(input, action('改名', 'Rename', () => post(base(item.id), {
      version: item.version, title: input.value,
    })))
    if (item.status === 'waiting') controls.append(action('继续办理', 'Resume', () => post(base(item.id), {
      version: item.version, status: 'working',
    })))
    if (item.status === 'working') controls.append(action('标记等回复', 'Wait for reply', () => post(base(item.id), {
      version: item.version, status: 'waiting',
    })))
    controls.append(action('关闭事项', 'Close case', () => post(base(item.id), {
      version: item.version, status: 'cancelled',
    })))
    row.append(controls)
    const outcome = node('div', '', 'assistant-row')
    const result = node('input')
    result.value = item.outcome ?? ''
    result.maxLength = 850
    result.placeholder = tr('填写核实过的结果', 'Enter the verified result')
    result.setAttribute('aria-label', result.placeholder)
    outcome.append(result, action('确认办完并整理 Skill', 'Complete and draft Skill', () => post(base(item.id), {
      version: item.version, status: 'done', outcome: result.value,
    })))
    row.append(outcome)
    if (availableCases.length) {
      const merge = node('div', '', 'assistant-row')
      const select = nameSelect(availableCases, tr('选择另一笔事项', 'Select another case'))
      const apply = action('归并到当前事项', 'Merge into this case', () => post(base(item.id), {
        version: item.version, mergeFrom: select.value,
      }))
      apply.disabled = true
      select.addEventListener('change', () => { apply.disabled = !select.value })
      merge.append(select, apply)
      row.append(merge)
    }
  }

  function addPlan(row, item, data) {
    const plan = item.plan
    if (!plan) return
    const section = node('div', '', 'assistant-prediction')
    section.append(
      node('strong', tr('执行方案', 'Execution plan')),
      node('p', [plan.site, plan.title].filter(Boolean).join(' · ')),
      node('p', tr('起始依据 ', 'Starting evidence ') + plan.before),
    )
    if (plan.reply) {
      section.append(
        node('p', tr('三局通收件人 ', 'Sanjutong recipient ') + plan.reply.recipient),
        node('p', tr('代发正文 ', 'Message to send ') + plan.reply.message),
      )
    } else {
      const steps = node('ol')
      const verbs = {
        setValue: tr('填写', 'Fill'), invoke: tr('触发', 'Activate'),
        select: tr('选择', 'Select'), click: tr('点击', 'Click'),
      }
      for (const step of plan.steps) {
        steps.append(node('li', [verbs[step.action], step.control, step.value].filter(value => value != null).join(' · ')))
      }
      section.append(steps, node('p', tr('完成时核对 ', 'Verify on completion ') + plan.verify))
    }
    if (!data.enabled || data.executionInFlight || item.run?.changed > 0 || item.plan.observationId !== data.observations[0]?.id) {
      section.append(node('p', tr('请先回到目标页面刷新观察，或核实上次执行的结果。', 'Refresh the target observation or verify the previous result.'), 'field-hint'))
    } else {
      const approvals = node('div', '', 'assistant-row')
      const approve = mode => post(base(item.id) + '/approve', { planId: plan.id, mode })
      approvals.append(
        plan.reply
          ? action('允许发送这条消息', 'Send this message', () => approve('once'), 'primary-button')
          : action('允许本次', 'Allow once', () => approve('once'), 'primary-button'),
        action('同一方案允许 5 分钟', 'Allow this plan for 5 minutes', () => approve('timed')),
        action('长期允许同一方案', 'Always allow this plan', () => approve('always')),
      )
      section.append(approvals)
    }
    row.append(section)
  }

  function addDraft(row, item, data) {
    if (!item.draft) {
      if (data.drafting.includes(item.id)) row.append(node('p', tr('正在整理 Skill…', 'Drafting Skill…')))
      else row.append(action('整理 Skill', 'Draft Skill', () => post(base(item.id) + '/draft')))
      return
    }
    const draft = item.draft
    const section = node('div', '', 'assistant-prediction')
    section.append(node('strong', draft.title), node('p', draft.trigger))
    for (const [label, lines] of [
      [tr('操作步骤', 'Steps'), draft.steps],
      [tr('完成检查', 'Checks'), draft.checks],
      [tr('例外', 'Exceptions'), draft.exceptions],
    ]) {
      if (!lines.length) continue
      section.append(node('p', label))
      const items = node('ol')
      lines.forEach(line => items.append(node('li', line)))
      section.append(items)
    }
    if (draft.installed) {
      section.append(
        node('p', tr('已采纳 · ', 'Installed · ') + draft.installed),
        action('停用 Skill', 'Disable Skill', () => request(base(item.id) + '/skill', { method: 'DELETE' })),
      )
    } else {
      section.append(action('启用 Skill', 'Enable Skill', () => post(base(item.id) + '/skill')))
    }
    row.append(section)
  }

  function render(data) {
    heading.textContent = tr('当前事项', 'Cases')
    const cases = data.cases ?? []
    const active = document.activeElement
    if (list.contains(active) && ['INPUT', 'SELECT', 'TEXTAREA'].includes(active.tagName)) return
    const open = cases.filter(entry => ['working', 'waiting'].includes(entry.status))
    const current = JSON.stringify([document.documentElement.lang, cases, data.drafting, data.enabled, data.executionInFlight, data.observations[0]?.id])
    if (current !== signature) {
      signature = current
      const opened = new Set([...list.children].filter(row => row.open).map(row => row.dataset.id))
      list.replaceChildren(...cases.map(item => {
        const row = node('details')
        row.dataset.id = item.id
        row.open = opened.has(item.id)
        row.append(node('summary', item.title + ' · ' + stateText(item.status)))
        const related = cases.find(entry => entry.id === item.relatedCaseId)
        if (related) row.append(node('p', tr('关联事项 ', 'Related case ') + related.title, 'field-hint'))
        for (const [label, value] of [
          [tr('请求人', 'Requester'), item.request?.requester],
          [tr('办理对象', 'Subject'), item.request?.subject],
          [tr('项目', 'Project'), item.request?.project],
          [tr('用途', 'Purpose'), item.request?.purpose],
          [tr('待补信息', 'Missing'), item.request?.missing?.join(' · ')],
        ]) {
          if (value) row.append(node('p', label + ' · ' + value, 'field-hint'))
        }
        if (item.facts) row.append(node('p', item.facts))
        if (item.outcome) row.append(node('p', item.outcome))
        const running = ['waitingTarget', 'running'].includes(item.run?.status)
        if (item.createdMs && item.completedMs) {
          const elapsed = Math.max(0, (item.completedMs - item.createdMs) / 1000).toFixed(1)
          row.append(node('p', tr(`事项历时 ${elapsed} 秒`, `Case elapsed time ${elapsed} s`), 'field-hint'))
        }
        if (item.run) {
          row.append(node('p', stateText(item.run.status) + ' · ' + item.run.detail))
          if (item.run.startedMs && item.run.endedMs) {
            const seconds = Math.max(0, (item.run.endedMs - item.run.startedMs) / 1000)
            const parts = [tr(`助手执行 ${seconds.toFixed(1)} 秒`, `Assistant run ${seconds.toFixed(1)} s`)]
            if (item.manualBaselineSeconds && item.run.status === 'done') {
              const delta = item.manualBaselineSeconds - seconds
              parts.push(delta >= 0
                ? tr(`估计节省 ${delta.toFixed(1)} 秒`, `Estimated savings ${delta.toFixed(1)} s`)
                : tr(`比人工基准多用 ${(-delta).toFixed(1)} 秒`, `${(-delta).toFixed(1)} s over the manual baseline`))
            }
            row.append(node('p', parts.join(' · '), 'field-hint'))
          }
          if (running) row.append(action('停止办理', 'Stop task', () => post(base(item.id) + '/stop')))
          if (item.run.status === 'unverified' && item.plan && !data.executionInFlight
            && (!item.plan.reply || item.run.changed >= 2)) {
            row.append(action('核实回执', 'Verify receipt', () => {
              feedback.textContent = tr('请打开目标窗口中的回执，助手会读取它。', 'Open the receipt in the target window so the assistant can read it.')
              return post(base(item.id) + '/verify')
            }))
          }
        }
        if (!running && open.includes(item)) {
          addPlan(row, item, data)
          if (item.run?.status === 'unverified') {
            const result = node('input')
            result.placeholder = tr('填写人工核实结果', 'Enter the result you verified')
            result.setAttribute('aria-label', result.placeholder)
            const resolution = node('div', '', 'assistant-row')
            const resolve = status => post(base(item.id), { version: item.version, status, outcome: result.value })
            resolution.append(
              result,
              action('人工确认办完', 'Confirm completion', () => resolve('done')),
              action('记录结果并关闭', 'Record result and close', () => resolve('cancelled')),
            )
            row.append(resolution)
          } else {
            addEdit(row, item, open.filter(entry => entry.id !== item.id))
          }
        }
        if (item.status === 'done') {
          addDraft(row, item, data)
          const field = node('input')
          field.type = 'number'
          field.min = 0
          field.max = 86400
          field.placeholder = tr('人工办理基准（秒）', 'Manual baseline in seconds')
          field.setAttribute('aria-label', field.placeholder)
          field.value = item.manualBaselineSeconds ?? ''
          const editor = node('div', '', 'assistant-row')
          editor.append(field, action('保存人工基准', 'Save manual baseline', () => post(base(item.id), {
            version: item.version, manualBaselineSeconds: Number(field.value),
          })))
          row.append(editor)
        }
        if (!running) {
          const controls = node('div', '', 'assistant-row')
          if (open.includes(item)) {
            const muted = item.muteUntilMs > Date.now()
            controls.append(action(
              muted ? '恢复事项提醒' : '静音此事项 30 分钟',
              muted ? 'Resume case notifications' : 'Mute this case for 30 minutes',
              () => post(base(item.id), { version: item.version, muteForMinutes: muted ? 0 : 30 }),
            ))
          }
          if (item.run?.status !== 'unverified') controls.append(action('删除事项', 'Delete case', () => {
            const prompt = item.draft?.installed
              ? tr('删除事项及其预测和授权？已采纳的 Skill 文件仍可在工作区管理。', 'Delete this case, its predictions, and grants? The installed Skill stays in the workspace.')
              : tr('删除事项及其预测和授权？', 'Delete this case, its predictions, and grants?')
            if (window.confirm(prompt)) return request(base(item.id), { method: 'DELETE' })
          }))
          row.append(controls)
        }
        return row
      }))
      if (!cases.length) list.append(node('p', tr('助手分析到具体请求后，事项会出现在这里。', 'Cases appear here when the assistant identifies a request.'), 'field-hint'))
    }
    const allowed = (data.grants ?? []).filter(grant => grant.mode !== 'once')
    const grantKey = JSON.stringify([document.documentElement.lang, allowed])
    if (grantKey !== grantSignature) {
      grantSignature = grantKey
      grants.replaceChildren(...allowed.map(grant => {
        const row = node('div', '', 'assistant-row')
        row.append(node('span', grant.plan.title + ' · ' + (grant.expiresMs
          ? new Date(grant.expiresMs).toLocaleTimeString() : tr('长期授权', 'Standing authorization'))),
        action('撤销授权', 'Revoke', () => request('/api/assistant/grants/' + grant.id, { method: 'DELETE' })))
        return row
      }))
    }
  }

  function assignment(observation, data) {
    const open = (data.cases ?? []).filter(entry => ['working', 'waiting'].includes(entry.status))
    if (!open.length) return null
    const row = node('div', '', 'assistant-row')
    const select = nameSelect(open, tr('调整关联事项', 'Change associated case'))
    select.value = open.find(entry => entry.observations.includes(observation))?.id || ''
    const assign = action('重新关联', 'Attach', () => {
      const item = open.find(entry => entry.id === select.value)
      return post(base(item.id), { version: item.version, observation })
    })
    assign.disabled = !select.value
    select.addEventListener('change', () => { assign.disabled = !select.value })
    row.append(select, assign)
    return row
  }

  return { render, assignment }
}
