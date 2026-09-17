"""Opt-in real-provider smoke benchmark. Only disposable fixture data is sent.
Usage: python scripts/check-real-provider.py EXE --run --output report.json
Reads the active local shell provider. Never prints or copies API keys to disk.
"""
import argparse, collections, http.client, http.server, json, os, socket, subprocess, tempfile, threading, time
from pathlib import Path

def main():
    parser=argparse.ArgumentParser();parser.add_argument('executable');parser.add_argument('--run',action='store_true');parser.add_argument('--output',required=True);parser.add_argument('--case',action='append');args=parser.parse_args()
    if not args.run: parser.error('--run is required: real model requests consume usage')
    settings=json.loads(Path('.opencowork/settings.json').read_text(encoding='utf-8-sig'));shell=settings.get('shell',{});profiles=shell.get('providerProfiles',[])
    profile=next((p for p in profiles if p['id']==shell.get('activeProviderProfileId')),None)
    if not profile: raise SystemExit('Configure and select a provider profile first')
    credential=profile.get('apiKey') or os.environ.get(profile.get('apiKeyEnv','OPENAI_API_KEY'),'')
    report={'model':profile['model'],'scope':'real-provider disposable smoke benchmark, not a broad comparative evaluation','results':[]}
    class Page(http.server.BaseHTTPRequestHandler):
        def log_message(self,*args): pass
        def do_GET(self):
            body=b'''<title>Owned benchmark</title><label>Name <input aria-label="Name" id="name"></label><button onclick="document.getElementById('result').textContent='Hello '+document.getElementById('name').value">Submit</button><p id="result">Waiting</p>'''
            self.send_response(200);self.send_header('Content-Type','text/html');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    page=http.server.ThreadingHTTPServer(('127.0.0.1',0),Page);threading.Thread(target=page.serve_forever,daemon=True).start()
    with tempfile.TemporaryDirectory(prefix='cowork-real-benchmark-') as temp:
        root=Path(temp);project=root/'project';config=root/'config';(project/'.opencowork').mkdir(parents=True);config.mkdir()
        provider={'name':profile.get('name','Provider'),'baseUrl':profile['baseUrl'],'apiKeyEnv':'COWORK_BENCH_KEY','timeoutMs':max(30000, min(int(profile.get('timeoutMs',90000)),120000))}
        config_data={'model':profile['model'],'provider':provider,'permissionMode':'workspace-write','teamMemorySync':{'enabled':False},'memory':{'enabled':False},'learning':{'enabled':False},'execution':{'maxIterations':8,'maxTokens':60000,'maxSeconds':240,'maxOutputTokens':1024,'repeatedResults':3},'mcpServers':{}}
        (project/'.opencowork/settings.json').write_text(json.dumps(config_data),encoding='utf-8')
        (project/'numbers.txt').write_text('17\n25\n',encoding='utf-8');(project/'notes.txt').write_text('before\nkeep this line\n',encoding='utf-8')
        with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
        env={k:v for k,v in os.environ.items() if not k.startswith('OPENCOWORK_')};env.update(OPENCOWORK_CONFIG_HOME=str(config),OPENCOWORK_SHELL_PORT=str(port),COWORK_BENCH_KEY=credential,NO_PROXY='127.0.0.1,localhost',no_proxy='127.0.0.1,localhost')
        def api(path,data=None):
            c=http.client.HTTPConnection('127.0.0.1',port,timeout=300);c.request('POST' if data is not None else 'GET',path,json.dumps(data) if data is not None else None,{'Content-Type':'application/json'});r=c.getresponse();body=r.read();c.close();assert r.status in (200,204),(path,r.status);return body
        with (root/'host.log').open('w') as log:
            process=subprocess.Popen([str(Path(args.executable).resolve())],cwd=project,env=env,stdout=log,stderr=log,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
            try:
                for _ in range(200):
                    try:api('/api/health');break
                    except OSError:time.sleep(.1)
                cases=[('read-and-write','Read numbers.txt using a file tool. Add the two integers and write only the sum to sum.txt using a file tool. Then report the result.',lambda text:(project/'sum.txt').exists() and (project/'sum.txt').read_text().strip()=='42'),('edit-and-verify','Change only the first line of notes.txt from before to after, preserving the second line. Use tools and read back the result before reporting completion.',lambda text:(project/'notes.txt').read_text().replace('\r\n','\n')=='after\nkeep this line\n'),('browser-form',f'Use the Browser tool to open http://127.0.0.1:{page.server_port}/, fill Name with Ada, click Submit, then inspect the page and report its result. Use ToolSearch if needed. Do not use bash or HTTP tools.',lambda text:'Hello Ada' in text)]
                for name,prompt,check in cases:
                    if args.case and name not in args.case: continue
                    config_data['permissionMode'] = 'danger-full-access' if name == 'browser-form' else 'workspace-write'
                    (project/'.opencowork/settings.json').write_text(json.dumps(config_data),encoding='utf-8')
                    started=time.monotonic();raw=api('/api/chat',{'input':prompt});events=[json.loads(line) for line in raw.splitlines() if line.strip()];result=next((e['response'] for e in events if e['type']=='complete'),{});text='\n'.join(e.get('output','') for e in result.get('events',[]) if e['type']=='tool_result' and not e.get('is_error'));calls=[e for e in result.get('events',[]) if e['type']=='tool_call'];usage=[e['usage'] for e in result.get('events',[]) if e['type']=='usage'];counts=collections.Counter((e['name'],json.dumps(e.get('input'),sort_keys=True)) for e in calls)
                    item={'case':name,'passed':bool(result.get('status')=='completed' and check(text)),'elapsedMs':round((time.monotonic()-started)*1000),'toolCalls':len(calls),'repeatedIdenticalCalls':sum(max(0,n-1) for n in counts.values()),'interventions':0,'foregroundUsage':usage,'backgroundRequests':0,'status':result.get('status','error'),'error':result.get('error'),'evidence':[e for e in result.get('events',[]) if e['type'] in ('tool_call','tool_result','assistant_text_delta')]};report['results'].append(item);print(json.dumps({k:v for k,v in item.items() if k != 'evidence'},ensure_ascii=False),flush=True)
            finally:process.terminate();process.wait(timeout=15)
    page.shutdown();Path(args.output).write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
    if not all(r['passed'] for r in report['results']):raise SystemExit(1)
if __name__=='__main__':main()
