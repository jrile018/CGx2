import argparse,gzip,hashlib,json,os,resource,sys,re,subprocess
from pathlib import Path
root=Path('/home/john-riley/.codex/worktrees/wave21-coordinator-review/lang')
out=Path('/home/john-riley/projects/lang/target/managed-repairs-review-20261024/managed-matrix-0949d5c');out.mkdir(parents=True,exist_ok=False)
sys.path.insert(0,str(root/'crates/cgx2-syntax/owned-result-runner-evidence'))
from supervisor import supervise,evidence
old='/home/john-riley/.codex/worktrees/685c/lang'
target=Path('/home/john-riley/projects/lang/target/release-plan-review');tmp=Path('/home/john-riley/projects/lang/target/cgx2-wave21-tmp');frozen=Path('/home/john-riley/projects/lang/target/managed-repairs-review-20261024/combined-b0f1ccc/frozen')
env={**os.environ,'RUSTUP_TOOLCHAIN':'1.98.1','CARGO_BUILD_JOBS':'2','CARGO_TARGET_DIR':str(target),'TMPDIR':str(tmp),'LIBRARY_PATH':'/tmp/cgx2-wave22-lib','PYTHONDONTWRITEBYTECODE':'1'}
delivery=json.loads((root/'crates/cgx2-syntax/default-managed-evidence/summary.json').read_text())
report={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'source_root':str(root),'checks':[],'release_complete':False,'external_evidence':'unrun; leak-enabled host controls remain blocked','environment':{k:env[k] for k in ['RUSTUP_TOOLCHAIN','CARGO_BUILD_JOBS','CARGO_TARGET_DIR','TMPDIR','LIBRARY_PATH']}}
def limits():
 resource.setrlimit(resource.RLIMIT_AS,(4*1024**3,4*1024**3));resource.setrlimit(resource.RLIMIT_CPU,(900,900));resource.setrlimit(resource.RLIMIT_FSIZE,(512*1024**2,512*1024**2))
def run(name,cmd):
 print('start',name,flush=True)
 result=supervise([str(x) for x in cmd],cwd=root,env=env,limits=limits,cap=8*1024**2,timeout=900,merged=True)
 data=result['stdout'];log=out/(name+'.log.gz');log.write_bytes(gzip.compress(data,mtime=0))
 record={'name':name,'command':[str(x) for x in cmd],**evidence(result,include_output=False),'log':log.name,'log_sha256':hashlib.sha256(data).hexdigest(),'status':'passed' if result['exit']==0 and result['reason']=='exit' else 'failed'}
 if name=='owned-result-original' and result['exit']==1 and result['reason']=='exit' and b'shadow-builtin:reject' in data:record['status']='known-superseded-negative'
 if name=='workspace-tests':
  rows=re.findall(rb'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored',data)
  record.update({label:sum(int(r[i]) for r in rows) for label,i in [('passed',1),('failed',2),('ignored',3)]})
 report['checks'].append(record);(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n')
 print('finish',name,record['status'],result['exit'],flush=True)
 return record['status']!='failed'

previous=out.parent/'combined-b0f1ccc'
(out/'frozen.json').write_bytes((previous/'frozen.json').read_bytes())
report['compiler_source_commit']='b0f1ccc4ad8130e22419f2a5366fd6a636c4b621'
report['prior_attempt']=str(previous/'verification.json')
run('repair-selftests',[sys.executable,'crates/cgx2-syntax/default-managed-evidence/test_repair_runner.py'])
run('managed-product',[sys.executable,'crates/cgx2-syntax/default-managed-evidence/run.py','--binary-dir',frozen,'--output-dir',out/'managed-product'])
manifest=json.loads((out/'frozen.json').read_text());changed=[]
for key in ['binary_sha256','probe_library_sha256']:
 for name,digest in manifest[key].items():
  if hashlib.sha256((frozen/name).read_bytes()).hexdigest()!=digest:changed.append(name)
report['frozen_artifacts_changed']=changed
inventory=json.loads((previous/'source-inventory.json').read_text())['sha256']
report['source_changes_since_compiler_build']=[p for p,h in inventory.items() if hashlib.sha256((root/p).read_bytes()).hexdigest()!=h]
(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n')
sys.exit(int(bool(changed) or any(x['status']=='failed' for x in report['checks'])))
