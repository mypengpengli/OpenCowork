"""Isolated multi-workspace regression; never uses the user's model credentials."""
import concurrent.futures
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
from urllib.parse import quote


def run(executable):
    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            payload = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            done = any(m['role'] == 'tool' for m in payload['messages'])
            delta = {'content': 'Workspace write verified.'} if done else {
                'tool_calls': [{'index': 0, 'id': 'workspace-write', 'type': 'function', 'function': {
                    'name': 'write_file', 'arguments': json.dumps({'path': 'created-by-agent.txt', 'content': 'selected workspace'})}}]}
            data = ('data: ' + json.dumps({'choices': [{'index': 0, 'delta': delta, 'finish_reason': 'stop' if done else 'tool_calls'}]}) + '\n\ndata: [DONE]\n\n').encode()
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    provider = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    threading.Thread(target=provider.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix='cowork-workspaces-') as temporary:
            root = Path(temporary)
            first, second, config = root/'first', root/'second project 中文', root/'config'
            config.mkdir()
            for project in (first, second):
                (project/'.opencowork').mkdir(parents=True)
                (project/'identity.txt').write_text(project.name, encoding='utf-8')
                settings = {'model': 'fixture', 'permissionMode': 'workspace-write', 'provider': {
                    'name': 'Fixture', 'apiKeyEnv': 'WORKSPACE_TEST_KEY', 'baseUrl': f'http://127.0.0.1:{provider.server_port}/v1'},
                    'teamMemorySync': {'enabled': False}}
                (project/'.opencowork/settings.json').write_text(json.dumps(settings), encoding='utf-8')
            with socket.socket() as sock:
                sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]
            env = {k: v for k, v in os.environ.items() if not k.startswith('OPENCOWORK_')}
            env.update(OPENCOWORK_CONFIG_HOME=str(config), OPENCOWORK_SHELL_PORT=str(port), WORKSPACE_TEST_KEY='fixture-only', NO_PROXY='127.0.0.1,localhost', no_proxy='127.0.0.1,localhost')
            log = (root/'shell.log').open('w')
            process = None

            def api(path, body=None, workspace=None, expected=200, method=None):
                connection = http.client.HTTPConnection('127.0.0.1', port, timeout=45)
                headers = {'Content-Type': 'application/json'}
                if workspace:
                    headers['X-OpenCowork-Workspace'] = workspace
                connection.request(method or ('POST' if body is not None else 'GET'), path, json.dumps(body) if body is not None else None, headers)
                response = connection.getresponse(); data = response.read(); connection.close()
                assert response.status == expected, (path, response.status, data)
                if path == '/api/chat' and expected == 200:
                    return next(json.loads(line)['response'] for line in data.splitlines() if json.loads(line)['type'] == 'complete')
                return json.loads(data) if data else None

            def start():
                child = subprocess.Popen([executable], cwd=first, env=env, stdout=log, stderr=log, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
                for _ in range(100):
                    try:
                        api('/api/health', expected=204); return child
                    except (OSError, AssertionError):
                        if child.poll() is not None:
                            raise AssertionError((root/'shell.log').read_text())
                        time.sleep(.1)
                child.terminate(); child.wait(); raise AssertionError('Host did not start')

            try:
                process = start()
                a = api('/api/workspaces')[0]['id']
                b = api('/api/workspaces/open', {'path': str(second)})['id']
                assert api('/api/workspaces/open', {'path': str(second/'.')})['id'] == b
                api('/api/workspaces/open', {'path': 'relative'}, expected=400)
                api('/api/workspaces/open', {'path': str(second/'identity.txt')}, expected=400)
                api('/api/bootstrap', workspace='unknown-id', expected=400)
                assert second in [Path(p) for p in api('/api/workspaces/directories?path=' + quote(str(root)))['folders']]

                def inspect(item):
                    identity, selected = item
                    assert Path(api('/api/bootstrap', workspace=identity)['cwd']) == selected
                    assert api('/api/workspace/file?path=identity.txt', workspace=identity)['content'] == selected.name
                    api('/api/workspace/file?path=../identity.txt', workspace=identity, expected=400)
                with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
                    list(pool.map(inspect, [(a, first), (b, second)] * 4))
                result = api('/api/chat', {'input': 'Write a workspace fixture'}, workspace=b)
                assert result['status'] == 'completed', result
                assert (second/'created-by-agent.txt').read_text() == 'selected workspace'
                assert not (first/'created-by-agent.txt').exists()
                session_id = result['session']['id'] if 'session' in result else result.get('sessionId', result.get('session_id'))
                if not session_id:
                    session_id = next((config/'sessions').glob('*.json')).stem
                before = (config/'sessions'/f'{session_id}.json').read_bytes()
                api('/api/chat', {'input': 'Must not move this session', 'sessionId': session_id}, workspace=a, expected=400)
                assert (config/'sessions'/f'{session_id}.json').read_bytes() == before
                assert api('/api/sessions', workspace=a)[0]['workspace'] == str(second)

                # Subfolder status must use paths relative to that selected folder.
                def git(*args):
                    return subprocess.run(['git', '-c', f'safe.directory={second.as_posix()}', *args], cwd=second, check=True, capture_output=True)
                git('init'); (second/'nested').mkdir(); (second/'nested/inside.txt').write_text('inside')
                nested = api('/api/workspaces/open', {'path': str(second/'nested')})['id']
                listing = api('/api/workspace', workspace=nested)
                assert 'inside.txt' in listing['files'], listing
                assert listing['changes'] and all(not c['path'].startswith(('nested/', '../')) and c['path'] != 'identity.txt' for c in listing['changes']), listing

                # Forget an empty recent project without deleting its folder or
                # invalidating a page that is already using its context.
                api('/api/workspaces/' + nested, method='DELETE')
                assert nested not in [w['id'] for w in api('/api/workspaces')]
                assert (second/'nested/inside.txt').read_text() == 'inside'
                assert Path(api('/api/bootstrap', workspace=nested)['cwd']) == second/'nested'
                assert (config/'sessions'/f'{session_id}.json').read_bytes() == before

                api('/api/features', {'computerEnabled': True, 'backgroundMemoryEnabled': True}, workspace=b)
                features = api('/api/features', {'computerEnabled': False}, workspace=b)
                assert features['computerEnabled'] is False and features['backgroundMemoryEnabled'] is True
                features = api('/api/features', {'computerEnabled': True}, workspace=b)
                assert features['computerEnabled'] is True and features['backgroundMemoryEnabled'] is True
                assert api('/api/features', workspace=a)['backgroundMemoryEnabled'] is False
                api('/api/features', {'backgroundMemoryEnabled': False}, workspace=b)

                api('/api/slash', {'input': '/permissions read-only'}, workspace=b)
                assert api('/api/bootstrap', workspace=b)['permissionMode'] == 'read-only'
                assert api('/api/bootstrap', workspace=a)['permissionMode'] == 'workspace-write'
                api('/api/slash', {'input': '/permissions unknown'}, workspace=b, expected=400)
                api('/api/slash', {'input': '/compact', 'sessionId': '../escape'}, workspace=b, expected=400)
                api('/api/slash', {'input': '/compact', 'sessionId': session_id}, workspace=a, expected=400)
                archive = json.loads(before)
                archive['messages'] = archive['messages'] * 8
                (config/'sessions'/f'{session_id}.json').write_text(json.dumps(archive), encoding='utf-8')
                compact = api('/api/slash', {'input': '/compact', 'sessionId': session_id}, workspace=b)
                assert 'Compacted: 0' not in compact['output'], compact
                stored = json.loads((config/'sessions'/f'{session_id}.json').read_text(encoding='utf-8'))
                assert len(stored['messages']) < len(archive['messages'])
                assert stored['workspace'] == archive['workspace']

                process.terminate(); process.wait(timeout=15); process = start()
                assert b in [w['id'] for w in api('/api/workspaces')]
                assert nested not in [w['id'] for w in api('/api/workspaces')]
                reopened = api('/api/workspaces/open', {'path': str(second/'nested')})['id']
                assert Path(api('/api/bootstrap', workspace=reopened)['cwd']) == second/'nested'
                inspect((b, second))
                print('PASS: folder validation, recent workspace persistence, concurrent project reads, scoped agent writes, session ownership, subfolder Git paths, recent removal and persistent slash commands')
            finally:
                if process and process.poll() is None:
                    process.terminate(); process.wait(timeout=15)
                log.close()
    finally:
        provider.shutdown(); provider.server_close()


if __name__ == '__main__':
    run(str(Path(sys.argv[1]).resolve()))
