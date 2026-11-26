import { initTasks } from '/assistant-tasks.mjs'

export function initAssistant({ request, t }) {
  const page = document.querySelector('#assistant-view')
  const lookup = name => page.querySelector(`#assistant-${name}`)
  const toggle = lookup('toggle')
  const interval = lookup('interval')
  const preview = lookup('preview')
  const previewButton = lookup('preview-toggle')
  const activity = lookup('observations')
  const form = lookup('decision-form')
  const status = lookup('status')
  const connection = lookup('decision-status')
  const lessons = lookup('lessons')
  const tasks = initTasks({ page, request, refresh: () => refresh() })
  let overview, timer
  let visible = false
  let changing = false
  let refreshing = false
  let previewOpen = false
  let previewAt = 0
  let rendered = ''
  let renderedLessons = ''
  const tr = (zh, en) => document.documentElement.lang.startsWith('en') ? en : zh
  const providerFields = ['textProvider', 'visionProvider', 'skillProvider', 'decisionProvider']

  async function loadProviderOptions(settings) {
    const profiles = await request('/api/provider-profiles')
    for (const key of providerFields) {
      const select = form.elements.namedItem(key)
      const current = settings ? settings[key] : select.value
      select.replaceChildren(new Option(key === 'decisionProvider'
        ? tr('选择已保存的 Provider', 'Choose a saved Provider')
        : tr('使用当前后台模型', 'Use current background model'), ''))
      for (const profile of profiles) {
        if (key !== 'decisionProvider' && (profile.id === 'default-provider' || profile.apiFormat === 'decisions')) continue
        select.append(new Option(`${profile.label} · ${profile.model}`, profile.id))
      }
      if (current && ![...select.options].some(option => option.value === current)) {
        select.append(new Option(tr('已移除的 Provider', 'Removed Provider'), current))
      }
      select.value = current ?? ''
    }
    updateDecisionChoice()
  }

  function updateDecisionChoice() {
    const enabled = form.elements.namedItem('useForAnalysis').checked
    const select = form.elements.namedItem('decisionProvider')
    select.disabled = !enabled
    select.required = enabled
  }

  function render(data) {
    overview = data
    tasks.render(data)
    toggle.disabled = changing || !data.available
    toggle.textContent = t(data.enabled ? 'assistant.pause' : 'assistant.start')
    toggle.dataset.i18n = data.enabled ? 'assistant.pause' : 'assistant.start'
    interval.disabled = changing || !data.available
    interval.value = String(data.intervalMs)
    lookup('phase').textContent = t(`assistant.phase.${data.phase}`)
    status.textContent = data.error || (!data.available ? t('assistant.unavailable')
      : data.language ? t('assistant.ocr', { language: data.language }) : '')
    lookup('analysis-status').textContent = data.analysisError || (data.analyzing ? t('assistant.analyzing')
      : data.decision.analyzeAutomatically ? t('assistant.budget', { remaining: data.autoRemaining }) : '')
    const m = data.metrics
    if (m) {
      const measure = n => n == null ? '—' : `${n} ms`
      lookup('metrics').textContent = [
        tr('近 24 小时保留样本', 'Retained samples over 24 hours'),
        tr(`${m.observations} 次观察、${m.predictions} 次分析、${m.images} 张图片`, `${m.observations} observations, ${m.predictions} analyses, ${m.images} images`),
        tr(`识别 P50 ${measure(m.captureP50Ms)}，分析 P50 ${measure(m.modelP50Ms)}，证据到预测 P95 ${measure(m.evidenceP95Ms)}`, `Capture P50 ${measure(m.captureP50Ms)}, analysis P50 ${measure(m.modelP50Ms)}, evidence to prediction P95 ${measure(m.evidenceP95Ms)}`),
        tr(`模型上报生成用量 ${m.generationTokens}，决策输入 ${m.decisionInputTokens} tokens`, `Reported generation usage ${m.generationTokens}, decision input ${m.decisionInputTokens} tokens`),
        tr(`反馈 ${m.correct} 条判断正确、${m.incorrect} 条判断错误、${m.unwanted} 条不必提醒`, `Feedback: ${m.correct} correct, ${m.incorrect} incorrect, ${m.unwanted} unwanted`),
      ].join(' · ')
    }
    previewButton.disabled = !data.hasPreview
    preview.hidden = !previewOpen || !data.hasPreview
    if (preview.hidden) preview.removeAttribute('src')
    if (!preview.hidden && Date.now() - previewAt >= 2500) {
      previewAt = Date.now()
      preview.src = `/api/assistant/preview?t=${previewAt}`
    }
    lookup('empty').hidden = data.observations.length > 0
    const lessonIds = data.lessons.map(item => item.id).join(',')
    lookup('lessons-empty').hidden = Boolean(lessonIds)
    if (renderedLessons !== lessonIds) {
      renderedLessons = lessonIds
      lessons.replaceChildren(...data.lessons.map(item => {
        const row = document.createElement('article')
        const body = document.createElement('p')
        body.textContent = [item.summary, item.lesson].filter(Boolean).join('\n')
        const remove = button('assistant.forget', async () => {
          await request(`/api/assistant/outcomes/${encodeURIComponent(item.id)}`, { method: 'DELETE' })
          await refresh()
        })
        row.append(body, remove)
        return row
      }))
    }
    const caseVersions = data.cases.map(item => [item.id, item.version])
    const ids = JSON.stringify([data.observations.map(item => item.id), data.predictions, lessonIds, data.imageObservation, caseVersions, data.decision.appRules])
    if (ids === rendered) return
    // Keep any user-entered outcome attached to its frame as observations update.
    const existing = new Map([...activity.children].map(row => [row.dataset.id, row]))
    const active = document.activeElement
    if (activity.contains(active) && ['TEXTAREA', 'INPUT', 'SELECT'].includes(active.tagName)) return
    rendered = ids
    activity.replaceChildren(...data.observations.map(item => {
      const row = document.createElement('details')
      row.dataset.id = item.id
      row.open = existing.get(item.id)?.open ?? false
      const heading = document.createElement('summary')
      const body = document.createElement('p')
      const event = {
        windowSwitch: tr('切换窗口', 'Window switch'),
        afterInput: tr('操作后变化', 'Changed after input'),
        assistant: tr('助手操作', 'Assistant action'),
      }[item.event]
      heading.textContent = [new Date(item.timeMs).toLocaleString(), item.app, item.window, event].filter(Boolean).join(' · ')
      body.textContent = item.text
      const actions = document.createElement('div')
      actions.className = 'assistant-row'
      const predict = async (image = false) => {
        lookup('analysis-status').textContent = t('assistant.analyzing')
        await request(`/api/assistant/observations/${encodeURIComponent(item.id)}/analyze?image=${image}`, { method: 'POST' })
        await refresh()
      }
      const appPolicy = data.decision.appRules?.[item.app?.toLowerCase()]
      const restricted = Object.values(data.decision.appRules ?? {}).some(mode => mode !== 'allow')
      if (['localOnly', 'ignore'].includes(appPolicy) || (restricted && (!item.app || item.windowText == null))) {
        const local = document.createElement('span')
        local.textContent = tr('仅在本地查看', 'Local view only')
        actions.append(local)
      } else {
        actions.append(button('assistant.analyze', predict))
      }
      if (data.imageObservation === item.id) {
        actions.append(button('assistant.analyzeImage', () => predict(true)))
      }
      const remove = button('assistant.forget', async () => {
        await request(`/api/assistant/observations/${encodeURIComponent(item.id)}`, { method: 'DELETE' })
        await refresh()
      })
      actions.append(remove)
      row.append(heading, body)
      const prediction = data.predictions.find(entry => entry.observationId === item.id)
      if (prediction) row.append(renderPrediction(prediction))
      const assignment = tasks.assignment(item.id, data)
      if (assignment) row.append(assignment)
      row.append(actions)
      const prior = data.lessons.find(lesson => lesson.observationId === item.id)
      row.append(existing.get(item.id)?.querySelector('.assistant-outcome') || outcomeForm(item.id, prior))
      return row
    }))
  }

  function button(key, run) {
    const element = document.createElement('button')
    element.type = 'button'
    element.className = 'ghost-button'
    element.dataset.i18n = key
    element.textContent = t(key)
    if (run) {
      element.addEventListener('click', async () => {
        element.disabled = true
        try {
          await run()
        } catch (error) {
          status.textContent = error.message
        } finally {
          element.disabled = false
        }
      })
    }
    return element
  }

  function renderPrediction(prediction) {
    const box = document.createElement('div')
    box.className = 'assistant-prediction'
    const title = document.createElement('strong')
    title.textContent = prediction.task
    const fields = document.createElement('dl')
    for (const [key, value] of [
      ['assistant.suggested', prediction.choices[prediction.choice]],
      ['assistant.basis', prediction.evidence],
      ['assistant.missing', prediction.missing.join(' · ')],
      ['assistant.options', Object.values(prediction.choices).join('\n')],
    ]) {
      if (!value) continue
      const label = document.createElement('dt')
      label.dataset.i18n = key
      label.textContent = t(key)
      const detail = document.createElement('dd')
      detail.textContent = value
      fields.append(label, detail)
    }
    const info = document.createElement('small')
    info.textContent = t(`assistant.selector.${prediction.selector}`)
    if (prediction.confidence != null) info.textContent += ` · ${Math.round(prediction.confidence * 100)}%`
    if (prediction.usedImage) info.textContent += ` · ${t('assistant.usedImage')}`
    box.append(title, fields, info)
    return box
  }

  function outcomeForm(id, prior) {
    const editor = document.createElement('form')
    editor.className = 'assistant-outcome'
    const [summary, lesson] = ['outcome', 'lesson'].map((key, index) => {
      const input = document.createElement('textarea')
      input.maxLength = index === 0 ? 400 : 1200
      input.setAttribute('aria-label', t(`assistant.${key}`))
      input.placeholder = t(`assistant.${key}`)
      return input
    })
    summary.required = true
    summary.value = prior?.summary || ''
    lesson.value = prior?.lesson || ''
    const feedback = document.createElement('select')
    feedback.setAttribute('aria-label', tr('对这次预测的反馈', 'Prediction feedback'))
    for (const [value, label] of [
      ['unrated', tr('评价这次预测', 'Rate this prediction')],
      ['correct', tr('判断正确', 'Correct')],
      ['incorrect', tr('判断有误', 'Incorrect')],
      ['exception', tr('这次是例外', 'This was an exception')],
      ['unwanted', tr('这次不必提醒', 'Unwanted reminder')],
    ]) feedback.add(new Option(label, value))
    feedback.value = prior?.feedback ?? 'unrated'
    const submit = button('assistant.addOutcome')
    submit.type = 'submit'
    editor.append(summary, lesson, feedback, submit)
    editor.addEventListener('submit', async event => {
      event.preventDefault()
      submit.disabled = true
      try {
        const data = await request(`/api/assistant/observations/${encodeURIComponent(id)}/outcome`, {
          method: 'POST', body: JSON.stringify({ summary: summary.value, lesson: lesson.value, feedback: feedback.value }),
        })
        render(data)
      } catch (error) {
        status.textContent = error.message
      } finally {
        submit.disabled = false
      }
    })
    return editor
  }

  async function refresh() {
    if (refreshing || changing || document.hidden) return
    refreshing = true
    try {
      const data = await request('/api/assistant')
      if (changing) return
      if (!overview) {
        await loadProviderOptions(data.decision)
        for (const [key, value] of Object.entries(data.decision)) {
          const field = form.elements.namedItem(key)
          if (!field || providerFields.includes(key)) continue
          if (field.type === 'checkbox') field.checked = value
          else if (key === 'appRules') field.value = Object.entries(value).map(([app, mode]) => `${app}=${mode}`).join('\n')
          else field.value = value
        }
        updateDecisionChoice()
      }
      render(data)
    } catch (error) {
      status.textContent = error.message
    } finally {
      refreshing = false
    }
  }

  async function update(body) {
    changing = true
    toggle.disabled = interval.disabled = true
    try {
      const data = await request('/api/assistant/observation', { method: 'POST', body: JSON.stringify(body) })
      changing = false
      render(data)
    } catch (error) {
      changing = false
      if (overview) render(overview)
      status.textContent = error.message
    }
  }

  toggle.addEventListener('click', () => update({ enabled: !overview.enabled }))
  interval.addEventListener('change', () => update({ intervalMs: Number(interval.value) }))
  previewButton.addEventListener('click', () => {
    previewOpen = !previewOpen
    previewAt = 0
    previewButton.setAttribute('aria-expanded', String(previewOpen))
    if (overview) render(overview)
  })

  async function configure(test) {
    if (!form.reportValidity()) return
    const body = Object.fromEntries([...new FormData(form)].map(([key, value]) => [key, value.trim()]))
    body.decisionProvider = form.elements.namedItem('decisionProvider').value
    const appRules = Object.create(null)
    for (const line of body.appRules.split('\n').map(row => row.trim()).filter(Boolean)) {
      const [name, policy, ...extra] = line.split('=').map(value => value.trim())
      if (extra.length || !name.toLowerCase().endsWith('.exe') || !['allow', 'textOnly', 'localOnly', 'ignore'].includes(policy) || name.toLowerCase() in appRules) {
        connection.textContent = tr('请检查应用名称、规则类型和重复的程序名。', 'Check executable names, policy values, and duplicates.')
        return
      }
      appRules[name.toLowerCase()] = policy
    }
    body.appRules = appRules
    body.dailyLimit = Number(body.dailyLimit)
    for (const name of ['useForAnalysis', 'analyzeAutomatically', 'useImages', 'showNotifications', 'autoRunReady', 'autoRunReplies']) {
      body[name] = form.elements.namedItem(name).checked
    }
    const buttons = [...form.querySelectorAll('button')]
    buttons.forEach(button => { button.disabled = true })
    connection.textContent = test ? t('assistant.testing') : ''
    try {
      await request('/api/assistant/decision', { method: 'POST', body: JSON.stringify(body) })
      if (test) {
        const result = await request('/api/assistant/decision/test', { method: 'POST' })
        connection.textContent = t('assistant.testResult', {
          choice: result.choice, confidence: `${Math.round(result.confidence * 100)}%`, latency: result.latencyMs,
        })
      } else {
        connection.textContent = t('assistant.saved')
      }
    } catch (error) {
      connection.textContent = error.message
    } finally {
      buttons.forEach(button => { button.disabled = false })
    }
  }

  form.addEventListener('submit', event => { event.preventDefault(); void configure(false) })
  form.elements.namedItem('useForAnalysis').addEventListener('change', updateDecisionChoice)
  lookup('decision-test').addEventListener('click', () => void configure(true))
  document.addEventListener('visibilitychange', () => {
    if (visible && !document.hidden) void refresh()
  })

  return {
    show(show) {
      page.classList.toggle('is-hidden', !show)
      if (visible === show) return
      visible = show
      clearInterval(timer)
      if (show) {
        if (overview) void loadProviderOptions().catch(error => { connection.textContent = error.message })
        void refresh()
        timer = setInterval(refresh, 1600)
      } else {
        preview.removeAttribute('src')
        previewAt = 0
      }
    },
  }
}
