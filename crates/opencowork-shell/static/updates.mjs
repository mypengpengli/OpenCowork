import { createUi } from '/ui-controls.mjs'

export function initUpdates({ state }) {
  const ui = createUi(state, () => {}), { tr, el, button } = ui
  const card = el('article', '', 'settings-card'); card.id = 'app-updates-card'
  card.append(el('h3', tr('软件更新', 'App updates')))
  const current = el('p', '', 'settings-card-copy'), status = el('p', '', 'settings-card-copy'); status.setAttribute('role', 'status')
  card.append(current, status)
  document.querySelector('#settings-environment-panel').append(card)
  if (!window.__OPENCOWORK_DESKTOP__ || !window.ipc?.postMessage) {
    current.textContent = tr('自动更新适用于安装版 Windows 桌面客户端。源码版通过 Git 和重新构建更新。', 'Automatic updates are available in the installed Windows desktop app. Update source builds with Git and rebuild.')
    return
  }
  const send = (action, extra = {}) => window.ipc.postMessage(JSON.stringify({ action, ...extra }))
  const auto = el('input'); auto.type = 'checkbox'; auto.id = 'auto-update-enabled'
  const label = el('label', '', 'feature-toggle'); label.append(auto, el('span', tr('自动检查并下载新版', 'Automatically check for and download updates')))
  const check = button(tr('检查更新', 'Check for updates'), () => send('updates:check'))
  const install = button(tr('退出并安装更新', 'Quit and install update'), () => send('updates:install'))
  const row = el('div', '', 'schedule-actions'); row.append(check, install)
  card.append(label, el('p', tr('启动后检查，此后每 6 小时检查一次。下载完成后需确认退出再安装，会保留设置、会话和工作区。', 'Checks on startup and every six hours. Confirm before quitting to install; settings, sessions and workspaces are preserved.'), 'settings-card-copy'), row)
  const banner = el('aside', '', 'app-update-banner'); banner.hidden = true
  const bannerText = el('span'), bannerButton = button(tr('退出并更新', 'Quit and update'), () => send('updates:install'))
  banner.append(bannerText, bannerButton, button(tr('稍后', 'Later'), () => { banner.hidden = true; dismissed = last?.latestVersion }))
  document.body.append(banner)
  let last, dismissed = null
  function render(value) {
    last = value; auto.checked = value.autoEnabled; auto.disabled = !value.canInstall
    current.textContent = `${tr('当前版本', 'Current version')}: ${value.currentVersion}${value.latestVersion ? ` · ${tr('最新版本', 'Latest version')}: ${value.latestVersion}` : ''}`
    const phases = { idle: tr('等待检查', 'Waiting to check'), checking: tr('正在检查更新…', 'Checking for updates…'), downloading: tr('发现新版，正在后台下载并校验…', 'Downloading and verifying the new release…'), ready: tr('新版已下载，可以退出并安装。', 'Update downloaded. Ready to quit and install.'), current: tr('已是最新正式版。', 'You have the latest stable release.'), available: tr('发现新版，源码运行版本请通过 Git 更新。', 'A new release is available. Update this source build with Git.'), error: tr('更新检查或下载失败，可稍后重试：', 'Update check or download failed. Retry later: ') + (value.error || '') }
    status.textContent = phases[value.phase] || value.phase
    if (!value.canInstall && value.phase === 'idle') status.textContent = tr('源码运行版本，请通过 Git 和重新构建更新。', 'Source build: update with Git and rebuild.')
    check.disabled = ['checking', 'downloading'].includes(value.phase)
    install.hidden = value.phase !== 'ready'; install.disabled = Boolean(state.sending)
    bannerButton.disabled = Boolean(state.sending)
    bannerText.textContent = tr(`OpenCowork ${value.latestVersion || ''} 已准备好`, `OpenCowork ${value.latestVersion || ''} is ready`)
    banner.hidden = value.phase !== 'ready' || dismissed === value.latestVersion
  }
  auto.addEventListener('change', () => send('updates:auto', { enabled: auto.checked }))
  window.addEventListener('opencowork:update', event => render(event.detail))
  window.addEventListener('opencowork:composer-state', () => { if (last) render(last) })
  window.addEventListener('opencowork:locale-changed', () => { ui.localize(card); ui.localize(banner); if (last) render(last) })
  install.hidden = true; send('updates:status')
}
