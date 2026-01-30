import DOMPurify from 'dompurify'
import { marked } from 'marked'

marked.setOptions({
  gfm: true,
  breaks: true,
  mangle: false,
  headerIds: false,
})

const allowedTags = [
  'a',
  'b',
  'blockquote',
  'br',
  'code',
  'del',
  'em',
  'h1',
  'h2',
  'h3',
  'h4',
  'hr',
  'i',
  'li',
  'ol',
  'p',
  'pre',
  'strong',
  'table',
  'tbody',
  'td',
  'th',
  'thead',
  'tr',
  'ul',
]

const allowedAttrs = ['class', 'href', 'rel', 'target', 'title', 'start', 'colspan', 'rowspan']

const MARKDOWN_CHAR_LIMIT = 140000
const MARKDOWN_PARSE_LIMIT = 40000
let hooksInstalled = false

function installHooks() {
  if (hooksInstalled) return
  hooksInstalled = true

  DOMPurify.addHook('afterSanitizeAttributes', (node) => {
    if (!(node instanceof HTMLAnchorElement)) return
    const href = node.getAttribute('href')
    if (!href) return
    node.setAttribute('rel', 'noreferrer noopener')
    node.setAttribute('target', '_blank')
  })
}

export function renderMarkdown(markdown: string): string {
  const input = markdown.trim()
  if (!input) return ''

  installHooks()

  let text = input
  let suffix = ''
  if (input.length > MARKDOWN_CHAR_LIMIT) {
    text = input.slice(0, MARKDOWN_CHAR_LIMIT)
    suffix = `\n\n... truncated (${input.length} chars).`
  }

  if (text.length > MARKDOWN_PARSE_LIMIT) {
    const html = `<pre class="code-block">${escapeHtml(`${text}${suffix}`)}</pre>`
    return DOMPurify.sanitize(html, {
      ALLOWED_TAGS: allowedTags,
      ALLOWED_ATTR: allowedAttrs,
    })
  }

  const rendered = marked.parse(`${text}${suffix}`) as string
  return DOMPurify.sanitize(rendered, {
    ALLOWED_TAGS: allowedTags,
    ALLOWED_ATTR: allowedAttrs,
  })
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}
