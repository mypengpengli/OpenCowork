import assert from 'node:assert/strict'
import { appendDraft, createUi, processMessageIndexes } from '../crates/opencowork-shell/static/ui-controls.mjs'

const draft = '保留这段草稿\nconst example = "<script>";'
assert.equal(appendDraft(draft, 'Plan first', true), `Plan first\n\n${draft}`)
assert.equal(appendDraft(draft, 'Continue'), `${draft}\n\nContinue`)
assert.equal(appendDraft('', 'Plan first'), 'Plan first')
assert.equal(appendDraft(appendDraft(draft, 'Plan first', true), 'Plan first', true), `Plan first\n\n${draft}`)

const text = (role, text) => ({ role, blocks: [{ type: 'text', text }] })
const history = [text('user', 'Task one'), text('assistant', 'I will inspect the files'),
  { role: 'assistant', blocks: [{ type: 'tool_use', name: 'Read' }] },
  { role: 'tool', blocks: [{ type: 'tool_result', output: 'contents' }] },
  text('assistant', 'Complete answer\n'.repeat(60)), text('user', 'Task two'),
  text('assistant', 'Another complete answer\n'.repeat(60))]
assert.deepEqual([...processMessageIndexes(history)].sort(), [1, 2, 3])
assert.equal(processMessageIndexes([text('assistant', 'Stopped partial output')]).size, 0)

// Exercise actual async button behavior: one in-flight action, local error, retry.
const feedback = { textContent: '', dataset: {} }
const card = { querySelector: () => feedback }
globalThis.document = { createElement: () => ({
  dataset: {}, attrs: {}, textContent: '', disabled: false,
  setAttribute(k, v) { this.attrs[k] = v }, getAttribute(k) { return this.attrs[k] }, removeAttribute(k) { delete this.attrs[k] },
  closest: () => card, dispatchEvent() {},
}) }
globalThis.CustomEvent ||= class { constructor(type) { this.type = type } }
const state = { locale: 'zh' }
const ui = createUi(state, () => assert.fail('Settings errors must stay in their card'))
let calls = 0, finish
const button = ui.button('保存执行设置', () => { calls++; return new Promise((_, reject) => { finish = reject }) })
const action = button.onclick()
assert.equal(button.disabled, true)
await button.onclick()
assert.equal(calls, 1)
finish(new Error('fixture failure')); await action
assert.equal(feedback.textContent, 'fixture failure')
assert.equal(feedback.dataset.tone, 'error')
assert.equal(button.disabled, false)
const retry = button.onclick(); assert.equal(calls, 2); finish(new Error('retry failure')); await retry
const input = document.createElement('input'); input.value = draft
ui.bind(input, '后台模型名称（留空跟随当前模型）', 'placeholder')
state.locale = 'en'; ui.localize({ querySelectorAll: () => [button, input] })
assert.equal(button.textContent, 'Save execution settings')
assert.equal(input.attrs.placeholder, 'Background model (blank uses current model)')
assert.equal(input.value, draft)
console.log('PASS: draft preservation, duplicate action suppression, local errors/retry and localization without form replacement')
