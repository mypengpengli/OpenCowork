// Protocol fixture: no external network or user configuration.
const readline = require('node:readline')
let initialized = false
let calls = 0
const reply = (id, result) => process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, result }) + '\n')
readline.createInterface({ input: process.stdin }).on('line', line => {
  const request = JSON.parse(line)
  if (request.method === 'initialize') {
    reply(request.id, { protocolVersion: '2025-06-18', capabilities: { tools: {} }, serverInfo: { name: 'fixture', version: '1' } })
  } else if (request.method === 'notifications/initialized') {
    initialized = true
  } else if (request.method === 'tools/list') {
    if (!initialized) process.exit(2)
    const names = request.params.cursor ? ['forbidden'] : ['echo', 'wait', 'browser_navigate', 'browser_snapshot', 'browser_click', 'browser_type', 'browser_take_screenshot']
    reply(request.id, { tools: names.map(name => ({ name, inputSchema: { type: 'object' } })), ...(request.params.cursor ? {} : { nextCursor: 'page2' }) })
  } else if (request.method === 'tools/call') {
    if (request.params.name === 'wait') return
    calls += 1
    if (request.params.name === 'browser_take_screenshot') {
      reply(request.id, { content: [{ type: 'image', mimeType: 'image/png', data: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aO1sAAAAASUVORK5CYII=' }] })
    } else {
      reply(request.id, { content: [{ type: 'text', text: JSON.stringify({ calls, ...request.params }) }] })
    }
  }
})
