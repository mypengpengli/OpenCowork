"""Loopback integration coverage for response replay, compaction and queued agents."""
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


def check(executable):
    active = peak = 0
    expected_peak = 6
    barrier_open = False
    lock = threading.Lock()
    counts = {}
    benchmark = []
    summary_calls = 0
    replay_verified = False

    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            nonlocal active, peak, summary_calls, replay_verified, barrier_open
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            model = body['model']
            messages = body['messages']
            if not body.get('stream'):
                with lock:
                    summary_calls += 1
                output = {'choices': [{'message': {'content': 'Goal: preserve KEEP_ME. Earlier file reads verified. Continue the remaining reads and report the final result.'}, 'finish_reason': 'stop'}], 'usage': {'prompt_tokens': 10, 'completion_tokens': 10}}
                data = json.dumps(output).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)
                return
            events = []

            def event(delta, reason=None):
                events.append({'choices': [{'index': 0, 'delta': delta, 'finish_reason': reason}]})

            def call(name, value, identity):
                event({'tool_calls': [{'index': len(events), 'id': identity, 'type': 'function', 'function': {'name': name, 'arguments': json.dumps(value)}}]})

            child = model == 'child-fixture'
            if child:
                with lock:
                    active += 1
                    peak = max(peak, active)
                barrier_deadline=time.monotonic()+60
                while time.monotonic()<barrier_deadline:
                    with lock:
                        if active>=expected_peak: barrier_open=True
                        saturated=barrier_open
                    if saturated: break
                    time.sleep(.01)
                time.sleep(.25)
                event({'reasoning_content': 'Review the assigned work.'})
                event({'content': 'Child result with verification evidence. ' * 20})
            elif model == 'process-log-limit-fixture':
                results=[json.loads(m['content']) for m in messages if m['role']=='tool']
                if not results: call('Process',{'action':'start','command':"Write-Output ('x'*18000000); Write-Output 'FINAL'",'timeoutMs':30000},'large-output-start')
                elif results[-1].get('status') in ('queued','running'):
                    call('Process',{'action':'inspect','id':results[0]['id'],'waitMs':30000},'large-output-status-'+str(len(results)))
                else:
                    assert results[-1]['status']=='exited' and results[-1]['truncated'] and results[-1]['exitCode']==0,results
                    event({'content':'Bounded log retained sixteen MiB while draining remaining output.'})
            elif model == 'process-server-fixture':
                results=[json.loads(m['content']) for m in messages if m['role']=='tool']
                if len(results)==0: call('Process',{'action':'start','command':f'python -u -m http.server {web_port} --bind 127.0.0.1','timeoutMs':30000},'server-start')
                elif len(results)==1: call('Process',{'action':'read','id':results[0]['id'],'untilText':'Serving HTTP','waitMs':10000},'server-ready')
                elif len(results)==2:
                    browser=http.client.HTTPConnection('127.0.0.1',web_port,timeout=5);browser.request('GET','/small.txt');response=browser.getresponse();assert response.status==200 and response.read()==b'verified';browser.close()
                    call('Process',{'action':'cancel','id':results[0]['id']},'server-stop')
                elif len(results)==3:
                    time.sleep(.3);call('Process',{'action':'inspect','id':results[0]['id']},'server-status')
                else:
                    assert results[-1]['status']=='cancelled',results
                    event({'content':'Background server served its page while the model continued working.'})
            elif model in ('process-pipe-fixture','process-pty-fixture'):
                results = [json.loads(m['content']) for m in messages if m['role']=='tool']
                stage=len(results)
                if stage==0:
                    command="Write-Output 'PROCESS_READY'; $value=[Console]::ReadLine(); Write-Output ('ECHO:'+ $value); Start-Sleep -Seconds 20" if model=='process-pipe-fixture' else "Write-Output 'PROCESS_READY'"
                    call('Process',{'action':'start','command':command,'pty':model=='process-pty-fixture','timeoutMs':30000},'process-start')
                elif stage==1:
                    call('Process',{'action':'read','id':results[0]['id'],'waitMs':10000,'untilText':'\nPROCESS_READY' if model=='process-pty-fixture' else 'PROCESS_READY'},'process-read-ready')
                elif stage==2:
                    assert 'PROCESS_READY' in results[1]['output'], results
                    text='hello\n' if model=='process-pipe-fixture' else "Write-Output 'ECHO:hello'\r"
                    call('Process',{'action':'input','id':results[0]['id'],'text':text},'process-input')
                elif stage==3:
                    call('Process',{'action':'read','id':results[0]['id'],'cursor':results[1]['cursor'],'waitMs':10000,'untilText':'\nECHO:hello' if model=='process-pty-fixture' else 'ECHO:hello'},'process-read-echo')
                elif stage==4:
                    if 'ECHO:hello' not in results[3]['output']:
                        call('Process',{'action':'read','id':results[0]['id'],'cursor':results[3]['cursor'],'waitMs':1000},'process-read-more')
                    else:
                        call('Process',{'action':'read','id':results[0]['id'],'cursor':results[3]['cursor'],'waitMs':100},'process-read-timeout')
                elif stage==5:
                    assert 'ECHO:hello' in results[3]['output'] or 'ECHO:hello' in results[4]['output'], results
                    if model=='process-pty-fixture': call('Process',{'action':'input','id':results[0]['id'],'text':'Start-Sleep -Seconds 20\r'},'pty-sleep')
                    else: call('Process',{'action':'cancel','id':results[0]['id']},'process-cancel')
                elif stage==6:
                    time.sleep(.4)
                    call('Process',{'action':'interrupt' if model=='process-pty-fixture' else 'inspect','id':results[0]['id']},'process-interrupt-status')
                elif model=='process-pty-fixture' and stage==7:
                    call('Process',{'action':'input','id':results[0]['id'],'text':"Write-Output 'AFTER_INTERRUPT'\r"},'pty-after-interrupt')
                elif model=='process-pty-fixture' and stage==8:
                    call('Process',{'action':'read','id':results[0]['id'],'cursor':results[4]['cursor'],'untilText':'\nAFTER_INTERRUPT','waitMs':5000},'pty-interrupt-read')
                elif model=='process-pty-fixture' and stage==9:
                    assert results[-1]['ready'],results
                    call('Process',{'action':'resize','id':results[0]['id'],'rows':40,'cols':100},'pty-resize')
                elif model=='process-pty-fixture' and stage==10:
                    call('Process',{'action':'cancel','id':results[0]['id']},'pty-cancel')
                elif model=='process-pty-fixture' and stage==11:
                    time.sleep(.4);call('Process',{'action':'inspect','id':results[0]['id']},'pty-final-status')
                else:
                    assert results[-1]['status']=='cancelled',results
                    event({'content':'Process session input, cursors and cancellation verified.'})
            elif model in ('nested-root-fixture', 'nested-child-fixture'):
                results = [m for m in messages if m['role'] == 'tool']
                if not results:
                    call('Subagents', {'action': 'create', 'name': 'nested-parent' if model == 'nested-root-fixture' else 'nested-leaf', 'task': 'Verify the delegated item and return all evidence', 'model': 'nested-child-fixture' if model == 'nested-root-fixture' else 'child-fixture'}, 'nested-create')
                elif len(results) == 1:
                    task = json.loads(results[0]['content'])
                    call('Subagents', {'action': 'wait', 'id': task['id'], 'timeoutMs': 30000}, 'nested-wait')
                else:
                    assert json.loads(results[-1]['content'])['status'] == 'completed'
                    event({'content': 'Nested delegation verified without exhausting execution slots.'})
            elif model == 'resume-child-fixture':
                counts[model]=counts.get(model,0)+1
                if counts[model]==1: event({'content':'RESUME_CHECKPOINT preserved progress.'})
                else:
                    assert any(m['role']=='assistant' and 'RESUME_CHECKPOINT' in str(m.get('content')) for m in messages)
                    event({'content':'Saved checkpoint restored under a new root.'})
            elif model == 'fork-child-fixture':
                assert body.get('reasoning_effort')=='high',body
                assert any(m['role']=='assistant' and 'Reasoning replay verified.' in str(m.get('content')) for m in messages)
                event({'content':'Forked context and independent effort verified.'})
            elif model in ('writer-a-fixture','writer-b-fixture'):
                if not any(m['role']=='tool' for m in messages): call('write_file',{'path':'shared-write.txt','content':model*200},model+'-write')
                else: event({'content':'Workspace write verified.'})
            elif model.startswith('queue-fixture'):
                batch=24 if model.endswith('-20') else 12
                results = [m for m in messages if m['role'] == 'tool']
                if len(results) < batch:
                    for index in range(batch):
                        call('Subagents', {'action': 'create', 'name': f'worker-{index}', 'task': f'Check item {index} and report evidence', 'model': 'child-fixture'}, f'create-{index}')
                elif len(results) < batch*2:
                    for index, result in enumerate(results[:batch]):
                        task = json.loads(result['content'])
                        call('Subagents', {'action': 'wait', 'id': task['id'], 'timeoutMs': 30000}, f'wait-{index}')
                else:
                    assert all(json.loads(m['content'])['status'] == 'completed' for m in results[batch:batch*2])
                    event({'content': 'All twelve delegated tasks completed.'})
            elif model == 'context-fixture':
                counts[model] = counts.get(model, 0) + 1
                index = counts[model]
                if index <= 4:
                    call('read_file', {'path': f'long{index}.txt'}, f'read-{index}')
                else:
                    assert any('KEEP_ME' in str(m.get('content')) for m in messages)
                    event({'content': 'KEEP_ME preserved after the long task.'})
            elif model == 'replay-fixture':
                if any(m['role'] == 'tool' for m in messages):
                    assistant = next(m for m in messages if m['role'] == 'assistant')
                    replay_verified = assistant.get('reasoning_content') == 'Check the file first.'
                    assert replay_verified
                    event({'content': 'Reasoning replay verified.'})
                else:
                    event({'reasoning_content': 'Check the file first.'})
                    event({'tool_calls': [{'index': 0, 'id': 'read-small', 'function': {'name': 'read_file', 'arguments': '{"path":'}}]})
                    event({'content': 'Reading the requested file.'})
                    event({'tool_calls': [{'index': 0, 'function': {'arguments': '"small.txt"}'}}]})
            elif model == 'length-fixture':
                call('write_file', {'path': 'must-not-exist.txt', 'content': 'unsafe'}, 'truncated-write')
            else:
                event({'content': 'Partial output must survive.'})
            if model != 'broken-fixture':
                event({}, 'length' if model == 'length-fixture' or (model=='resume-child-fixture' and counts[model]==1) else 'stop')
            wire = ''.join('data: ' + json.dumps(item) + '\n\n' for item in events)
            if model != 'broken-fixture':
                wire += 'data: [DONE]\n\n'
            data = wire.encode()
            try:
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                pass
            finally:
                if child:
                    with lock:
                        active -= 1

    model_server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    model_server.daemon_threads = True
    threading.Thread(target=model_server.serve_forever, daemon=True).start()
    with socket.socket() as web_socket:
        web_socket.bind(('127.0.0.1',0)); web_port=web_socket.getsockname()[1]
    with tempfile.TemporaryDirectory(prefix='opencowork-runtime-upgrades-') as work:
        root = Path(work)
        project, config = root / 'project', root / 'config'
        (project / '.opencowork').mkdir(parents=True)
        config.mkdir()
        (project / 'small.txt').write_text('verified', encoding='utf-8')
        for index in range(1, 5):
            (project / f'long{index}.txt').write_text(f'file-{index}\n' + 'data ' * 2400, encoding='utf-8')
        settings = {'model': 'replay-fixture', 'provider': {'name': 'Fixture', 'apiKeyEnv': 'UPGRADE_FIXTURE_KEY', 'baseUrl': f'http://127.0.0.1:{model_server.server_port}/v1', 'maxRetries': 0}, 'permissionMode': 'workspace-write', 'execution': {'maxTokens': 1000000, 'maxSeconds': 120, 'maxIterations': 20}, 'mcpServers': {}, 'teamMemorySync': {'enabled': False}}
        settings_file = project / '.opencowork/settings.json'
        def write_settings():
            temporary=settings_file.with_suffix('.test-pending')
            temporary.write_text(json.dumps(settings),encoding='utf-8')
            for attempt in range(200):
                try: os.replace(temporary,settings_file); return
                except PermissionError: time.sleep(.01)
            raise AssertionError('Settings file remained busy')
        write_settings()
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        environment = {k: v for k, v in os.environ.items() if not k.startswith('OPENCOWORK_')}
        environment.update(OPENCOWORK_CONFIG_HOME=str(config), OPENCOWORK_SHELL_PORT=str(port), UPGRADE_FIXTURE_KEY='fixture-only', NO_PROXY='127.0.0.1,localhost', no_proxy='127.0.0.1,localhost')

        def api(path, payload=None):
            connection = http.client.HTTPConnection('127.0.0.1', port, timeout=120)
            connection.request('POST' if payload is not None else 'GET', path, json.dumps(payload) if payload is not None else None, {'Content-Type': 'application/json'})
            response = connection.getresponse()
            data = response.read()
            connection.close()
            assert response.status in (200, 204), (path, response.status, data)
            return json.loads(data) if data else None

        def chat(model, prompt):
            nonlocal barrier_open
            with lock: barrier_open=False
            settings['model'] = model
            write_settings()
            connection = http.client.HTTPConnection('127.0.0.1', port, timeout=120)
            started=time.monotonic()
            connection.request('POST', '/api/chat', json.dumps({'input': prompt, 'turnId': f'fixture-{model}'}), {'Content-Type': 'application/json'})
            response = connection.getresponse()
            assert response.status == 200, response.read()
            messages = [json.loads(line) for line in response if line.strip()]
            connection.close()
            complete = next(m['response'] for m in messages if m['type'] == 'complete')
            benchmark.append({'scenario':model,'elapsedMs':round((time.monotonic()-started)*1000),'status':complete['status'],'providerPeak':peak if model.startswith('queue-fixture') else None,'iterations':complete['iterations'],'estimatedPromptTokens':complete['estimatedPromptTokens'],'usageSource':'local-fixture'})
            return messages, complete

        with (root / 'host.log').open('w+') as log:
            host = subprocess.Popen([str(Path(executable).resolve())], cwd=project, env=environment, stdout=log, stderr=log, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
            try:
                for _ in range(200):
                    try:
                        api('/api/health')
                        break
                    except OSError:
                        time.sleep(.05)
                messages, result = chat('replay-fixture', 'Read the file and verify the result.')
                assert result['status'] == 'completed' and replay_verified, result
                assert any(m.get('event', {}).get('type') == 'assistant_reasoning_delta' for m in messages)
                assert any(b['type'] == 'reasoning' for m in result['session']['messages'] for b in m['blocks'])
                first_assistant = next(m for m in result['session']['messages'] if m['role'] == 'assistant')
                assert [b['type'] for b in first_assistant['blocks']] == ['reasoning', 'tool_use', 'text'], first_assistant
                replay_session_id=result['sessionId']
                print('PASS: reasoning events, saved blocks and tool-round replay', flush=True)
                _, result = chat('broken-fixture', 'Test an interrupted stream')
                assert result['status'] == 'failed' and 'provider_stream_interrupted' in result['error'], result
                assert any('Partial output must survive.' in str(m) for m in result['session']['messages'])
                _, result = chat('length-fixture', 'Test truncated tool output')
                assert result['status'] == 'failed' and not (project / 'must-not-exist.txt').exists(), result
                print('PASS: partial output recovery and truncated tool suppression', flush=True)
                settings['provider']['modelCapabilities'] = {'contextWindow': 14000, 'maxOutputTokens': 128}
                settings['context'] = {'autoCompactBufferTokens': 128, 'maxOutputTokens': 128}
                _, result = chat('context-fixture', 'Read four long files. KEEP_ME is a mandatory constraint.')
                assert result['status'] == 'completed' and result['compacted'], result.get('error')
                assert summary_calls >= 1 and result['session']['context_collapse_archive'], result
                diagnostics = api('/api/context-diagnostics/' + result['sessionId'])
                assert diagnostics['compacted'] and diagnostics['contextWindowTokens'] == 14000, diagnostics
                assert any('KEEP_ME' in str(m) for m in result['session']['context_collapse_archive'])
                print('PASS: live between-tool compaction and source archive', flush=True)
                settings['provider'].pop('modelCapabilities')
                settings.pop('context')
                snapshots = []
                stop_poll = threading.Event()

                def poll():
                    while not stop_poll.wait(.1):
                        try:
                            tasks = api('/api/subagents')['tasks']
                            snapshots.append((sum(t['status'] == 'running' for t in tasks), sum(t['status'] == 'queued' for t in tasks)))
                        except OSError:
                            pass
                observer = threading.Thread(target=poll, daemon=True)
                observer.start()
                _, result = chat('queue-fixture', 'Delegate twelve independent checks and wait for every result.')
                stop_poll.set()
                observer.join(3)
                assert result['status'] == 'completed', result.get('error')
                tasks = api('/api/subagents')['tasks']
                assert len(tasks) == 12 and all(t['status'] == 'completed' for t in tasks), tasks
                assert peak == 6 and max(n for n, _ in snapshots) <= 6, (peak, snapshots)
                assert any(q >= 6 for _, q in snapshots), snapshots
                assert all(len(t['result']) > 200 for t in tasks)
                api('/api/subagents', {'maxConcurrent': 3, 'maxDepth': 2})
                settings['subagents'] = {'maxConcurrent': 3, 'maxDepth': 2}
                assert api('/api/subagents')['settings']['maxConcurrent'] == 3
                print('PASS: twelve tasks, six workers, queued admission, full results and custom settings', flush=True)
                with lock:
                    peak = 0
                expected_peak=3
                _, result = chat('queue-fixture-3', 'Run twelve more checks with the new concurrency limit.')
                assert result['status'] == 'completed' and peak == 3, (result.get('error'), peak)
                api('/api/subagents', {'maxConcurrent': 8, 'maxDepth': 1})
                settings['subagents'] = {'maxConcurrent': 8, 'maxDepth': 1}
                with lock:
                    peak = 0
                expected_peak=8
                _, result = chat('queue-fixture-8', 'Run twelve more checks with eight workers.')
                assert result['status'] == 'completed' and peak == 8, (result.get('error'), peak)
                print('PASS: configured limits of three and eight affect actual worker concurrency', flush=True)
                for limit in (12,20):
                    expected_peak=limit
                    api('/api/subagents',{'maxConcurrent':limit,'maxDepth':1}); settings['subagents']={'maxConcurrent':limit,'maxDepth':1}
                    with lock: peak=0
                    _, result=chat('queue-fixture-'+str(limit),'Run all twelve independent checks.')
                    assert result['status']=='completed' and peak==limit,(limit,peak,result.get('error'))
                settings['provider']['modelCapabilities']={'maxConcurrentRequests':2}
                expected_peak=2
                with lock: peak=0
                _, result=chat('queue-fixture-global','Check provider capacity across worker processes.')
                assert result['status']=='completed' and peak==2,(peak,result.get('error'))
                settings['provider'].pop('modelCapabilities')
                print('PASS: configured twelve/twenty workers and shared provider request permits',flush=True)
                for limit in (1,4):
                    expected_peak=limit;api('/api/subagents',{'maxConcurrent':limit,'maxDepth':1});settings['subagents']={'maxConcurrent':limit,'maxDepth':1}
                    with lock: peak=0
                    _, result=chat('queue-fixture-'+str(limit),'Run the identical twelve-task benchmark.')
                    assert result['status']=='completed' and peak==limit,(limit,peak)
                cancelled = []
                reader = threading.Thread(target=lambda: cancelled.append(chat('queue-fixture-cancel', 'Start child checks, then await cancellation.')), daemon=True)
                reader.start()
                for _ in range(150):
                    current = [t for t in api('/api/subagents')['tasks'] if t['group'] == 'fixture-queue-fixture-cancel']
                    if any(t['status'] == 'running' for t in current):
                        break
                    time.sleep(.05)
                assert current
                api('/api/chat/fixture-queue-fixture-cancel/cancel', {})
                reader.join(20)
                assert cancelled and cancelled[0][1]['status'] == 'cancelled'
                for _ in range(100):
                    current = [t for t in api('/api/subagents')['tasks'] if t['group'] == 'fixture-queue-fixture-cancel']
                    if all(t['status'] not in ('running', 'queued', 'waiting', 'ready') for t in current):
                        break
                    time.sleep(.05)
                assert all(t['status'] not in ('running', 'queued', 'waiting', 'ready') for t in current), current
                print('PASS: parent cancellation stops running children and queued tasks', flush=True)
                api('/api/subagents', {'maxConcurrent': 1, 'maxDepth': 2})
                settings['subagents'] = {'maxConcurrent': 1, 'maxDepth': 2}
                expected_peak=1
                _, result = chat('nested-root-fixture', 'Delegate a nested verification with a single execution slot.')
                nested = [t for t in api('/api/subagents')['tasks'] if t['group'] == 'fixture-nested-root-fixture']
                assert result['status'] == 'completed' and len(nested) == 2 and all(t['status'] == 'completed' for t in nested), (result.get('error'), nested)
                print('PASS: nested delegation releases and reacquires a single execution slot', flush=True)
                settings['permissionMode']='danger-full-access'
                for fixture in ('process-pipe-fixture','process-pty-fixture','process-server-fixture','process-log-limit-fixture'):
                    _, result=chat(fixture,'Start a process, read output, send input and cancel it.')
                    assert result['status']=='completed',result.get('error')
                    assert result['session']['request_metrics'] and all(m['elapsedMs']>=0 and m['status']!='running' for m in result['session']['request_metrics'])
                    assert len({m['id'] for m in result['session']['request_metrics']})==len(result['session']['request_metrics'])
                    print('PASS: '+fixture+' real process lifecycle and persisted request timings',flush=True)
                def wait_task(identity):
                    for _ in range(300):
                        task=next(t for t in api('/api/subagents')['tasks'] if t['id']==identity)
                        if task['status'] not in ('queued','running','waiting','ready'): return task
                        time.sleep(.1)
                    raise AssertionError(task)
                forked=api('/api/subagent-tasks',{'name':'forked','task':'Verify inherited conversation','model':'fork-child-fixture','forkFrom':replay_session_id,'reasoningEffort':'high'})
                assert wait_task(forked['id'])['status']=='completed'
                writers=[api('/api/subagent-tasks',{'name':'writer-'+str(i),'task':'Write and verify the shared file','model':model,'writable':True}) for i,model in enumerate(('writer-a-fixture','writer-b-fixture'))]
                writers=[wait_task(t['id']) for t in writers]
                assert all(t['status']=='completed' and t['evidence'] for t in writers),writers
                writers.sort(key=lambda t:t['startedAt'])
                assert writers[1]['startedAt']>=writers[0]['finishedAt'],writers
                failed=api('/api/subagent-tasks',{'name':'resume-test','task':'Save progress and resume after restart','model':'resume-child-fixture'})
                assert wait_task(failed['id'])['status']=='failed'
                print('PASS: explicit fork, independent reasoning effort, serialized writes and structured evidence',flush=True)
                # Simulate a former host whose OS lease has been released.
                groups = list((config / 'subagents/groups').glob('*.json'))
                stale_group = json.loads(groups[0].read_text(encoding='utf-8'))
                stale_group.update(id='stale-host', host=4294967294, cancelled=False)
                (config / 'subagents/groups/stale-host.json').write_text(json.dumps(stale_group), encoding='utf-8')
                stale_task = dict(nested[0], id='stale-task', group='stale-host', status='running')
                (config / 'subagents/tasks/stale-task.json').write_text(json.dumps(stale_task), encoding='utf-8')
                host.terminate(); host.wait(10)
                host = subprocess.Popen([str(Path(executable).resolve())], cwd=project, env=environment, stdout=log, stderr=log, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
                for _ in range(200):
                    try:
                        restored = next((t for t in api('/api/subagents')['tasks'] if t['id'] == 'stale-task'), {})
                        if restored.get('status') == 'interrupted': break
                    except OSError: pass
                    time.sleep(.05)
                assert restored['status'] == 'interrupted', restored
                print('PASS: restart marks orphaned tasks interrupted and preserves saved results', flush=True)
                resumed=api('/api/subagents/'+failed['id']+'/resume',{})
                assert resumed['group']!=failed['group'] and resumed['resumedFrom']==failed['id']
                assert wait_task(resumed['id'])['status']=='completed'
                print('PASS: cross-root resume reuses saved partial context after host restart',flush=True)
                if len(sys.argv)>2:
                    report=Path(sys.argv[2]);report.parent.mkdir(parents=True,exist_ok=True);report.write_text(json.dumps({'provider':'local-fixture','scenarios':benchmark},indent=2),encoding='utf-8')
            finally:
                host.terminate()
                host.wait(10)
                time.sleep(.5)
                model_server.shutdown()


if __name__ == '__main__':
    check(sys.argv[1])
