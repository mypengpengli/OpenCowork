import assert from 'node:assert/strict'
import { linkTarget, highlightTokens, renderMarkdown } from '../crates/opencowork-shell/static/markdown.mjs'
for (const target of ['javascript:alert(1)', 'javascript:1', 'data:text/html,<script>', '//evil.test', 'https://good.test%0aevil', 'vbscript:x', '%6aavascript:alert(1)']) assert.equal(linkTarget(target), null)
assert.deepEqual(linkTarget('D:\\project\\file.rs:42:3'), { kind: 'file', path: 'D:\\project\\file.rs', line: 42 })
assert.deepEqual(linkTarget('src/main.rs#L12'), { kind: 'file', path: 'src/main.rs', line: 12 })
assert.deepEqual(linkTarget('example.txt:2'), { kind: 'file', path: 'example.txt', line: 2 })
const source = 'const text = "<script>alert(1)</script>"; // untouched\nreturn 42;'
assert.equal(highlightTokens(source).map(t => t.text).join(''), source)
class Node {
  constructor(tag, text = '') { this.tagName = tag; this.children = []; this.textContent = text }
  append(...children) { this.children.push(...children) }
}
globalThis.document = { createElement: tag => new Node(tag), createTextNode: text => new Node('#text', text) }
const markdown = '# Heading\n\n**Bold** and [File](src/main.rs:12), [unsafe](javascript:alert), [Web](https://example.com)\n\n| Key | Value |\n| --- | --- |\n| a | b |\n\n- [x] Done\n- Pending\n\n```js\n' + source + '\n```\n\n<script>alert(1)</script>\n<img src=x onerror=alert(1)>'
let opened
const tree = renderMarkdown(markdown, { onFile: (path, line) => { opened = [path, line] } })
const flatten = n => [n, ...n.children.flatMap(flatten)]
const nodes = flatten(tree)
for (const tag of ['h1', 'strong', 'table', 'input', 'pre', 'code']) assert.ok(nodes.some(n => n.tagName === tag), tag)
assert.ok(!nodes.some(n => ['script', 'img', 'iframe'].includes(n.tagName)))
assert.equal(nodes.filter(n => n.tagName === 'a').length, 1)
await nodes.find(n => n.className === 'markdown-file-link').onclick()
assert.deepEqual(opened, ['src/main.rs', 12])
assert.doesNotThrow(() => renderMarkdown('>'.repeat(20000) + ' nested text'))
console.log('PASS: Markdown formatting, inert HTML, URL filtering, file locations, token preservation and nesting bounds')
