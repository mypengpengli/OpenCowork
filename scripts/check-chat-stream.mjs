import assert from 'node:assert/strict'
import { readChatStream, appendChatEvent } from '../crates/opencowork-shell/static/chat-stream.mjs'

const encoder = new TextEncoder()
const messages = [
  { type: 'started', turn_id: 'demo', session_id: 'session-demo' },
  { type: 'event', event: { type: 'assistant_text_delta', text: '你好🌏\n第二行' } },
  { type: 'complete', response: { status: 'completed', sessionId: 'session-demo' } },
]
const wire = encoder.encode(messages.map(JSON.stringify).join('\r\n'))
const received = []
const response = new Response(new ReadableStream({
  start(controller) {
    // Split UTF-8 characters, JSON tokens, and CRLF boundaries.
    for (const byte of wire) controller.enqueue(new Uint8Array([byte]))
    controller.close()
  },
}))
assert.equal((await readChatStream(response, (message) => received.push(message))).status, 'completed')
assert.deepEqual(received, messages)
await assert.rejects(readChatStream(new Response('{"type":"started"}\n'), () => {}), /before the turn was saved/)
await assert.rejects(readChatStream(new Response('{"type":"error","message":"failed"}\n'), () => {}), /failed/)

let release
let firstSeen
const seen = new Promise((resolve) => { firstSeen = resolve })
const incremental = new Response(new ReadableStream({
  start(controller) {
    controller.enqueue(encoder.encode(JSON.stringify(messages[1]) + '\n'))
    release = () => {
      controller.enqueue(encoder.encode(JSON.stringify(messages[2]) + '\n'))
      controller.close()
    }
  },
}))
const reading = readChatStream(incremental, () => firstSeen())
await seen
release()
await reading

const projected = []
appendChatEvent(projected, { type: 'assistant_text_delta', text: '检查' })
appendChatEvent(projected, { type: 'assistant_text_delta', text: '文件' })
appendChatEvent(projected, { type: 'tool_call', id: '1', name: 'read_file', input: '{}' })
appendChatEvent(projected, { type: 'message_stop' })
appendChatEvent(projected, { type: 'tool_result', tool_use_id: '1', tool_name: 'read_file', output: 'ok', is_error: false })
appendChatEvent(projected, { type: 'assistant_text_delta', text: '完成' })
assert.equal(projected.length, 3)
assert.equal(projected[0].blocks[0].text, '检查文件')
assert.equal(projected[0].blocks[1].id, '1')
assert.equal(projected[1].role, 'tool')
assert.equal(projected[2].blocks[0].text, '完成')
console.log('PASS: incremental events, split UTF-8, incomplete streams, errors, and tool/message order')
