from pathlib import Path
import json,gzip,hashlib,subprocess,os,sys,resource,re
root=Path('/home/john-riley/.codex/worktrees/4836/lang');out=Path('/home/john-riley/projects/lang/target/ordinary-memory-review-20261024/a3b98d7');out.mkdir(parents=True,exist_ok=False)
target=Path('/home/john-riley/projects/lang/target/release-plan-review');tmp=Path('/home/john-riley/projects/lang/target/cgx2-wave21-tmp');frozen=out/'frozen'
sys.path.insert(0,str(root/'crates/cgx2-syntax/owned-result-runner-evidence'))
from supervisor import supervise,evidence
env={**os.environ,'RUSTUP_TOOLCHAIN':'1.98.1','CARGO_BUILD_JOBS':'2','CARGO_TARGET_DIR':str(target),'TMPDIR':str(tmp),'LIBRARY_PATH':'/tmp/cgx2-wave22-lib','PYTHONDONTWRITEBYTECODE':'1'}
report={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'source_root':str(root),'checks':[],'ordinary_memory_complete':False,'release_complete':False,'environment':{k:env[k] for k in ['RUSTUP_TOOLCHAIN','CARGO_BUILD_JOBS','CARGO_TARGET_DIR','TMPDIR','LIBRARY_PATH']}}
def limits():
 resource.setrlimit(resource.RLIMIT_AS,(4*1024**3,4*1024**3));resource.setrlimit(resource.RLIMIT_CPU,(1800,1800));resource.setrlimit(resource.RLIMIT_FSIZE,(512*1024**2,512*1024**2))
def run(name,cmd,timeout=900):
 print('start',name,flush=True);r=supervise([str(a) for a in cmd],cwd=root,env=env,limits=limits,cap=8*1024**2,timeout=timeout,merged=True);raw=r['stdout'];log=out/(name+'.log.gz');log.write_bytes(gzip.compress(raw,mtime=0))
 row={'name':name,'command':[str(a) for a in cmd],**evidence(r,include_output=False),'log':log.name,'log_sha256':hashlib.sha256(raw).hexdigest(),'passed':r['exit']==0 and r['reason']=='exit'}
 if name=='workspace':
  counts=re.findall(rb'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored',raw)
  row['test_counts']={k:sum(int(x[i]) for x in counts) for k,i in [('passed',1),('failed',2),('ignored',3)]}
 report['checks'].append(row);(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n');print('finish',name,row['passed'],flush=True);return row['passed']
checks=[('workspace',['cargo','test','--workspace','--all-features','--offline','--locked','--no-fail-fast']),('clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--offline','--locked','--','-D','warnings']),('fmt',['cargo','fmt','--all','--check']),('diff',['git','diff','--check']),('evidence-selftests',[sys.executable,'crates/cgx2-syntax/ordinary-memory-evidence/test_run.py']),('build',['cargo','build','--workspace','--all-features','--offline','--locked']),('freeze',[sys.executable,'crates/cgx2-syntax/managed-core-repair-evidence/freeze.py','--binary-dir',target/'debug','--output-dir',frozen,'--manifest',out/'frozen.json'])]
for n,c in checks:
 if not run(n,c):sys.exit(1)
run('product',[sys.executable,'crates/cgx2-syntax/ordinary-memory-evidence/run.py','--binary-dir',frozen,'--output-dir',out/'product','--regressions'],3600)
run('standalone',[sys.executable,'crates/cgx2-syntax/ordinary-memory-evidence/standalone.py','--product-dir',out/'product','--output-dir',out/'standalone'],1200)
m=json.loads((out/'frozen.json').read_text());changed=[]
for key in ['binary_sha256','probe_library_sha256']:
 for name,h in m[key].items():
  if hashlib.sha256((frozen/name).read_bytes()).hexdigest()!=h:changed.append(name)
report['frozen_artifacts_changed']=changed
(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n')
sys.exit(int(bool(changed) or not all(x['passed'] for x in report['checks'])))
