"""Package a built Windows binary; Python is only needed by release maintainers."""
from pathlib import Path
import argparse, hashlib, os, shutil, subprocess, tempfile, tomllib, zipfile

parser=argparse.ArgumentParser()
parser.add_argument('--exe',type=Path,required=True)
parser.add_argument('--out-dir',type=Path,required=True)
parser.add_argument('--cargo',default='cargo')
args=parser.parse_args()
project=Path(__file__).resolve().parents[1]
version=tomllib.loads((project/'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
out=args.out_dir.resolve();out.mkdir(parents=True,exist_ok=True)
cargo_home=Path(os.environ.get('CARGO_HOME',str(Path.home()/'.cargo')))
tree=subprocess.check_output([args.cargo,'tree','--manifest-path',str(project/'Cargo.toml'),'--target','x86_64-pc-windows-msvc','--prefix','none','--format','{p}','--locked','--offline'],encoding='utf-8')
used={(parts[0],parts[1].removeprefix('v')) for line in tree.splitlines() if len(parts:=line.split())>=2 and parts[1].startswith('v')}
with tempfile.TemporaryDirectory(prefix='chingtech-package-') as folder:
    stage=Path(folder)/'ChingTech-Monitor-Rust';stage.mkdir()
    for source,name in [(args.exe,'ChingTech-Monitor-Rust.exe'),(project/'assets/app.ico','ChingTech-Monitor.ico'),(project/'README.md','使用說明.md'),(project/'config.example.json','config.example.json'),(project/'scripts/Allow-Cameras.cmd','允許設備傳入照片.cmd')]:
        shutil.copyfile(source,stage/name)
    notice=['Third-party dependencies. License texts are included under licenses/.','Versions are locked in Cargo.lock in the source release.','']
    for package in tomllib.loads((project/'Cargo.lock').read_text(encoding='utf-8'))['package']:
        if 'source' not in package or (package['name'],package['version']) not in used:continue
        matches=list((cargo_home/'registry/src').glob(f"*/{package['name']}-{package['version']}"))
        if not matches:raise RuntimeError(f"Dependency source not available: {package['name']} {package['version']}")
        source=matches[0];meta=tomllib.loads((source/'Cargo.toml').read_text(encoding='utf-8'))['package']
        notice.append(f"{package['name']} {package['version']} | {meta.get('license','See license file')} | {meta.get('repository','')}")
        for file in source.iterdir():
            if file.is_file() and file.name.upper().startswith(('LICENSE','LICENCE','COPYING','NOTICE','UNLICENSE')):
                destination=stage/'licenses'/source.name/file.name;destination.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(file,destination)
    (stage/'THIRD_PARTY_NOTICES.txt').write_text('\n'.join(notice)+'\n',encoding='utf-8')
    archive=out/f'ChingTech-Monitor-Rust-v{version}.zip'
    with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED) as z:
        for file in sorted(stage.rglob('*')):
            if file.is_file():z.write(file,file.relative_to(stage.parent))
    with zipfile.ZipFile(archive) as z:assert z.testzip() is None
    for name in ('ChingTech-Monitor-Rust.exe','ChingTech-Monitor.ico'):
        shutil.copyfile(stage/name,out/name)
    shutil.copyfile(stage/'使用說明.md',out/'Rust版使用說明.md')
    shutil.copyfile(archive,out/'ChingTech-Monitor-Rust.zip')
    print(archive.name,archive.stat().st_size,'bytes')
    print('EXE SHA256',hashlib.sha256((out/'ChingTech-Monitor-Rust.exe').read_bytes()).hexdigest())
