import { createUi } from '/ui-controls.mjs'
export function initSetup({ state, request, status, openSettings }) {
  const { el, tr, button } = createUi(state, status)
  const card = el('article', '', 'settings-card setup-card'), summary = el('p'), results = el('pre', '', 'feature-file-preview')
  const hint = el('div', '', 'setup-hint'), hintText = el('span')
  hint.hidden = true
  hint.append(hintText, button(tr('配置模型', 'Configure model'), () => openSettings('provider')))
  document.querySelector('#chat-empty-state').append(hint)
  let data, version = 0, validated = false
  const configure = button(tr('配置模型', 'Configure model'), () => document.querySelector('#new-provider-profile-button').click())
  const probe = button(tr('检测模型连接、工具、图像和流式能力', 'Check connection, tools, vision and streaming'), async () => {
    results.textContent = tr('正在检测，最多 4 次小请求，可能产生用量…', 'Checking, up to 4 small requests; usage may be charged…')
    try {
      const value = await request('/api/provider-probe', { method: 'POST', body: '{}' })
      validated = value.checks.every(c => c.status === 'verified')
      results.textContent = `${value.model} · ${value.protocol}\n` + value.checks.map(c => `${c.kind}: ${c.status} · ${c.elapsedMs} ms · ${c.tokens ?? '?'} tokens ${c.error || ''}`).join('\n')
      paint()
    } catch (e) { validated = false; results.textContent = e.message; throw e }
  })
  function paint() {
    if (!data) return
    summary.textContent = `${tr('工作区', 'Workspace')}: ${data.workspace}\n${tr('模型', 'Model')}: ${data.provider} / ${data.model}\n${data.credentialPresent ? tr('已找到凭据', 'Credential found') : tr('未找到凭据，请配置 API Key 或环境变量（无密钥的本地服务可直接检测）', 'No credential found. Configure an API key or environment variable; keyless local services can be checked directly.')} · ${validated ? tr('本次检测通过', 'Verified in this check') : tr('连接尚未验证', 'Connection not yet verified')}\nMCP: ${data.mcpCount} ${tr('个已配置；可选，不影响内置工具', 'configured; optional for built-in tools')}`
    hint.hidden = data.credentialPresent || validated
    hintText.textContent = tr('先配置模型服务，即可开始任务。', 'Configure a model provider to start a task.')
  }
  async function refresh() {
    const current = ++version
    try { const next = await request('/api/setup-status'); if (current !== version) return; if (data && JSON.stringify(data) !== JSON.stringify(next)) { validated = false; results.textContent = '' } data = next; paint() }
    catch (e) { if (current === version) summary.textContent = e.message }
  }
  const actions = el('div', '', 'feature-toolbar')
  actions.append(configure, probe, button(tr('配置 MCP（可选）', 'Configure MCP (optional)'), () => openSettings('mcp')), button(tr('查看技能（可选）', 'Browse skills (optional)'), () => openSettings('skills')), button(tr('刷新配置状态', 'Refresh setup status'), refresh))
  card.append(el('h3', tr('开始使用', 'Getting started')), summary, actions, results)
  document.querySelector('#settings-provider-panel').prepend(card)
  window.addEventListener('opencowork:bootstrap-loaded', () => { validated = false; results.textContent = ''; refresh() })
  refresh()
  window.addEventListener('opencowork:locale-changed', paint)
  return { refresh }
}
