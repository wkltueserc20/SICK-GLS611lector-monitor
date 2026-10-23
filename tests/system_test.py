"""Exercise the native receiver with four real TCP FTP clients, no camera changes."""
from pathlib import Path
import concurrent.futures,ftplib,io,json,os,shutil,socket,subprocess,tempfile,time,urllib.request,sys

PROJECT=Path(__file__).resolve().parents[1]
EXE=Path(os.environ.get('CT_RUST_EXE',str(PROJECT/'target/release/chingtech-monitor.exe'))).resolve()
OK=b'<camera><image><symbol><result>001067</result></symbol></image></camera>'
NG=b'<camera><image><EVTimingAnalysis Name="DecoRun"/><DecoderProfiler><procd-img-trig><FramesDecoded>1</FramesDecoded></procd-img-trig></DecoderProfiler></image></camera>'

with tempfile.TemporaryDirectory(prefix='chingtech-rust-') as folder:
    root=Path(folder)
    config={'ftp_host':'127.0.0.1','ftp_port':22121,'web_port':28080,'max_photos':3,
            'cameras':[{'ip':f'127.0.0.{i}','label':label} for i,label in enumerate(['左上','右上','左下','右下'],1)]}
    (root/'config.json').write_text(json.dumps(config,ensure_ascii=False),encoding='utf-8')
    prior=root/'photos/127.0.0.1';prior.mkdir(parents=True)
    (prior/'legacy.jpg').write_bytes(b'legacy-ng');(prior/'legacy.xml').write_bytes(NG)
    process=subprocess.Popen([str(EXE),'--headless','--data-dir',str(root)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    url='http://127.0.0.1:28080'
    def state():return json.load(urllib.request.urlopen(url+'/api/status',timeout=2))
    def connect(i,active=False):
        ftp=ftplib.FTP(source_address=(f'127.0.0.{i}',0));ftp.connect('127.0.0.1',22121,timeout=10);ftp.login('anonymous','anonymous');ftp.set_pasv(not active);return ftp
    def upload(i,xml=OK,reverse=False,active=False):
        with connect(i,active) as ftp:
            if reverse:ftp.storbinary('STOR repeated.xml',io.BytesIO(xml))
            ftp.storbinary('STOR repeated.jpg',io.BytesIO(f'image-{i}'.encode()))
            if not reverse:ftp.storbinary('STOR repeated.xml',io.BytesIO(xml))
    try:
        for _ in range(60):
            if process.poll() is not None:raise RuntimeError(process.stderr.read().decode(errors='replace'))
            try:state();break
            except OSError:time.sleep(.1)
        else:raise AssertionError('Native receiver did not start')
        assert (root/'ng_photos/127.0.0.1/legacy.jpg').exists()
        assert state()['max_photos']==3 and Path(state()['data_path'])==root
        for asset,kind in (('/app.js','javascript'),('/app.css','css')):
            response=urllib.request.urlopen(url+asset)
            assert kind in response.headers['Content-Type'] and response.read()
        if os.environ.get('CT_DIAG'):
            for i in range(1,5):
                print('Testing source',i,flush=True);upload(i)
        else:
            with concurrent.futures.ThreadPoolExecutor(4) as pool:list(pool.map(upload,range(1,5)))
        cameras=state()['cameras'];assert all(c['count']==1 and c['latest']['codes']==['001067'] for c in cameras)
        for c in cameras:assert urllib.request.urlopen(url+'/'+c['latest']['file']).read()==f"image-{c['ip'].split('.')[-1]}".encode()
        upload(1,NG);ng=state()['cameras'][0]['ng'];assert ng and state()['cameras'][0]['latest']['result']=='ng'
        assert (root/'ng_photos/127.0.0.1'/ng['name']).exists()
        upload(1,OK,reverse=True);assert state()['cameras'][0]['ng']==ng
        upload(1,OK,active=True)
        assert state()['cameras'][0]['retained']==3
        assert sum(len(list((root/k/'127.0.0.1').rglob('*.jpg'))) for k in ('photos','ng_photos'))==3
        assert urllib.request.urlopen(url+'/'+ng['file']).read()==b'image-1'
        with connect(2) as ftp:
            for path in ('../escape.jpg','C:/escape.jpg'):
                try:ftp.storbinary('STOR '+path,io.BytesIO(b'bad'));raise AssertionError('Traversal accepted')
                except ftplib.error_perm:pass
            try:ftp.sendcmd('PORT 8,8,8,8,20,0');raise AssertionError('FTP bounce accepted')
            except ftplib.error_perm:pass
            try:ftp.login('anonymous','wrong');raise AssertionError('Wrong password accepted')
            except ftplib.error_perm:pass
        for ip in ('127.0.0.1','127.0.0.2','127.0.0.3','127.0.0.4'):
            for base in ('photos','ng_photos'):
                for image in (root/base/ip).rglob('*.jpg'):assert image.with_suffix('.xml').exists()
        assert urllib.request.urlopen(url+'/logo.png').read()[:8]==b'\x89PNG\r\n\x1a\n'
        def burst(i):
            for _ in range(10):upload(i,NG if i==1 else OK)
        with concurrent.futures.ThreadPoolExecutor(4) as pool:list(pool.map(burst,range(1,5)))
        assert all(c['retained']==3 for c in state()['cameras'])
        ng=state()['cameras'][0]['ng']
        before=state()
        process.terminate();process.wait(timeout=10)
        process=subprocess.Popen([str(EXE),'--headless','--data-dir',str(root)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        for _ in range(60):
            try:
                after=state()
                if after['cameras'][0]['count']==before['cameras'][0]['count']:break
            except OSError:pass
            time.sleep(.1)
        assert state()['cameras'][0]['ng']==ng
        print('PASS: four simultaneous IPs; image isolation; passive/active FTP; either file order; repeated names; NG/OK; NG archive; combined cap; migration/restart; credentials; traversal/bounce rejection; embedded logo')
    finally:
        if sys.exc_info()[0]:time.sleep(.5)
        if sys.exc_info()[0]:print('Receiver exit status before cleanup:',process.poll())
        process.terminate();process.wait(timeout=10)
        if sys.exc_info()[0]:
            log=root/'logs/receiver-rust.log'
            print(log.read_text(encoding='utf-8',errors='replace') if log.exists() else 'No receiver log')
            print(process.stderr.read().decode(errors='replace'))
    for port in (22121,28080):
        with socket.socket() as probe:assert probe.connect_ex(('127.0.0.1',port))!=0
print('PASS: process exit releases FTP and HTTP ports')
