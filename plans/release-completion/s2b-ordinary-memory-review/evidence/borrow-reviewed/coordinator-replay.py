from pathlib import Path
import json,hashlib,gzip,subprocess,os,sys,resource
root=Path('/home/john-riley/.codex/worktrees/wave21-coordinator-review/lang');out=Path('/home/john-riley/projects/lang/target/ordinary-borrow-coordinator-review-20261024');out.mkdir(exist_ok=False)
b=Path('/home/john-riley/.codex/visualizations/2026/10/24/01a16ea8-819b-7261-a64a-9bf5a9bb1b8f/borrow-nominal-frozen-01');m=json.loads((b/'manifest.json').read_text());sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
report={'coordinator_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'worker_commit':'35a8618996c2f89fadd3b89fa8f4a123765688e6','binary_dir':str(b),'manifest_sha256':sha(b/'manifest.json'),'source_changes':[k for k,v in m['source_sha256'].items() if not (root/k).is_file() or sha(root/k)!=v],'artifact_changes':[k for k,v in m['artifact_sha256'].items() if sha(b/k)!=v],'dependency_changes':[k for k,v in m['dependency_sha256'].items() if sha(b/k)!=v],'checks':[]}
(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n');assert not(report['source_changes'] or report['artifact_changes'] or report['dependency_changes']), report
sys.path.insert(0,str(root/'crates/cgx2-syntax/owned-result-runner-evidence'));from supervisor import supervise,evidence
env={**os.environ,'RUSTUP_TOOLCHAIN':'1.98.1','CARGO_BUILD_JOBS':'2','TMPDIR':'/home/john-riley/projects/lang/target/cgx2-wave21-tmp','LIBRARY_PATH':'/tmp/cgx2-wave22-lib','PYTHONDONTWRITEBYTECODE':'1'}
def limits():
 resource.setrlimit(resource.RLIMIT_CPU,(1200,)*2);resource.setrlimit(resource.RLIMIT_AS,(4*1024**3,)*2);resource.setrlimit(resource.RLIMIT_FSIZE,(512*1024**2,)*2)
external=Path('/home/john-riley/.codex/visualizations/2026/10/23/01a167e4-6669-7803-b925-af52f00fe0f7/ordinary-borrow-coordinator-review')
cmd=[sys.executable,str(root/'crates/cgx2-syntax/ordinary-borrow-repair-evidence/run.py'),'--binary-dir',str(b),'--output-dir',str(external)]
report['checks'].append({'name':'new-product','argv':cmd,'cwd':str(root),'reason':'pending'});(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n');print('start243 product checks',flush=True)
r=supervise(cmd,cwd=root,env=env,limits=limits,cap=8*1024**2,timeout=1200);row=report['checks'][-1];row.update(evidence(r));(out/'product.log.gz').write_bytes(gzip.compress(r['stdout']+r['stderr'],mtime=0));report['artifact_changes_after']=[k for k,v in m['artifact_sha256'].items() if sha(b/k)!=v];report['dependency_changes_after']=[k for k,v in m['dependency_sha256'].items() if sha(b/k)!=v];report['external_output']=str(external);(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n');print('finish',r['exit'],r['reason'],flush=True)
