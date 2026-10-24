from pathlib import Path
import json,gzip,hashlib,subprocess,os,sys,resource,shutil,re
root=Path('/home/john-riley/.codex/worktrees/a9af/lang');out=Path('/home/john-riley/projects/lang/target/ordinary-admission-review-20261024/9330300');out.mkdir(parents=True,exist_ok=False)
target=Path('/home/john-riley/projects/lang/target/release-plan-review');frozen=out/'frozen'
sys.path.insert(0,str(root/'crates/cgx2-syntax/owned-result-runner-evidence'))
from supervisor import supervise,evidence
env={**os.environ,'RUSTUP_TOOLCHAIN':'1.98.1','CARGO_BUILD_JOBS':'2','CARGO_TARGET_DIR':str(target),'TMPDIR':'/home/john-riley/projects/lang/target/cgx2-wave21-tmp','LIBRARY_PATH':'/tmp/cgx2-wave22-lib','PYTHONDONTWRITEBYTECODE':'1'}
report={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'checks':[],'acceptance_scope':'Admission repair review baseline; nominal borrow constructor defect remains open'}
def limits():
 resource.setrlimit(resource.RLIMIT_AS,(4*1024**3,4*1024**3));resource.setrlimit(resource.RLIMIT_CPU,(900,900));resource.setrlimit(resource.RLIMIT_FSIZE,(512*1024**2,512*1024**2))
def run(name,cmd):
 print('start',name,flush=True);r=supervise([str(a) for a in cmd],cwd=root,env=env,limits=limits,cap=8*1024**2,timeout=1200,merged=True);raw=r['stdout'];(out/(name+'.log.gz')).write_bytes(gzip.compress(raw,mtime=0));row={'name':name,'command':list(map(str,cmd)),**evidence(r,include_output=False),'passed':r['exit']==0 and r['reason']=='exit','raw_log_sha256':hashlib.sha256(raw).hexdigest()}
 if name=='tests':
  counts=re.findall(rb'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',raw);row['counts']={k:sum(int(c[i]) for c in counts) for k,i in [('passed',0),('failed',1),('ignored',2)]}
 report['checks'].append(row);(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n');print('finish',name,row['passed'],flush=True);return row['passed']
for name,cmd in [('tests',['cargo','test','-p','cgx2-sema','-p','cgx2-package','-p','cgx2-cli','--all-features','--offline','--locked']),('clippy',['cargo','clippy','-p','cgx2-sema','-p','cgx2-package','-p','cgx2-cli','--all-targets','--all-features','--offline','--locked','--','-D','warnings']),('fmt',['cargo','fmt','--all','--check']),('diff',['git','diff','--check']),('build',['cargo','build','--workspace','--all-features','--offline','--locked'])]:
 if not run(name,cmd):sys.exit(1)
frozen.mkdir();artifacts={}
for name in ['cgx2','cgx2-native','cgx2c','libcgx2_api_index.rlib','libcgx2_cli.rlib']:
 shutil.copyfile(target/'debug'/name,frozen/name);os.chmod(frozen/name,0o755);artifacts[name]=hashlib.sha256((frozen/name).read_bytes()).hexdigest()
manifest={'binary_directory':str(frozen),'artifact_sha256':artifacts,'source_sha256':{str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [root/'Cargo.toml',root/'Cargo.lock',*[p for p in (root/'crates').rglob('*') if p.is_file() and p.suffix in ('.rs','.cg','.toml')]]}}
(frozen/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
run('product',[sys.executable,root/'crates/cgx2-syntax/ordinary-admission-repair-evidence/run.py','--binary-dir',frozen,'--output-dir',out/'product'])
report['frozen_artifacts_changed']=[n for n,h in artifacts.items() if hashlib.sha256((frozen/n).read_bytes()).hexdigest()!=h];(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n');sys.exit(int(bool(report['frozen_artifacts_changed']) or not all(r['passed'] for r in report['checks'])))
