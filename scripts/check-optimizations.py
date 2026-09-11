"""Six-area regression suite. Disposable projects and loopback mock model only."""
import http.client, http.server, json, os, socket, subprocess, sys, tempfile, threading, time
from pathlib import Path
from urllib.parse import quote

def run(executable):
    metrics=[]; requests=[]; failures=[]; gate=threading.Event()
    with tempfile.TemporaryDirectory(prefix='cowork-optimizations-') as temporary:
        root=Path(temporary); project=root/'project'; config=root/'config'; (project/'.opencowork').mkdir(parents=True);config.mkdir()
        class Model(http.server.BaseHTTPRequestHandler):
            def log_message(self,*args): pass
            def do_GET(self):
                body=('''<!doctype html><meta charset="utf-8"><title>Owned fixture</title><label>Name <input aria-label="Name" id="name"></label><button onclick="document.querySelector('#status').textContent='Submitted '+document.querySelector('#name').value; console.log('fixture-submitted');fetch('/ping')">Submit</button><p id="status">Waiting</p><iframe title="Inner fixture" src="/frame"></iframe>''' if self.path=='/' else '<!doctype html><p>Inner frame verified</p>').encode()
                self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
            def do_POST(self):
                try: self.model()
                except (BrokenPipeError,ConnectionResetError,ConnectionAbortedError): pass
                except Exception as e:
                    failures.append(repr(e)); self.send_error(500,str(e))
            def model(self):
                b=json.loads(self.rfile.read(int(self.headers['Content-Length'])));requests.append(b)
                messages=b['messages'];user='\n'.join(m['content'] for m in messages if m['role']=='user' and isinstance(m.get('content'),str));results=[m for m in messages if m['role']=='tool'];n=len(results);tool=None;text='Verified 中文深层事实：桌面自动化完成。'
                if not b.get('stream'):
                    system=' '.join(m.get('content','') for m in messages if m['role']=='system')
                    if 'Independently check' in system:
                        data=json.loads(messages[-1]['content']);text=json.dumps({'completed':True,'reason':'The fixture verification returned success','evidenceIds':[e['id'] for e in data['evidence']]})
                    elif 'propose at most one reusable' in system:
                        data=json.loads(messages[-1]['content']);text=json.dumps({'title':'Fixture verification','description':'When checking a fixture','steps':'Read the fixture and run its check.','checks':'Verify fixture output.','fallback':'Reobserve on error.','evidenceIds':[e['id'] for e in data['evidence'][:2]]})
                    elif 'Extract at most 3' in system:text=json.dumps({'notes':[{'title':'Fixture fact','fact':'The fixture uses Rust.'}]})
                    elif any(m.get('content') and isinstance(m['content'],list) for m in messages):text='red'
                    elif b.get('tools'):tool=('probe_stamp',{'value':7})
                    else:text='OK'
                    message={'content':text} if not tool else {'tool_calls':[{'id':'probe','type':'function','function':{'name':tool[0],'arguments':json.dumps(tool[1])}}]}
                    data=json.dumps({'choices':[{'message':message}],'usage':{'prompt_tokens':1000,'completion_tokens':100,'prompt_tokens_details':{'cached_tokens':600}}}).encode()
                    self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data);return
                if 'root-escape' in user and n==0:tool=('write_file',{'path':str(root/'outside.txt'),'content':'must not write'})
                elif 'ambiguous-edit' in user and n==0:tool=('edit_file',{'path':'ambiguous.txt','old':'same','new':'other'})
                elif 'loop-check' in user:tool=('read_file',{'path':'fixture.txt'})
                elif 'goal-check' in user:
                    seq=[('UpdatePlan',{'goal':'goal-check','steps':[{'title':'Check fixture','status':'completed','check':'Tool reports original','evidence':'Actual tool verification follows'}]}),('read_file',{'path':'fixture.txt'}),('bash',{'command':'Write-Output original','timeoutMs':5000})]
                    if n<len(seq):tool=seq[n]
                elif 'browser-check' in user:
                    if n==0:tool=('Browser',{'action':'open','url':f'http://127.0.0.1:{server.server_port}/'})
                    else:
                        previous={} if n==8 else json.loads(results[-1]['content']);first=json.loads(results[0]['content']);tab=first['tabId'];state=previous.get('stateId')
                        def ref(name):return next(e['index'] for e in previous['elements'] if e.get('name')==name)
                        if n==1:tool=('Browser',{'action':'fill','tabId':tab,'stateId':state,'elementIndex':ref('Name'),'text':'中文 Ada'})
                        elif n==2:tool=('Browser',{'action':'click','tabId':tab,'stateId':state,'elementIndex':ref('Submit')})
                        elif n==3:
                            assert 'Submitted 中文 Ada' in json.dumps(previous,ensure_ascii=False),previous
                            tool=('Browser',{'action':'resize','tabId':tab,'width':390,'height':844})
                        elif n==4:tool=('Browser',{'action':'diagnostics','tabId':tab})
                        elif n==5:
                            assert any('consoleAPICalled' in e['method'] for e in previous['events']),previous
                            tool=('Browser',{'action':'snapshot','tabId':tab})
                        elif n==6:
                            frames=previous['frames'];frame=frames['frameTree']['childFrames'][0]['frame']['id'] if 'frameTree' in frames else frames['childFrames'][0]['frame']['id']
                            tool=('Browser',{'action':'snapshot','tabId':tab,'frameId':frame})
                        elif n==7:
                            assert 'Inner frame verified' in json.dumps(previous),previous
                            tool=('Browser',{'action':'click','tabId':tab,'stateId':first['stateId'],'elementIndex':0})
                        elif n==8:assert 'stale_browser_state' in results[-1]['content']
                elif 'computer-warm' in user and n<3:tool=('Computer',{'action':'list_windows'})
                elif 'crash-check' in user:
                    if n==0:tool=('bash',{'command':"Write-Output durable-before-crash",'timeoutMs':5000})
                    else:gate.wait(30)
                elif 'steering-check' in user and n==0:
                    gate.wait(10);tool=('read_file',{'path':'fixture.txt'})
                delta={'tool_calls':[{'index':0,'id':f'call-{n}','type':'function','function':{'name':tool[0],'arguments':json.dumps(tool[1])}}]} if tool else {'content':text}
                chunks=[{'choices':[{'delta':delta}]},{'choices':[],'usage':{'prompt_tokens':1000,'completion_tokens':100,'prompt_tokens_details':{'cached_tokens':600}}}]
                data=''.join('data: '+json.dumps(c)+'\n\n' for c in chunks).encode()+b'data: [DONE]\n\n'
                self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);threading.Thread(target=server.serve_forever,daemon=True).start()
        def git(*args):return subprocess.run(['git','-c',f'safe.directory={project}',*args],cwd=project,check=True,capture_output=True)
        git('init');git('config','user.name','Fixture');git('config','user.email','fixture@example.invalid')
        (project/'fixture.txt').write_text('original\n',encoding='utf-8');(project/'ambiguous.txt').write_text('same same',encoding='utf-8');git('add','.');git('commit','-m','fixture')
        settings={'model':'fixture','permissionMode':'workspace-write','provider':{'name':'Fixture','apiKeyEnv':'TEST_OPTIMIZATION_KEY','baseUrl':f'http://127.0.0.1:{server.server_port}/v1','timeoutMs':30000},'teamMemorySync':{'enabled':False},'execution':{'maxIterations':40,'maxTokens':250000,'maxSeconds':120,'repeatedResults':3}}
        setting_path=project/'.opencowork/settings.json';setting_path.write_text(json.dumps(settings),encoding='utf-8')
        with socket.socket() as listener:listener.bind(('127.0.0.1',0));port=listener.getsockname()[1]
        env={k:v for k,v in os.environ.items() if not k.startswith('OPENCOWORK_')};env.update(OPENCOWORK_CONFIG_HOME=str(config),OPENCOWORK_SHELL_PORT=str(port),TEST_OPTIMIZATION_KEY='fixture-only',NO_PROXY='127.0.0.1,localhost',no_proxy='127.0.0.1,localhost')
        log=(root/'host.log').open('w',encoding='utf-8');process=None
        def api(path,data=None,code=200,method=None):
            c=http.client.HTTPConnection('127.0.0.1',port,timeout=120);c.request(method or ('POST' if data is not None else 'GET'),path,json.dumps(data) if data is not None else None,{'Content-Type':'application/json'});r=c.getresponse();body=r.read();c.close();assert r.status==code,(path,r.status,body);return json.loads(body) if body else None
        def start():
            p=subprocess.Popen([executable],cwd=project,env=env,stdout=log,stderr=log,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0));deadline=time.monotonic()+20
            while True:
                try:api('/api/health',code=204);return p
                except (OSError,AssertionError):
                    if time.monotonic()>deadline:raise
                    time.sleep(.1)
        def chat(text,**extra):
            c=http.client.HTTPConnection('127.0.0.1',port,timeout=120);c.request('POST','/api/chat',json.dumps({'input':text,**extra}),{'Content-Type':'application/json'});r=c.getresponse();assert r.status==200,r.read();events=[json.loads(line) for line in r if line.strip()];c.close();return next(e['response'] for e in events if e['type']=='complete')
        def passed(name):metrics.append({'case':name,'status':'passed','model':'loopback-fixture'});print('PASS:',name,flush=True)
        try:
            process=start()
            r=chat('root-escape');assert not (root/'outside.txt').exists();assert 'workspace_write_denied' in str(r);passed('workspace write containment')
            r=chat('cache-check');usage=next(e['usage'] for e in r['events'] if e['type']=='usage');assert usage['input_tokens']==400 and usage['cache_read_input_tokens']==600 and usage['output_tokens']==100,usage;sid=r['sessionId'];passed('cache token buckets and totals')
            r=chat('ambiguous-edit');assert (project/'ambiguous.txt').read_text()=='same same';assert any(e['is_error'] for e in r['events'] if e['type']=='tool_result');passed('ambiguous edit rejected')
            r=chat('loop-check');assert r['status']=='failed' and 'no_progress' in r['error'],r;passed('repeated result stop')
            found=api('/api/history-search?q='+quote('中文深层事实'));assert any(x['sessionId']==sid for x in found['results']);api('/api/sessions/'+sid,code=200,method='DELETE');assert not any(x['sessionId']==sid for x in api('/api/history-search?q='+quote('中文深层事实'))['results']);passed('Chinese full text and deletion propagation')
            (project/'fixture.txt').write_text('changed\n',encoding='utf-8');rev=api('/api/review?path=fixture.txt');api('/api/review',{'path':'fixture.txt','version':rev['version'],'action':'stage','hunk':0});assert b'changed' in git('diff','--cached').stdout
            rev=api('/api/review?path=fixture.txt');api('/api/review',{'path':'fixture.txt','version':rev['version'],'action':'unstage','hunk':0});rev=api('/api/review?path=fixture.txt');snap=api('/api/review',{'path':'fixture.txt','version':rev['version'],'action':'revert','hunk':0});assert (project/'fixture.txt').read_text()=='original\n';api('/api/review',{'action':'restore','snapshotId':snap['snapshotId']});assert (project/'fixture.txt').read_text()=='changed\n'
            rev=api('/api/review?path=fixture.txt');(project/'fixture.txt').write_text('user-new\n');api('/api/review',{'path':'fixture.txt','version':rev['version'],'action':'revert'},400);(project/'fixture.txt').write_text('original\n');passed('hunk stage unstage revert restore and stale rejection')
            tree=api('/api/worktrees',{'name':'isolated-fixture'});assert Path(tree['path']).is_dir() and Path(tree['path'])!=project;passed('isolated Git worktree')
            settings['permissionMode']='danger-full-access';setting_path.write_text(json.dumps(settings),encoding='utf-8')
            r=chat('goal-check',goal='goal-check');g=api('/api/workflows/'+r['sessionId'])['goal'];assert g['status']=='completed' and g['checks'],g;passed('goal evidence verification')
            # Stable IDs suppress duplicate delivery across in-flight and queued input.
            initial=chat('queue-seed');queue_id=initial['sessionId'];gate.clear();queued=[]
            worker=threading.Thread(target=lambda:queued.append(chat('steering-check',sessionId=queue_id)),daemon=True);worker.start()
            deadline=time.monotonic()+10
            while time.monotonic()<deadline and not list((config/'turn-journals').glob(queue_id+'.json')):time.sleep(.05)
            for action,text,msgid in [('steer','extra-steering-marker','steer-fixture'),('queue','extra-queued-marker','queue-fixture')]:
                for _ in range(2):api('/api/workflows/'+queue_id,{'action':action,'text':text,'id':msgid})
            gate.set();worker.join(30);assert queued and queued[0]['status']=='completed',queued
            stored=json.loads((config/'sessions'/f'{queue_id}.json').read_text(encoding='utf-8'))
            for marker in ['extra-steering-marker','extra-queued-marker']:
                texts=[b.get('text') for m in stored['messages'] if m['role']=='user' for b in m['blocks']];assert texts.count(marker)==1,(marker,texts)
            assert all(m['status']=='delivered' for m in api('/api/workflows/'+queue_id)['messages']);passed('in-flight steering queued messages and stable IDs')
            settings['learning']={'enabled':True,'autoAdopt':False};setting_path.write_text(json.dumps(settings),encoding='utf-8');chat('goal-check')
            deadline=time.monotonic()+20;candidates=[]
            while time.monotonic()<deadline:
                learning=api('/api/learning');candidates=learning['candidates']
                if candidates:break
                time.sleep(.2)
            assert candidates,learning;c=candidates[0];assert c['status']=='pending' and c['evidenceIds']
            api('/api/learning',{'id':c['id'],'action':'adopt'});assert (project/'.opencowork/skills'/c['slug']/'SKILL.md').is_file()
            api('/api/learning',{'id':c['id'],'action':'disable'});assert not (project/'.opencowork/skills'/c['slug']/'SKILL.md').exists();passed('evidence-linked skill candidate adopt disable')
            settings['learning']['enabled']=False;settings['execution']['maxTokens']=500;setting_path.write_text(json.dumps(settings),encoding='utf-8')
            limited=chat('ambiguous-edit');assert limited['status']=='failed' and 'turn_budget_reached' in limited['error'];assert not any(e['type']=='tool_result' and not e['is_error'] for e in limited['events']);passed('token budget stops before tool execution')
            settings['execution']['maxTokens']=250000;setting_path.write_text(json.dumps(settings),encoding='utf-8')
            diagnostics=api('/api/provider-probe',{});assert all(c['status']=='verified' for c in diagnostics['checks']),diagnostics;passed('provider tools vision stream diagnostics')
            r=chat('browser-check');assert r['status']=='completed',(r,failures);assert len([e for e in r['events'] if e['type']=='tool_result'])==8,(r,failures);passed('owned browser form responsive iframe diagnostics stale refs')
            if os.name=='nt':
                r=chat('computer-warm');values=[json.loads(e['output'])['timing'] for e in r['events'] if e['type']=='tool_result'];assert len(values)==3 and values[0]['helperColdStart'] and not values[1]['helperColdStart'],r;metrics.append({'computerTiming':values});passed('persistent computer helper')
                api('/api/computer/takeover',{'paused':True});r=chat('computer-warm');assert 'computer_paused' in str(r);api('/api/computer/takeover',{'paused':False});passed('computer takeover')
            job=api('/api/schedules',{'text':'scheduled fixture','intervalSeconds':60,'timezone':'Asia/Shanghai','missedPolicy':'once'});p=next(config.rglob(job['id']+'.json'));v=json.loads(p.read_text());v['nextRunAt']=int(time.time())-1;p.write_text(json.dumps(v))
            deadline=time.monotonic()+25
            while time.monotonic()<deadline:
                inbox=api('/api/schedules')['inbox']
                if inbox:break
                time.sleep(.3)
            assert inbox and inbox[0]['status']=='completed',inbox;passed('scheduled execution and local inbox')
            # Crash after an observed tool result, while the following provider call waits.
            gate.clear();output=[]
            def interrupted_chat():
                try:output.append(chat('crash-check'))
                except (OSError,http.client.HTTPException):pass
            t=threading.Thread(target=interrupted_chat,daemon=True);t.start();deadline=time.monotonic()+20;journal=None
            while time.monotonic()<deadline:
                for p in (config/'turn-journals').glob('*.json'):
                    if 'durable-before-crash' in p.read_text(encoding='utf-8'):journal=p;break
                if journal:break
                time.sleep(.1)
            assert journal,'No durable tool checkpoint';recovered_id=journal.stem;process.kill();process.wait();gate.set();process=start();recovered=api('/api/sessions/'+recovered_id);assert 'durable-before-crash' in str(recovered);assert not journal.exists();passed('host crash recovery without action replay')
            assert not failures,failures
            print(json.dumps({'suite':'six-area-fixture','results':metrics},ensure_ascii=False),flush=True)
        finally:
            gate.set()
            if process and process.poll() is None:process.terminate();process.wait(timeout=15)
            log.close();server.shutdown();server.server_close()
if __name__=='__main__':run(str(Path(sys.argv[1]).resolve()))
