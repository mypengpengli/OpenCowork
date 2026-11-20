import json, os, socket, subprocess, tempfile, time, threading, http.server, urllib.request
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        if not body.get('stream'):
            self.send_response(200); self.end_headers()
            self.wfile.write(json.dumps({'choices':[{'message':{'content':'summary'},'finish_reason':'stop'}]}).encode()); return
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
        def emit(delta, reason=None):
            self.wfile.write(('data: '+json.dumps({'choices':[{'index':0,'delta':delta,'finish_reason':reason}]})+'\n\n').encode()); self.wfile.flush()
        if any('UI created task' in str(m.get('content')) for m in body['messages']):
            if not any(m['role']=='tool' for m in body['messages']):
                time.sleep(3)
                emit({'tool_calls':[{'index':0,'id':'ui-read','function':{'name':'read_file','arguments':'{"path":"ui.txt"}'}}]})
                emit({},'tool_calls')
            else:
                emit({'content':'Manual task result verified.'});emit({},'stop')
            self.wfile.write(b'data: [DONE]\n\n');self.wfile.flush();return
        emit({'reasoning_content':'Inspecting the saved constraints. '})
        time.sleep(1)
        emit({'reasoning_content':'<img src=x onerror=alert(1)> Keep this as plain text.'})
        time.sleep(1)
        emit({'content':'Verified **streaming** output'})
        time.sleep(1)
        emit({'content':' arrives without rebuilding earlier messages.'})
        time.sleep(1)
        emit({},'stop'); self.wfile.write(b'data: [DONE]\n\n'); self.wfile.flush()

server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Model)
threading.Thread(target=server.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory(prefix='oc-upgrades-ui-') as work:
    root=Path(work); project=root/'project'; config=root/'config'
    (project/'.opencowork').mkdir(parents=True); config.mkdir()
    (project/'ui.txt').write_text('UI evidence',encoding='utf-8')
    (project/'.opencowork/settings.json').write_text(json.dumps({'model':'ui-fixture','provider':{'name':'UI fixture','apiKeyEnv':'UI_FIXTURE_KEY','baseUrl':f'http://127.0.0.1:{server.server_port}/v1','maxRetries':0},'permissionMode':'read-only','teamMemorySync':{'enabled':False}}))
    with socket.socket() as sock: sock.bind(('127.0.0.1',0)); port=sock.getsockname()[1]
    env={k:v for k,v in os.environ.items() if not k.startswith('OPENCOWORK_')}
    env.update(OPENCOWORK_CONFIG_HOME=str(config),OPENCOWORK_SHELL_PORT=str(port),UI_FIXTURE_KEY='fixture-only',NO_PROXY='localhost,127.0.0.1')
    host=subprocess.Popen([os.environ.get('OC_UI_EXE',str(Path(os.environ['LOCALAPPDATA'])/'OpenClaw/target/debug/opencowork-shell.exe'))],cwd=project,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=0x08000000)
    try:
        for _ in range(200):
            try: urllib.request.urlopen(f'http://127.0.0.1:{port}/api/health'); break
            except OSError: time.sleep(.05)
        with sync_playwright() as p:
            browser=p.chromium.launch(channel='msedge',headless=True)
            page=browser.new_page(viewport={'width':1440,'height':1000})
            errors=[]; page.on('pageerror',lambda e:errors.append(str(e)))
            page.goto(f'http://127.0.0.1:{port}/'); page.wait_for_selector('#composer-input')
            page.locator('#composer-input').fill('Verify the response display')
            page.locator('#composer-form').evaluate('(form)=>form.requestSubmit()')
            reasoning=page.locator('.message-reasoning').last
            expect(reasoning.locator('summary')).to_be_visible(timeout=15000)
            reasoning.locator('summary').click()
            expect(reasoning.locator('.message-reasoning-text')).to_contain_text('Inspecting')
            page.evaluate("window.savedReasoning = document.querySelector('.message-reasoning')")
            expect(reasoning.locator('.message-reasoning-text')).to_contain_text('<img src=x',timeout=5000)
            assert page.evaluate("savedReasoning === document.querySelector('.message-reasoning')")
            assert page.locator('.message-reasoning img').count()==0
            expect(page.locator('#message-list')).to_contain_text('Verified',timeout=5000)
            page.evaluate("window.savedUser = document.querySelector('[data-message-index=\"0\"]'); window.savedReasoning = document.querySelector('.message-reasoning')")
            expect(page.locator('#message-list')).to_contain_text('without rebuilding',timeout=5000)
            assert page.evaluate("savedUser === document.querySelector('[data-message-index=\"0\"]') && savedReasoning === document.querySelector('.message-reasoning')")
            assert reasoning.get_attribute('open') is not None
            page.wait_for_timeout(1500)
            page.screenshot(path=str(Path(os.environ['TEMP'])/'oc-upgrades-chat.png'),full_page=True)
            page.locator('#nav-settings-button').click()
            page.locator('[data-settings-tab="permission"]').click()
            expect(page.locator('#subagent-concurrency')).to_have_value('6')
            page.locator('#subagent-concurrency').fill('8')
            page.get_by_role('button',name='保存子智能体设置',exact=True).click()
            expect(page.get_by_text('设置已保存',exact=True).last).to_be_visible()
            page.reload(); page.locator('#nav-settings-button').click(); page.locator('[data-settings-tab="permission"]').click()
            expect(page.locator('#subagent-concurrency')).to_have_value('8')
            page.locator('#locale-toggle-button').click()
            expect(page.get_by_text('Concurrent subagents',exact=True)).to_be_visible()
            page.screenshot(path=str(Path(os.environ['TEMP'])/'oc-upgrades-settings.png'),full_page=True)
            page.locator('[data-settings-tab="provider"]').click()
            page.locator('#settings-provider-panel').get_by_role('button',name='Edit',exact=True).click()
            expect(page.locator('#provider-streaming')).to_be_checked()
            page.locator('#provider-context-window').fill('32000')
            page.locator('#provider-output-limit').fill('1024')
            page.locator('#provider-images').select_option('false')
            page.locator('#provider-reasoning-replay').check()
            page.locator('#provider-form').evaluate('(form)=>form.requestSubmit()')
            expect(page.locator('#drawer-status')).to_have_text('Provider profile saved.')
            page.reload(); page.locator('#nav-settings-button').click(); page.locator('[data-settings-tab="provider"]').click()
            page.locator('#settings-provider-panel').get_by_role('button',name='Edit',exact=True).click()
            expect(page.locator('#provider-context-window')).to_have_value('32000')
            expect(page.locator('#provider-output-limit')).to_have_value('1024')
            expect(page.locator('#provider-images')).to_have_value('false')
            expect(page.locator('#provider-reasoning-replay')).to_be_checked()
            page.locator('#close-drawer-button').click()
            page.locator('#nav-chat-button').click()
            page.locator('#composer-add').click()
            page.get_by_role('button',name='Subagent tasks',exact=True).click()
            expect(page.get_by_text('No child tasks',exact=True)).to_be_visible()
            page.locator('#subagent-task-name').fill('ui-task')
            page.locator('#subagent-task-prompt').fill('UI created task: read ui.txt and report evidence.')
            page.get_by_role('button',name='Create child task',exact=True).click()
            item=page.locator('details[data-id]').filter(has=page.locator('summary',has_text='ui-task')).first
            expect(item.locator('summary')).to_be_visible(timeout=15000)
            item.locator('summary').click()
            if item.get_by_role('button',name='Send instructions',exact=True).count():
                item.locator('textarea').fill('Preserve the UI evidence in the final result.')
                item.get_by_role('button',name='Send instructions',exact=True).click()
            expect(item).to_contain_text('Manual task result verified.',timeout=20000)
            expect(item).to_contain_text('Completed tool: read_file')
            assert not errors, errors
            browser.close()
        print('PASS: Edge UI reasoning, safe text, incremental DOM, saved concurrency and localization')
    finally:
        host.terminate(); host.wait(10); server.shutdown()
