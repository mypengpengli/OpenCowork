export async function readChatStream(response, onMessage) {
  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(error.error || `Request failed: ${response.status}`)
  }
  if (!response.body) throw new Error('Streaming response is unavailable.')
  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''
  let completed = null
  function consume(line) {
    if (!line.trim()) return
    const message = JSON.parse(line)
    if (message.type === 'error') throw new Error(message.message)
    if (message.type === 'complete') completed = message.response
    onMessage(message)
  }
  try {
    while (true) {
      const { value, done } = await reader.read()
      buffer += done ? decoder.decode() : decoder.decode(value, { stream: true })
      let newline
      while ((newline = buffer.indexOf('\n')) !== -1) {
        consume(buffer.slice(0, newline))
        buffer = buffer.slice(newline + 1)
      }
      if (done) break
    }
    if (buffer.trim()) consume(buffer)
    if (!completed) throw new Error('Connection ended before the turn was saved. Refresh the session to recover received output.')
    return completed
  } finally {
    await reader.cancel().catch(() => {})
    reader.releaseLock()
  }
}

export function appendChatEvent(messages, event) {
  function assistant() {
    let message = messages.at(-1)
    if (!message || message.role !== 'assistant' || message.__finished) {
      message = { role: 'assistant', blocks: [], __pending: true, __pendingState: 'streaming' }
      messages.push(message)
    }
    return message
  }
  switch (event.type) {
    case 'assistant_text_delta': {
      const message = assistant()
      const last = message.blocks.at(-1)
      if (last?.type === 'text') last.text += event.text
      else message.blocks.push({ type: 'text', text: event.text })
      break
    }
    case 'tool_call':
      assistant().blocks.push({ type: 'tool_use', id: event.id, name: event.name, input: event.input })
      break
    case 'tool_result':
      messages.push({ role: 'tool', blocks: [{ ...event, type: 'tool_result' }] })
      break
    case 'usage':
      assistant().usage = event.usage
      break
    case 'message_stop':
      if (messages.at(-1)?.role === 'assistant') messages.at(-1).__finished = true
      break
  }
}
