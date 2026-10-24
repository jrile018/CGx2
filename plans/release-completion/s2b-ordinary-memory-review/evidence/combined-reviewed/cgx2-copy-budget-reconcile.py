from pathlib import Path
import sys,json,os,resource,hashlib,gzip,difflib
root=Path('/home/john-riley/.codex/worktrees/wave21-coordinator-review/lang');out=Path('/home/john-riley/projects/lang/target/ordinary-combined-review-20261024/all-repairs-rerun/copy-budget-reconciliation');out.mkdir(exist_ok=False)
sys.path.insert(0,str(root/'crates/cgx2-syntax/owned-result-runner-evidence'));from supervisor import supervise,evidence
source=root/'crates/cgx2-syntax/ordinary-parity-repair-evidence/runtime_probe.rs';original=source.read_text();changed=original.replace('let copy=4*root +','let copy=6*root +');assert changed!=original
probe=out/'runtime_probe.rs';probe.write_text(changed);(out/'probe.diff').write_text(''.join(difflib.unified_diff(original.splitlines(True),changed.splitlines(True),fromfile=str(source),tofile=str(probe))))
b=out.parent/'frozen';cmds=[['rustc','-C','link-arg=-Wl,--threads=2','--edition=2021',str(probe),'--extern',f'cgx2_runtime={b}/libcgx2_runtime.rlib','-L',f'dependency={b}/deps','-o',str(out/'probe')],[str(out/'probe')]]
env={**os.environ,'RUSTUP_TOOLCHAIN':'1.98.1','TMPDIR':'/home/john-riley/projects/lang/target/cgx2-wave21-tmp'}
def limits():
 resource.setrlimit(resource.RLIMIT_CPU,(120,120));resource.setrlimit(resource.RLIMIT_AS,(4*1024**3,)*2);resource.setrlimit(resource.RLIMIT_FSIZE,(512*1024**2,)*2)
report={'rationale':'Safe source lifetime retains the source root until publication. Source root slot len1/cap1 now expands to cap2 for destination publication, charging full64-byte replacement nonrefundable root table. Independently derived expected copy charge439=old375+64; guard metadata16 yields455. Original375 failure preserved unchanged. Runtime implementation and frozen artifacts unchanged.','commands':[],'original_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'probe_sha256':hashlib.sha256(probe.read_bytes()).hexdigest()}
for i,cmd in enumerate(cmds):
 row={'argv':cmd,'cwd':str(out)};report['commands'].append(row);(out/'report.json').write_text(json.dumps(report,indent=2));r=supervise(cmd,cwd=out,env=env,limits=limits,cap=2*1024**2,timeout=180);row.update(evidence(r));(out/'report.json').write_text(json.dumps(report,indent=2));assert r['exit']==0
rows=[json.loads(s) for s in report['commands'][-1]['stdout'].splitlines()];report['checks']=rows;report['passed']=sum(r['passed'] for r in rows);report['failed']=sum(not r['passed'] for r in rows);(out/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(report['passed'],report['failed']);print([r for r in rows if r['name'].startswith('copy-budget')])
