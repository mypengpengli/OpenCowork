"""Isolated scheduler API and real Edge UI checks; no user settings or live keys."""
import http.client
import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time

from playwright.sync_api import sync_playwright, expect


class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        # Keep the task active long enough to verify running-job mutation locks.
        time.sleep(2)
        for delta, reason in [({'content': 'Scheduled task evidence saved.'}, None), ({}, 'stop')]:
            chunk = {'choices': [{'index': 0, 'delta': delta, 'finish_reason': reason}]}
            self.wfile.write(('data: ' + json.dumps(chunk) + '\n\n').encode())
        self.wfile.write(b'data: [DONE]\n\n')
        self.wfile.flush()


def main():
    executable = str(Path(sys.argv[1]).resolve())
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory(prefix='oc-schedules-') as work:
        root = Path(work)
        project, config = root/'project', root/'config'
        (project/'.opencowork').mkdir(parents=True)
        config.mkdir()
        (project/'.opencowork/settings.json').write_text(json.dumps({
            'model': 'schedule-fixture', 'permissionMode': 'read-only',
            'provider': {'name': 'Fixture', 'apiKeyEnv': 'SCHEDULE_FIXTURE_KEY',
                         'baseUrl': f'http://127.0.0.1:{server.server_port}/v1', 'maxRetries': 0},
            'teamMemorySync': {'enabled': False},
        }), encoding='utf-8')
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            port = listener.getsockname()[1]
        env = {k: v for k, v in os.environ.items() if not k.startswith('OPENCOWORK_')}
        env.update(OPENCOWORK_CONFIG_HOME=str(config), OPENCOWORK_SHELL_PORT=str(port),
                   SCHEDULE_FIXTURE_KEY='fixture-only', NO_PROXY='127.0.0.1,localhost')

        def api(payload=None, status=200):
            connection = http.client.HTTPConnection('127.0.0.1', port, timeout=20)
            connection.request('POST' if payload is not None else 'GET', '/api/schedules',
                               json.dumps(payload) if payload is not None else None,
                               {'Content-Type': 'application/json'})
            response = connection.getresponse()
            body = response.read()
            connection.close()
            assert response.status == status, (response.status, body)
            return json.loads(body)

        def wait_for(predicate):
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                value = predicate()
                if value:
                    return value
                time.sleep(.1)
            raise AssertionError('Timed out waiting for schedule state')

        def start():
            process = subprocess.Popen([executable], cwd=project, env=env,
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                       creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
            def ready():
                try:
                    return api()
                except OSError:
                    return None
            wait_for(ready)
            return process

        process = start()
        try:
            api({'text': 'invalid', 'intervalSeconds': 1, 'dailyTime': '09:00'}, 400)
            api({'action': 'pause', 'id': 'missing'}, 400)
            daily = {'name': 'Daily fixture', 'text': 'Read fixture', 'dailyTime': '09:00', 'timezone': 'Asia/Shanghai'}
            job = api(daily)
            api({'action': 'pause', 'id': job['id']})
            edited = api({**daily, 'action': 'update', 'id': job['id'], 'name': 'Updated daily'})
            assert edited['status'] == 'paused'
            api({**daily, 'action': 'update', 'id': job['id'], 'timezone': 'invalid'}, 400)
            assert api()['jobs'][0]['name'] == 'Updated daily'
            process.terminate(); process.wait(timeout=10)
            process = start()
            assert api()['jobs'][0]['status'] == 'paused'
            api({'action': 'delete', 'id': job['id']})

            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel='msedge', headless=True)
                page = browser.new_page(viewport={'width': 1440, 'height': 1000})
                errors = []
                page.on('pageerror', lambda error: errors.append(str(error)))
                page.on('dialog', lambda dialog: dialog.accept())
                page.goto(f'http://127.0.0.1:{port}/')
                page.locator('#nav-schedules-button').click()
                view = page.locator('#schedules-view')
                expect(view).to_be_visible()
                expect(view).to_contain_text('还没有定时任务')
                view.get_by_role('button', name='新建任务', exact=True).click()
                page.locator('#schedule-name').fill('UI scheduled fixture')
                page.locator('#schedule-text').fill('Read the scheduled fixture')
                page.locator('#schedule-minutes').fill('60')
                # Polling must preserve an unsaved draft and focus.
                page.wait_for_timeout(3200)
                expect(page.locator('#schedule-text')).to_have_value('Read the scheduled fixture')
                view.get_by_role('button', name='保存任务', exact=True).click()
                card = view.locator('[data-job-id]')
                expect(card).to_have_count(1)
                expect(card).to_contain_text('每 60 分钟')
                card.get_by_role('button', name='暂停', exact=True).click()
                expect(card).to_contain_text('已暂停')
                card.get_by_role('button', name='编辑', exact=True).click()
                page.locator('#schedule-name').fill('Renamed fixture')
                page.locator('#schedule-mode').select_option('daily')
                page.locator('#schedule-daily').fill('09:30')
                page.locator('#schedule-timezone').fill('Asia/Shanghai')
                view.get_by_role('button', name='保存任务', exact=True).click()
                expect(card).to_contain_text('Renamed fixture')
                expect(card).to_contain_text('已暂停')
                expect(card).to_contain_text('09:30')
                identifier = api()['jobs'][0]['id']
                card.get_by_role('button', name='立即执行', exact=True).click()
                wait_for(lambda: any(j['status'] == 'running' for j in api()['jobs']))
                api({'action': 'delete', 'id': identifier}, 400)
                result = wait_for(lambda: api()['inbox'])
                assert result[0]['status'] == 'completed', result
                job = api()['jobs'][0]
                assert job['lastStatus'] == 'completed' and job['lastRunAt']
                view.get_by_role('button', name='刷新', exact=True).click()
                expect(view.get_by_role('button', name='查看结果', exact=True)).to_be_visible()
                artifact = Path(__file__).resolve().parent.parent/'.tmp/schedules-main.png'
                artifact.parent.mkdir(exist_ok=True)
                page.screenshot(path=str(artifact), full_page=True)
                view.get_by_role('button', name='查看结果', exact=True).click()
                expect(page.locator('#chat-view')).to_be_visible()
                expect(page.locator('#message-list')).to_contain_text('Scheduled task evidence saved.')
                page.locator('#nav-schedules-button').click()
                card.get_by_role('button', name='删除', exact=True).click()
                expect(card).to_have_count(0)
                assert len(api()['inbox']) == 1
                assert api()['inbox'][0]['jobName'] == 'Renamed fixture'
                page.reload(); page.locator('#nav-schedules-button').click()
                expect(view.get_by_role('button', name='查看结果', exact=True)).to_be_visible()
                page.locator('#locale-toggle-button').click()
                expect(view.get_by_role('button', name='New task', exact=True)).to_be_visible()
                page.set_viewport_size({'width': 640, 'height': 900})
                assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
                assert not errors, errors
                browser.close()
            print('PASS: schedule CRUD, validation, paused edits, restart persistence, execution locks, scheduled model execution with a local fixture, result navigation, retained history, draft polling, localization and narrow layout')
        finally:
            process.terminate(); process.wait(timeout=10)
            server.shutdown()


if __name__ == '__main__':
    main()
