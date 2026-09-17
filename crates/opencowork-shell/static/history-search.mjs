export function initHistorySearch({ state, request, container, count, openMatch }) {
  let key = '', version = 0, timer, data = null, error = '', loading = false
  const text = (zh, en) => state.locale === 'zh' ? zh : en
  const node = (tag, content, cls = '') => { const n = document.createElement(tag); n.textContent = content; n.className = cls; return n }
  function paint(query) {
    container.replaceChildren()
    count.textContent = loading ? text('搜索中…', 'Searching…') : `${data?.results.length || 0} ${text('条匹配', 'matches')}`
    if (loading) { container.append(node('p', text('正在搜索消息与归档…', 'Searching messages and archives…'), 'history-empty')); return }
    if (error) {
      const retry = node('button', text('重试搜索', 'Retry search'), 'ghost-button'); retry.type = 'button'
      retry.onclick = () => { key = ''; render(query) }
      container.append(node('p', error, 'history-empty'), retry); return
    }
    for (const result of data?.results || []) {
      const session = state.bootstrap?.sessions?.find(s => s.id === result.sessionId)
      const card = node('article', '', 'history-card history-search-result'), open = node('button', session?.title || result.sessionId, 'history-open-title'); open.type = 'button'
      open.onclick = () => Promise.resolve(openMatch({ ...result, query })).catch(e => { const feedback = node('p', e.message); feedback.setAttribute('role', 'alert'); card.append(feedback) })
      const label = `${result.archived ? text('归档消息', 'Archived message') : text('消息', 'Message')} ${result.messageIndex + 1}`
      card.append(open, node('small', `${result.workspace || text('未归属项目', 'Unassigned')} · ${label}`), node('p', result.snippet))
      container.append(card)
    }
    if (!data?.results.length) container.append(node('p', text('正文和归档中没有匹配结果。', 'No matching messages or archives.'), 'history-empty'))
    else if (data.results.length >= data.limit) container.append(node('p', text('显示前 100 条，请缩小搜索范围。', 'Showing the first 100 results. Refine your search.')))
  }
  function render(query) {
    const signature = JSON.stringify((state.bootstrap?.sessions || []).map(s => [s.id, s.updatedAtUnixMs]))
    const next = `${query}|${signature}`
    if (next !== key) {
      key = next; clearTimeout(timer); const token = ++version; loading = true; data = null; error = ''
      timer = setTimeout(async () => {
        try { const result = await request(`/api/history-search?q=${encodeURIComponent(query)}&all=true`); if (version !== token) return; data = result }
        catch (e) { if (version !== token) return; error = e.message }
        if (version !== token) return
        loading = false; paint(query)
      }, 250)
    }
    paint(query)
  }
  return { render, clear() { clearTimeout(timer); ++version; key = ''; data = null } }
}
