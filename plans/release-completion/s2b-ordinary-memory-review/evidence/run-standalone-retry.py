from pathlib import Path
import sys,os,gzip,json,hashlib,resource
out=Path(__file__).parent
root=Path('/home/john-riley/.codex/worktrees/4836/lang')
sys.path.insert(0,str(root/'crates/cgx2-syntax/owned-result-runner-evidence'))
from supervisor import supervise,evidence
env={**os.environ,'RUSTUP_TOOLCHAIN':'1.98.1','CARGO_BUILD_JOBS':'2','TMPDIR':'/home/john-riley/projects/lang/target/cgx2-wave21-tmp','PYTHONDONTWRITEBYTECODE':'1'}
def limits():
    resource.setrlimit(resource.RLIMIT_AS,(4*1024**3,4*1024**3))
    resource.setrlimit(resource.RLIMIT_CPU,(900,900))
    resource.setrlimit(resource.RLIMIT_FSIZE,(512*1024**2,512*1024**2))
argv=[sys.executable,str(out/'standalone-outside-retry.py'),'--product-dir',str(out/'product'),'--output-dir',str(out/'standalone-retry')]
r=supervise(argv,cwd=root,env=env,limits=limits,cap=8*1024**2,timeout=1200,merged=True)
raw=r['stdout'];(out/'standalone-retry.log.gz').write_bytes(gzip.compress(raw,mtime=0))
report={'command':argv,**evidence(r,include_output=False),'passed':r['exit']==0 and r['reason']=='exit','log_sha256':hashlib.sha256(raw).hexdigest(),'directory_substitution_only':True,'original_runner_sha256':hashlib.sha256((root/'crates/cgx2-syntax/ordinary-memory-evidence/standalone.py').read_bytes()).hexdigest(),'wrapper_sha256':hashlib.sha256((out/'standalone-outside-retry.py').read_bytes()).hexdigest()}
(out/'standalone-retry-verification.json').write_text(json.dumps(report,indent=2)+'\n')
print(raw.decode(errors='replace'));print('passed',report['passed'])
sys.exit(0 if report['passed'] else 1)
