// Run against `npm run dev`. Native IPC is simulated; Rust tests cover the backend.
// Set OPENCOWORK_PLAYWRIGHT_DIR to an installed playwright package if not on NODE_PATH.
const { chromium } = require(process.env.OPENCOWORK_PLAYWRIGHT_DIR || 'playwright')
const assert = require('node:assert/strict')

;(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true })
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } })
  const errors = []
  page.on('pageerror', error => errors.push(String(error)))
  try {
    await page.addInitScript(() => {
      localStorage.setItem('opencowork-locale-version', '5')
      localStorage.setItem('opencowork-locale', 'en')
      const tasks = JSON.parse(localStorage.getItem('fixture-tasks') || '[]')
      tasks.forEach(task => { if (task.status === 'running') task.status = 'interrupted' })
      const persist = () => localStorage.setItem('fixture-tasks', JSON.stringify(tasks))
      const config = {
        model: { provider: 'api', api: { type: 'openai', request_format: 'chat_completions', endpoint: 'http://localhost', model: 'fixture', api_key: '', max_output_tokens: 8192 }, ollama: {} },
        capture: {}, storage: { max_context_tokens: 128000 },
        tools: { mode: 'whitelist', allowed_dirs: ['D:/fixture'], allowed_commands: [], parallel_reads: 4, mcp_servers: [], browser_server: null }, ui: { show_progress: true },
      }
      window.fixture = {
        tasks,
        finish: id => {
          const task = tasks.find(task => task.id === id)
          task.status = 'completed'
          task.response = JSON.stringify({ response: 'First task finished exactly once' })
          task.artifacts = ['D:/fixture/result.txt']
          task.verification = ['Checked fixture result']
          task.steps = [{ title: 'Create result', status: 'completed' }]
          persist()
        },
      }
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} }
      window.__TAURI_INTERNALS__ = {
        transformCallback: () => 1,
        unregisterCallback() {},
        invoke: async (command, args = {}) => {
          switch (command) {
            case 'get_config': return config
            case 'get_system_locale': return 'en'
            case 'ensure_bash_runtime': return { available: true }
            case 'get_capture_status': return { is_capturing: false, record_count: 0, last_capture_time: null }
            case 'list_skills': case 'get_recent_alerts': case 'list_profiles': return []
            case 'get_skills_dir': return 'D:/fixture/skills'
            case 'plugin:event|listen': return 1
            case 'list_agent_tasks': return structuredClone(tasks)
            case 'start_agent_task': {
              const input = args.input
              const previous = tasks.find(task => task.id === input.resume_from)
              const task = { id: `fixture-${tasks.length}`, conversation_id: input.conversation_id, objective: previous?.objective || input.message, status: 'running', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), steps: [], artifacts: [], verification: [], response: null, error: null }
              tasks.push(task)
              persist()
              return task.id
            }
            case 'cancel_request': {
              tasks.find(task => task.id === args.requestId).status = 'cancelled'
              persist()
              return
            }
            case 'preview_task_artifact': return { kind: 'text', content: '<script>unsafe markup must stay text</script>\nResult preview' }
            default: return null
          }
        },
      }
    })
    await page.goto(process.env.OPENCOWORK_UI_URL || 'http://127.0.0.1:1420')
    const send = async text => {
      await page.locator('textarea').fill(text)
      await page.locator('textarea').press('Enter')
      await page.waitForFunction(text => window.fixture.tasks.some(task => task.objective === text), text)
    }
    const expandTasks = async () => {
      if (!await page.locator('.task-panel .n-collapse-item--active').count()) {
        await page.locator('.task-panel .n-collapse-item__header').click()
      }
    }
    await send('First task')
    await page.locator('.new-chat-button').click()
    await send('Second task')
    await page.evaluate(() => window.fixture.finish('fixture-0'))
    await page.waitForFunction(() => JSON.parse(localStorage.getItem('opencowork-conversations')).some(c => c.title === 'First task' && c.messages.some(m => m.taskId === 'fixture-0')))
    assert.equal(await page.getByText('First task finished exactly once', { exact: true }).count(), 0)
    await page.locator('.conversation-title').filter({ hasText: /^First task$/ }).click()
    await page.getByText('First task finished exactly once', { exact: true }).waitFor()
    await expandTasks()
    await page.getByRole('button', { name: 'D:/fixture/result.txt', exact: true }).click()
    await page.locator('.artifact-preview').waitFor()
    assert.match(await page.locator('.artifact-preview').innerText(), /<script>/)
    assert.equal(await page.locator('.artifact-preview script').count(), 0)
    await page.keyboard.press('Escape')
    await page.locator('.conversation-title').filter({ hasText: /^Second task$/ }).click()
    await expandTasks()
    await page.getByRole('button', { name: 'Stop task', exact: true }).click()
    await page.getByRole('button', { name: 'Inspect and resume', exact: true }).click()
    await page.waitForFunction(() => window.fixture.tasks.length === 3)
    await page.reload()
    await page.locator('.conversation-title').filter({ hasText: /^Second task$/ }).click()
    await expandTasks()
    await page.getByText('Interrupted', { exact: true }).waitFor()
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('opencowork-conversations')).find(c => c.title === 'First task').messages.filter(m => m.taskId === 'fixture-0').length), 1)
    if (process.env.OPENCOWORK_UI_SCREENSHOT) await page.screenshot({ path: process.env.OPENCOWORK_UI_SCREENSHOT, fullPage: true })
    assert.deepEqual(errors, [])
    console.log('PASS: conversation routing, background completion, deduplication, stop/resume, reload recovery UI and safe artifact preview (mock IPC).')
  } catch (error) {
    console.error(await page.locator('.task-panel').innerText().catch(() => 'No task panel'))
    console.error(await page.evaluate(() => window.fixture.tasks))
    if (process.env.OPENCOWORK_UI_SCREENSHOT) await page.screenshot({ path: process.env.OPENCOWORK_UI_SCREENSHOT, fullPage: true })
    throw error
  } finally { await browser.close() }
})().catch(error => { console.error(error); process.exitCode = 1 })
