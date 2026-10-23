"""Isolated real TCP tests for 1–10 devices, configuration API and stable history."""
from pathlib import Path
import concurrent.futures, ftplib, io, json, os, socket, subprocess, tempfile, time, urllib.request, urllib.error

PROJECT=Path(__file__).resolve().parents[1]
EXE=Path(os.environ.get('CT_RUST_EXE',str(PROJECT/'target/release/chingtech-monitor.exe')))
URL='http://127.0.0.1:28081'
NG=b'<camera><image><symbol><result>NoRead</result></symbol></image></camera>'
OK=b'<camera><image><symbol><result>TEST123</result></symbol></image></camera>'

def get(path):
    return json.load(urllib.request.urlopen(URL+path,timeout=5))
def post(value,origin=None,kind='application/json',raw=None):
    headers={'Content-Type':kind}
    if origin:headers['Origin']=origin
    request=urllib.request.Request(URL+'/api/config',data=raw if raw is not None else json.dumps(value).encode(),headers=headers,method='POST')
    try:
        with urllib.request.urlopen(request,timeout=5) as r:return r.status,json.load(r)
    except urllib.error.HTTPError as e:return e.code,json.load(e)
def start(root):
    process=subprocess.Popen([str(EXE),'--headless','--data-dir',str(root)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    for _ in range(80):
        if process.poll() is not None:raise RuntimeError(process.stderr.read().decode(errors='replace'))
        try:get('/api/status');return process
        except OSError:time.sleep(.1)
    process.terminate();process.wait();raise AssertionError('Startup timeout')
def upload(ip,port,content=OK):
    with ftplib.FTP(source_address=(ip,0)) as ftp:
        ftp.connect('127.0.0.1',port,timeout=5);ftp.login('anonymous','anonymous')
        ftp.storbinary('STOR same.jpg',io.BytesIO(ip.encode()))
        ftp.storbinary('STOR same.xml',io.BytesIO(content))
def rejected(ip,port):
    try:
        with ftplib.FTP(source_address=(ip,0)) as ftp:
            ftp.connect('127.0.0.1',port,timeout=2);ftp.login('anonymous','anonymous')
    except (OSError,EOFError,ftplib.Error):return
    raise AssertionError('Unauthorized source/port accepted')

with tempfile.TemporaryDirectory(prefix='chingtech-settings-') as folder:
    root=Path(folder)
    config={'ftp_host':'127.0.0.1','ftp_port':22221,'web_port':28081,'max_photos':3,'cameras':[{'ip':'127.0.0.1','label':'單台'}]}
    (root/'config.json').write_text(json.dumps(config),encoding='utf-8')
    p=start(root)
    try:
        assert len(get('/api/status')['cameras'])==1
        single=get('/api/config');assert single['cameras'][0]['id']=='127.0.0.1'
        upload('127.0.0.1',22221,NG)
        legacy_ng=get('/api/status')['cameras'][0]['ng'];assert legacy_ng
        # Requests from another website must never change persistent settings.
        before=(root/'config.json').read_bytes()
        for value,origin,kind,raw in [(single,'https://example.org','application/json',None),(single,None,'text/plain',None),(single,None,'application/json',b'not json')]:
            assert post(value,origin,kind,raw)[0]==400
            assert (root/'config.json').read_bytes()==before
        for cameras in ([],[single['cameras'][0]]*11,[single['cameras'][0]]*2):
            assert post({**single,'cameras':cameras})[0]==400
        assert post({**single,'cameras':[{**single['cameras'][0],'ftp_port':30000}]})[0]==400
        with socket.socket() as occupied:
            occupied.bind(('127.0.0.1',22224));occupied.listen()
            assert post({**single,'cameras':[{**single['cameras'][0],'ftp_port':22224}]})[0]==400
        ten={**single,'cameras':[single['cameras'][0]]+[
            {'id':f'unit-{i}','ip':('127.0.1.1' if i==2 else f'127.0.0.{i}'),'label':f'設備 {i}','ftp_port':22222 if i==2 else None,'enabled':True}
            for i in range(2,11)]}
        assert post(ten)[0]==200
        assert (root/'config.previous.json').read_bytes()==before
        assert get('/api/status')['restart_required']
        assert len(get('/api/status')['cameras'])==1 # Explicit restart semantics.
        p.terminate();p.wait(timeout=10);p=start(root)
        assert len(get('/api/status')['cameras'])==10
        assert not get('/api/status')['restart_required']
        with concurrent.futures.ThreadPoolExecutor(10) as pool:
            list(pool.map(lambda c: upload(c['ip'],c.get('ftp_port') or ten['ftp_port']),ten['cameras']))
        state=get('/api/status');assert all(c['latest']['codes']==['TEST123'] for c in state['cameras'])
        assert len({c['latest']['file'] for c in state['cameras']})==10
        for c in state['cameras']:
            assert urllib.request.urlopen(URL+'/'+c['latest']['file']).read()==c['ip'].encode()
        rejected('127.0.1.1',22221);rejected('127.0.0.1',22222)
        ten['cameras'][0]['ip']='127.0.0.11';ten['cameras'][0]['label']='已換 IP';ten['cameras'][0]['ftp_port']=22223
        ten['cameras'][2]['enabled']=False
        assert post(ten)[0]==200
        p.terminate();p.wait(timeout=10);p=start(root)
        state=get('/api/status');assert len(state['cameras'])==9
        migrated=state['cameras'][0];assert migrated['ng']==legacy_ng and migrated['count']==2
        upload('127.0.0.11',22223)
        assert (root/'photos/127.0.0.1').exists() and not (root/'photos/127.0.0.11').exists()
        rejected('127.0.0.3',22221);rejected('127.0.0.11',22221)
        disabled={**ten,'cameras':[{**c,'enabled':False} for c in ten['cameras']]}
        assert post(disabled)[0]==200
        p.terminate();p.wait(timeout=10);p=start(root)
        assert get('/api/status')['cameras']==[]
        assert len(get('/api/config')['cameras'])==10
        with socket.socket() as probe:assert probe.connect_ex(('127.0.0.1',22221))!=0
        print('PASS: legacy single device; ten simultaneous devices; shared/separate FTP ports; same-octet cache isolation; source-port authorization; validation; occupied-port rejection; origin protection; previous config backup; restart-required behavior; stable history after IP change; disabled/all-disabled devices')
    finally:
        p.terminate();p.wait(timeout=10)
