"""Readiness and sync-settings regression; disposable host, no external requests."""
import http.client, json, os, socket, subprocess, sys, tempfile, time
from pathlib import Path

def run(executable):
    with tempfile.TemporaryDirectory(prefix='cowork-setup-') as temp:
        root=Path(temp);project=root/'project';config=root/'config';(project/'.opencowork').mkdir(parents=True);config.mkdir()
        settings={'model':'fixture','provider':{'name':'Fixture','apiKeyEnv':'COWORK_SETUP_KEY','baseUrl':'http://127.0.0.1:9/v1'},'teamMemorySync':{'enabled':False},'mcpServers':{}}
        (project/'.opencowork/settings.json').write_text(json.dumps(settings),encoding='utf-8')
        with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
        env={k:v for k,v in os.environ.items() if not k.startswith('OPENCOWORK_')};env.update(OPENCOWORK_CONFIG_HOME=str(config),OPENCOWORK_SHELL_PORT=str(port),COWORK_SETUP_KEY='fixture-secret-not-for-response')
        def api(path,data=None,status=200):
            c=http.client.HTTPConnection('127.0.0.1',port,timeout=10);c.request('POST' if data is not None else 'GET',path,json.dumps(data) if data is not None else None,{'Content-Type':'application/json'});r=c.getresponse();body=r.read();c.close();assert r.status==status,(path,r.status,body);assert b'fixture-secret-not-for-response' not in body;return json.loads(body) if body else None
        with (root/'host.log').open('w') as log:
            process=subprocess.Popen([executable],cwd=project,env=env,stdout=log,stderr=log,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
            try:
                for _ in range(150):
                    try:api('/api/health',status=204);break
                    except OSError:time.sleep(.1)
                ready=api('/api/setup-status');assert ready['credentialPresent'] and ready['model']=='fixture' and ready['mcpCount']==0
                assert not api('/api/team-memory-settings')['enabled']
                valid={'enabled':True,'endpoint':'http://127.0.0.1:9/sync','repo':'fixture/repo','tokenEnv':'COWORK_MISSING_SYNC_TEST_KEY'}
                api('/api/team-memory-settings',valid);assert api('/api/team-memory-settings')['repo']=='fixture/repo'
                for bad in [{**valid,'endpoint':'not-a-url'},{**valid,'endpoint':'https://example.test/#fragment'},{**valid,'tokenFile':'relative.txt','tokenEnv':''},{**valid,'tokenFile':str(root/'token.txt')},{**valid,'repo':'x'*2049}]:api('/api/team-memory-settings',bad,status=400)
                assert api('/api/team-memory-settings')['repo']=='fixture/repo'
                failure=api('/api/team-memory-sync',status=500);assert 'environment variable' in failure['error'] and 'missing' in failure['error']
                api('/api/team-memory-settings',{**valid,'enabled':False,'tokenEnv':''})
                assert api('/api/team-memory-conflicts')=={'conflicts':[]}
                api('/api/team-memory-conflicts',{'id':'../outside','choice':'remote'},status=400)
                print('PASS: setup presence without secret exposure; sync settings validation/persistence; missing auth; empty conflicts and invalid resolution')
            finally:process.terminate();process.wait(timeout=15)
if __name__=='__main__':run(str(Path(sys.argv[1]).resolve()))
