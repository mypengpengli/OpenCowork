// DOM-only Markdown subset: no HTML execution, remote image loads or unsafe URLs.
export function linkTarget(raw) {
  let value = String(raw).trim().replace(/^<|>$/g, '')
  try { value = decodeURIComponent(value) } catch { return null }
  if (/[\u0000-\u001f\u007f]/.test(value) || value.startsWith('//')) return null
  if (/^(?:javascript|vbscript|data|file|blob):/i.test(value)) return null
  if (/^https?:\/\//i.test(value)) return { kind: 'web', href: value }
  const match = value.match(/^(.*?)(?::(\d+)(?::\d+)?|#L(\d+))?$/)
  const path = match[1]
  if (/^[a-z][a-z\d+.-]*:/i.test(path) && !/^[a-z]:[\\/]/i.test(path)) return null
  if (!path || path.startsWith('#')) return null
  return { kind: 'file', path, line: Number(match[2] || match[3] || 1) }
}

export function highlightTokens(source) {
  const pattern = /("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)|(\/\/[^\n]*|#[^\n]*|\/\*[\s\S]*?\*\/)|\b(fn|let|mut|const|function|return|if|else|for|while|class|def|import|from|export|async|await|pub|use|struct|enum|impl|match|true|false|null|None|True|False|try|catch|throw|new)\b|\b(\d+(?:\.\d+)?)\b/g
  const tokens = []; let end = 0
  for (const match of String(source).matchAll(pattern)) {
    if (match.index > end) tokens.push({ text: source.slice(end, match.index), kind: '' })
    tokens.push({ text: match[0], kind: match[1] ? 'string' : match[2] ? 'comment' : match[3] ? 'keyword' : 'number' })
    end = match.index + match[0].length
  }
  tokens.push({ text: source.slice(end), kind: '' }); return tokens
}

export function renderMarkdown(source, { onFile, onError = () => {}, locale = 'zh', depth = 0 } = {}) {
  const node = (tag, text = '', className = '') => { const n = document.createElement(tag); n.textContent = text; n.className = className; return n }
  function inline(parent, text, depth = 0) {
    if (depth > 8) { parent.append(document.createTextNode(text)); return }
    const pattern = /(`+)([^`\n]+)\1|!?\[([^\]\n]+)\]\((<[^>\n]+>|[^)\n]+)\)|\*\*([^*\n]+)\*\*|__([^_\n]+)__|~~([^~\n]+)~~|\*([^*\n]+)\*/g
    let end = 0
    for (const m of text.matchAll(pattern)) {
      parent.append(document.createTextNode(text.slice(end, m.index)))
      if (m[1]) parent.append(node('code', m[2]))
      else if (m[3]) {
        const target = linkTarget(m[4])
        if (!target) parent.append(document.createTextNode(m[3]))
        else if (target.kind === 'web') { const a = node('a', m[3]); a.href = target.href; a.target = '_blank'; a.rel = 'noopener noreferrer'; parent.append(a) }
        else { const b = node('button', m[3], 'markdown-file-link'); b.type = 'button'; b.title = `${target.path}:${target.line}`; b.onclick = () => Promise.resolve(onFile?.(target.path, target.line)).catch(onError); parent.append(b) }
      } else { const child = node(m[5] || m[6] ? 'strong' : m[7] ? 'del' : 'em'); inline(child, m[5] || m[6] || m[7] || m[8], depth + 1); parent.append(child) }
      end = m.index + m[0].length
    }
    parent.append(document.createTextNode(text.slice(end)))
  }
  if (depth > 12) return node('pre', String(source), 'markdown-body')
  const root = node('div', '', 'markdown-body'), lines = String(source).replace(/\r\n/g, '\n').split('\n')
  const tableCells = line => line.trim().replace(/^\||\|$/g, '').split(/(?<!\\)\|/).map(s => s.trim().replace(/\\\|/g, '|'))
  for (let i = 0; i < lines.length;) {
    const line = lines[i]
    if (!line.trim()) { i++; continue }
    const fence = line.match(/^\s*(`{3,}|~{3,})([\w+.-]*)[^\n]*$/)
    if (fence) {
      const content = []; i++
      while (i < lines.length && !new RegExp(`^\\s*${fence[1][0]}{${fence[1].length},}\\s*$`).test(lines[i])) content.push(lines[i++])
      if (i < lines.length) i++
      const raw = content.join('\n'), section = node('section', '', 'markdown-code'), header = node('div', '', 'markdown-code-header')
      header.append(node('span', fence[2] || (locale === 'zh' ? '代码' : 'Code')))
      const copy = node('button', locale === 'zh' ? '复制代码' : 'Copy code'); copy.type = 'button'
      copy.onclick = async () => { try { await navigator.clipboard.writeText(raw); copy.textContent = locale === 'zh' ? '已复制' : 'Copied' } catch (e) { onError(e) } }
      header.append(copy); const pre = node('pre'), code = node('code'); pre.tabIndex = 0
      for (const token of highlightTokens(raw)) code.append(token.kind ? node('span', token.text, `syntax-${token.kind}`) : document.createTextNode(token.text))
      pre.append(code); section.append(header, pre); root.append(section); continue
    }
    const heading = line.match(/^(#{1,6})\s+(.+)$/)
    if (heading) { const h = node(`h${heading[1].length}`); inline(h, heading[2]); root.append(h); i++; continue }
    if (/^\s*(?:---+|\*\*\*+|___+)\s*$/.test(line)) { root.append(node('hr')); i++; continue }
    if (i + 1 < lines.length && line.includes('|') && /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*$/.test(lines[i + 1])) {
      const wrap = node('div', '', 'markdown-table'), table = node('table'), head = node('thead'), row = node('tr')
      for (const cell of tableCells(line)) { const th = node('th'); inline(th, cell); row.append(th) }
      head.append(row); table.append(head); const body = node('tbody'); i += 2
      while (i < lines.length && lines[i].trim() && lines[i].includes('|')) { const tr = node('tr'); for (const cell of tableCells(lines[i++])) { const td = node('td'); inline(td, cell); tr.append(td) } body.append(tr) }
      table.append(body); wrap.append(table); root.append(wrap); continue
    }
    if (/^\s*>/.test(line)) {
      const quote = node('blockquote'), content = []
      while (i < lines.length && /^\s*>/.test(lines[i])) content.push(lines[i++].replace(/^\s*>\s?/, ''))
      quote.append(renderMarkdown(content.join('\n'), { onFile, onError, locale, depth: depth + 1 })); root.append(quote); continue
    }
    const list = line.match(/^\s*(?:([-+*])|(\d+)[.)])\s+(.+)$/)
    if (list) {
      const ul = node(list[2] ? 'ol' : 'ul'); if (list[2]) ul.start = Number(list[2])
      while (i < lines.length) {
        const item = lines[i].match(/^\s*(?:([-+*])|(\d+)[.)])\s+(.+)$/)
        if (!item || Boolean(item[2]) !== Boolean(list[2])) break
        const li = node('li'), task = item[3].match(/^\[([ xX])\]\s+(.*)$/)
        if (task) { const box = node('input'); box.type = 'checkbox'; box.checked = task[1] !== ' '; box.disabled = true; li.append(box); inline(li, task[2]) } else inline(li, item[3])
        ul.append(li); i++
      }
      root.append(ul); continue
    }
    const paragraph = node('p'), content = [line]; i++
    while (i < lines.length && lines[i].trim() && !/^\s*(?:#{1,6}\s|>|`{3}|~{3}|[-+*]\s|\d+[.)]\s)/.test(lines[i]) && !(lines[i].includes('|') && /^\s*\|?\s*:?-{3}/.test(lines[i + 1] || ''))) content.push(lines[i++])
    inline(paragraph, content.join('\n')); root.append(paragraph)
  }
  return root
}
