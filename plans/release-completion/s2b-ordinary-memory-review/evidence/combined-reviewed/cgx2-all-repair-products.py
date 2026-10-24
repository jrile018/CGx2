from pathlib import Path
import json,gzip,hashlib,subprocess,os,sys,resource,shutil,re
root=Path('/home/john-riley/.codex/worktrees/wave21-coordinator-review/lang');out=Path('/home/john-riley/projects/lang/target/ordinary-combined-review-20261024/all-repairs-rerun');out.mkdir(parents=True,exist_ok=True)
target=Path('/home/john-riley/projects/lang/target/release-plan-review');frozen=out/'frozen'
sys.path.insert(0,str(root/'crates/cgx2-syntax/owned-result-runner-evidence'))
from supervisor import supervise,evidence
env={**os.environ,'RUSTUP_TOOLCHAIN':'1.98.1','CARGO_BUILD_JOBS':'2','CARGO_TARGET_DIR':str(target),'TMPDIR':'/home/john-riley/projects/lang/target/cgx2-wave21-tmp','LIBRARY_PATH':'/tmp/cgx2-wave22-lib','PYTHONDONTWRITEBYTECODE':'1'}
report={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'checks':[],'acceptance_scope':'Combined four bounded repairs; nominal borrow, harness completeness and remaining source capabilities open'}
def limits():
 resource.setrlimit(resource.RLIMIT_AS,(4*1024**3,4*1024**3));resource.setrlimit(resource.RLIMIT_CPU,(900,900));resource.setrlimit(resource.RLIMIT_FSIZE,(512*1024**2,512*1024**2))
def run(name,cmd):
 print('start',name,flush=True);r=supervise([str(a) for a in cmd],cwd=root,env=env,limits=limits,cap=8*1024**2,timeout=1200,merged=True);raw=r['stdout'];(out/(name+'.log.gz')).write_bytes(gzip.compress(raw,mtime=0));row={'name':name,'command':list(map(str,cmd)),**evidence(r,include_output=False),'passed':r['exit']==0 and r['reason']=='exit','raw_log_sha256':hashlib.sha256(raw).hexdigest()}
 if name=='tests':
  counts=re.findall(rb'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',raw);row['counts']={k:sum(int(c[i]) for c in counts) for k,i in [('passed',0),('failed',1),('ignored',2)]}
 report['checks'].append(row);(out/'product-verification.json').write_text(json.dumps(report,indent=2)+'\n');print('finish',name,row['passed'],flush=True);return row['passed']

external=Path('/home/john-riley/.codex/visualizations/2026/10/23/01a167e4-6669-7803-b925-af52f00fe0f7/ordinary-combined-20261024')
external.mkdir(parents=True,exist_ok=False)
(root/'target/tmp-ordinary-evidence-parity').mkdir(parents=True,exist_ok=True)
(root/'target/release-ordinary-evidence-parity').mkdir(parents=True,exist_ok=True)
here=root/'crates/cgx2-syntax/ordinary-parity-repair-evidence'
cmds=[('api',[sys.executable,root/'crates/cgx2-syntax/ordinary-api-repair-evidence/run.py','--binary-dir',frozen,'--output-dir',out/'api']),('admission',[sys.executable,root/'crates/cgx2-syntax/ordinary-admission-repair-evidence/run.py','--binary-dir',frozen,'--output-dir',external/'admission']),('matrix',[sys.executable,here/'run.py','--binary-dir',frozen,'--output-dir',out/'matrix','--standalone-root',external]),('auto',[sys.executable,here/'auto.py','--binary-dir',frozen,'--output-dir',out/'auto','--standalone-root',external]),('historical',[sys.executable,here/'replay.py','--binary-dir',frozen,'--output-dir',out/'historical']),('package',[sys.executable,here/'package_parity.py','--binary-dir',frozen,'--historical-product',out/'historical/product','--output-dir',out/'package','--standalone-root',external]),('review',[sys.executable,here/'review.py','--binary-dir',frozen,'--matrix-report',out/'matrix/report.json','--output-dir',out/'review']),('selftests',[sys.executable,here/'test_harness.py'])]
for name,cmd in cmds:run(name,cmd)
manifest=json.loads((frozen/'manifest.json').read_text());stage1=json.loads((out/'verification.json').read_text())
report['frozen_artifacts_changed']=[n for n,h in manifest['artifact_sha256'].items() if hashlib.sha256((frozen/n).read_bytes()).hexdigest()!=h]
report['dependencies_changed']=[n for n,h in stage1['dependency_hashes'].items() if hashlib.sha256((frozen/'deps'/n).read_bytes()).hexdigest()!=h]
report['external_root']=str(external)
(out/'product-verification.json').write_text(json.dumps(report,indent=2)+'\n')
print('products finished',flush=True)
