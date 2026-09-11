"""Isolated P1/P2 and computer-control integration checks; no external model calls.
Usage: python scripts/check-features.py path/to/opencowork-shell.exe
"""
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
    screenshots = []
    extraction = []
    seen = []
    class Model(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            seen.append(body)
            if not body.get('stream'):
                extraction.append(body)
                assert body['max_tokens'] == 768
                time.sleep(1.5)
                result = {'choices': [{'message': {'content': json.dumps({'notes': [{'title': 'Test project convention', 'fact': 'This fixture uses Rust for its runtime.' if len(extraction)==1 else 'This fixture uses Rust 2024 for its runtime.'}]})}}]}
                data = json.dumps(result).encode()
                self.send_response(200); self.send_header('Content-Type','application/json'); self.send_header('Content-Length', str(len(data))); self.end_headers(); self.wfile.write(data)
                return
            user = '\n'.join(m['content'] for m in body['messages'] if m['role']=='user' and isinstance(m.get('content'),str))
            results = [m for m in body['messages'] if m['role']=='tool']
            tool = None
            if 'computer-snapshot' in user and not results: tool=('Computer',{'action':'snapshot'})
            if 'computer-screenshot' in user and not results: tool=('Computer',{'action':'screenshot'})
            if 'computer-stale-image' in user:
                if not results: tool=('Computer',{'action':'screenshot'})
                elif len(results)==1: tool=('Computer',{'action':'snapshot'})
                else: assert not any(c.get('type')=='image_url' for m in body['messages'] if isinstance(m.get('content'),list) for c in m['content']), 'Stale screenshot survived a newer observation'
            if 'plan-check' in user and not results: tool=('UpdatePlan',{'goal':'Verify the fixture','steps':[{'title':'Review file','status':'completed','check':'Read fixture contents','evidence':'fixture.txt contains original'}]})
            if 'plan-pending' in user and not results: tool=('UpdatePlan',{'goal':'Continue later','steps':[{'title':'Review file','status':'in_progress','check':'Read fixture contents'}]})
            if 'computer-screenshot' in user and results:
                images = [c for m in body['messages'] if isinstance(m.get('content'),list) for c in m['content'] if c.get('type')=='image_url']
                screenshots.extend(images)
            if results:
                assert 'Computer action failed' not in results[-1]['content'], results[-1]['content']
            delta = {'tool_calls':[{'index':0,'id':'call-fixture','type':'function','function':{'name':tool[0],'arguments':json.dumps(tool[1])}}]} if tool else {'content':'Verified fixture. This fixture uses Rust for its runtime.'}
            events = [ {'choices':[{'index':0,'delta':delta,'finish_reason':None}]}, {'choices':[{'index':0,'delta':{},'finish_reason':'tool_calls' if tool else 'stop'}]} ]
            data = ''.join('data: '+json.dumps(e)+'\n\n' for e in events).encode()+b'data: [DONE]\n\n'
            self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.send_header('Content-Length',str(len(data))); self.end_headers(); self.wfile.write(data)
        def log_message(self,*args): pass
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix='cowork-features-') as tmp:
            root=Path(tmp); project=root/'project'; config=root/'config'; (project/'.opencowork').mkdir(parents=True); config.mkdir()
            def git(*args): return subprocess.run(['git','-c',f'safe.directory={project}',*args],cwd=project,check=True,capture_output=True)
            git('init'); git('config','user.email','fixture@example.invalid'); git('config','user.name','Fixture')
            (project/'fixture.txt').write_text('original\n',encoding='utf-8'); git('add','fixture.txt'); git('commit','-m','fixture')
            (project/'fixture.txt').write_text('changed 中文\n',encoding='utf-8')
            (project/'large.txt').write_text('界'*30000,encoding='utf-8')
            settings={'model':'fixture','permissionMode':'danger-full-access','provider':{'name':'Fixture','apiKeyEnv':'FEATURE_TEST_KEY','baseUrl':f'http://127.0.0.1:{server.server_port}/v1','timeoutMs':30000},'teamMemorySync':{'enabled':False}}
            (project/'.opencowork/settings.json').write_text(json.dumps(settings))
            with socket.socket() as listener: listener.bind(('127.0.0.1',0)); port=listener.getsockname()[1]
            env={k:v for k,v in os.environ.items() if not k.startswith('OPENCOWORK_')}
            env.update(OPENCOWORK_CONFIG_HOME=str(config),OPENCOWORK_SHELL_PORT=str(port),FEATURE_TEST_KEY='fixture-only',NO_PROXY='127.0.0.1,localhost',no_proxy='127.0.0.1,localhost')
            def api(path,payload=None,status=200):
                conn=http.client.HTTPConnection('127.0.0.1',port,timeout=45)
                conn.request('POST' if payload is not None else 'GET',path,json.dumps(payload) if payload is not None else None,{'Content-Type':'application/json'})
                response=conn.getresponse(); data=response.read(); assert response.status==status,(path,response.status,data);conn.close()
                return json.loads(data) if data else None
            def chat(prompt,refs=None):
                conn=http.client.HTTPConnection('127.0.0.1',port,timeout=45)
                conn.request('POST','/api/chat',json.dumps({'input':prompt,'references':refs or []}),{'Content-Type':'application/json'})
                response=conn.getresponse(); assert response.status==200,response.read()
                messages=[json.loads(line) for line in response if line.strip()];conn.close()
                final=next(m['response'] for m in messages if m['type']=='complete')
                assert final['status']=='completed',final
                for event in final['events']:
                    if event['type']=='tool_result': assert not event['is_error'],event
                return final
            log=(root/'shell.log').open('w')
            process=subprocess.Popen([executable],cwd=project,env=env,stdout=log,stderr=log,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
            try:
                deadline=time.monotonic()+20
                while True:
                    try: api('/api/health',status=204);break
                    except (OSError,AssertionError):
                        if time.monotonic()>deadline: raise
                        time.sleep(.1)
                assert api('/api/features')['computerEnabled'] is True
                listing=api('/api/workspace'); assert 'fixture.txt' in listing['files']; assert any(c['path']=='fixture.txt' for c in listing['changes'])
                assert 'changed' in api('/api/workspace/diff?path=fixture.txt')['content']
                assert '中文' in api('/api/workspace/file?path=fixture.txt')['content']
                api('/api/workspace/file?path=../outside',status=400)
                preview=api('/api/references/preview',[{'kind':'file','path':'large.txt'}]); assert preview['references'][0]['truncated'];assert preview['usedCharacters']==12000
                api('/api/references/preview',[{'kind':'file','path':'fixture.txt'}]*9,status=400)
                result=chat('reference-check',[{'kind':'file','path':'fixture.txt'}]); sid=result['sessionId']
                assert any('changed 中文' in str(m['content']) for m in seen[-1]['messages'])
                context=api('/api/context-diagnostics/'+sid);assert context['attachments']['references'][0]['path']=='fixture.txt';assert context['layers']
                preview=api('/api/references/preview',[{'kind':'session','path':sid}]);assert preview['usedCharacters']>0
                done=chat('plan-check');assert api('/api/task-plan/'+done['sessionId'])['steps'][0]['status']=='completed'
                pending=chat('plan-pending');assert api('/api/task-plan/'+pending['sessionId'])['steps'][0]['status']=='interrupted'
                if os.name=='nt':
                    chat('computer-snapshot');capture=chat('computer-screenshot');assert screenshots and screenshots[0]['image_url']['url'].startswith('data:image/jpeg;base64,')
                    image_result=next(json.loads(e['output']) for e in capture['events'] if e['type']=='tool_result' and e['tool_name']=='Computer')
                    conn=http.client.HTTPConnection('127.0.0.1',port,timeout=10)
                    conn.request('GET','/api/computer/screenshots/'+Path(image_result['screenshotPath']).name)
                    response=conn.getresponse();assert response.status==200 and response.getheader('Content-Type')=='image/jpeg';assert response.read().startswith(b'\xff\xd8');conn.close()
                    api('/api/computer/screenshots/..%5Csettings.json',status=400)
                    chat('computer-stale-image')
                api('/api/features',{'computerEnabled':False,'backgroundMemoryEnabled':False})
                assert not api('/api/features')['computerEnabled']
                manifest=api('/api/tool-manifest');assert '"name": "Computer"' not in json.dumps(manifest)
                api('/api/features',{'computerEnabled':True,'backgroundMemoryEnabled':True})
                before=time.monotonic();chat('memory-check');elapsed=time.monotonic()-before
                deadline=time.monotonic()+15
                while time.monotonic()<deadline:
                    memory_status=api('/api/background-memory')
                    if memory_status['state'] in ('completed','failed'):break
                    time.sleep(.2)
                assert memory_status['state']=='completed',memory_status
                assert extraction and list(config.rglob('auto-*.md'))
                assert elapsed<1.5,('Background extraction delayed main response',elapsed)
                provenance=api('/api/memory-provenance');assert provenance['sources'] and provenance['sources'][0]['sourceSession']
                note=Path(provenance['sources'][0]['path']);assert 'source_session:' in note.read_text(encoding='utf-8')
                for p in config.rglob('.background/status.json'):p.unlink()
                chat('memory-conflict-check');deadline=time.monotonic()+15
                while time.monotonic()<deadline:
                    provenance=api('/api/memory-provenance')
                    if provenance['conflicts']:break
                    time.sleep(.2)
                assert provenance['conflicts'],provenance
                conflict=provenance['conflicts'][0];assert conflict['status']=='needs_review';assert 'Rust 2024' not in note.read_text(encoding='utf-8')
                api('/api/memory-provenance',{'id':conflict['id'],'action':'replace'});assert 'Rust 2024' in note.read_text(encoding='utf-8')
                print('PASS: memory provenance, conflict staging and explicit replacement')

                print('PASS: workspace diffs, path containment, reference/session budgets, diagnostics, resumable plans, computer snapshot/image input, feature switch, independent budgeted memory extraction')
            finally:
                process.terminate();process.wait(timeout=10);log.close()
    finally: server.shutdown();server.server_close()

if __name__=='__main__': check(str(Path(sys.argv[1]).resolve()))
