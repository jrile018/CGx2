#!/usr/bin/env python3
"""Required bounded regressions; historical files are read only."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import sys

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
sys.path.insert(0,str(HERE.parent/'owned-result-runner-evidence'))
from supervisor import supervise,evidence

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir',type=Path,required=True)
    parser.add_argument('--only',help='Run named checks (comma-separated) in a separate attempt directory')
    args=parser.parse_args();out=args.output_dir.resolve();out.mkdir(parents=True,exist_ok=True)
    target=ROOT/'target/release-default-managed';temporary=ROOT/'target/tmp-default-managed'
    temporary.mkdir(parents=True,exist_ok=True);target.mkdir(parents=True,exist_ok=True)
    env={**os.environ,'RUSTUP_TOOLCHAIN':'1.98.1','CARGO_BUILD_JOBS':'2','CARGO_TARGET_DIR':str(target),'TMPDIR':str(temporary),'LIBRARY_PATH':'/tmp/cgx2-wave22-lib'}
    binary=target/'debug';py=sys.executable
    commands=[
        ('build',['cargo','build','--workspace','--all-features','--offline','--locked']),
        ('workspace-tests',['cargo','test','--workspace','--all-features','--offline','--locked','--no-fail-fast']),
        ('clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--offline','--locked','--','-D','warnings']),
        ('fmt',['cargo','fmt','--all','--check']),('diff',['git','diff','--check']),
        ('managed-product',[py,HERE/'run.py','--binary-dir',binary,'--output-dir',out/'managed-product']),
        ('owned-symbol195',[py,'crates/cgx2-syntax/owned-symbol-compatibility-evidence/run.py','--binary-dir',binary,'--output',out/'owned-symbol195.json']),
        ('owned-result-original',[py,'crates/cgx2-syntax/owned-result-evidence/run.py','--binary-dir',binary,'--output',out/'owned-result-original.json']),
        ('owned-result-reconciled',[py,'plans/release-completion/s2b-owned-result-review/reconcile.py','--binary-dir',binary,'--output-dir',out/'reconciled']),
        ('ordinary256',[py,'crates/cgx2-syntax/ordinary-owned-evidence/run.py','--binary-dir',binary,'--output',out/'ordinary256.json']),
        ('foundation',[py,'crates/cgx2-syntax/s2b-evidence/run.py','--binary-dir',binary,'--output',out/'foundation.json']),
        ('semantic',[py,'crates/cgx2-sema/tests/semantic_resolution/run.py','--binary',binary/'cgx2','--output',out/'semantic.json']),
        ('specialization',[py,'crates/cgx2-sema/tests/source_specialization/run.py','--binary',binary/'cgx2','--output-dir',out/'specialization']),
        ('visibility',[py,'crates/cgx2-api-index/tests/verify_visibility_cli.py','--binary',binary/'cgx2','--output-dir',out/'visibility']),
        ('binders',[py,'crates/cgx2-package/scripts/verify_binders.py','--binary-dir',binary,'--output',out/'binders.json']),
        ('output',[py,'crates/cgx2-cli/scripts/verify_output_bounds.py','--binary',binary/'cgx2','--output',out/'output.json','--build-address-space-mib','4096']),
        ('emitters',[py,'crates/cgx2-emit-python/tests/s2b_product.py','--binary-dir',binary,'--output',out/'emitters.json']),
        ('graph99',[py,'crates/cgx2-syntax/s2b-memory-evidence/run.py','--binary-dir',binary,'--output',out/'graph99.json']),
        ('guards518',[py,'crates/cgx2-emit-rust/tests/graph_guard_parity/run.py','--binary-dir',binary,'--output',out/'guards518.json']),
        ('reports265',[py,'crates/cgx2-cli/tests/memory_report_layout/run.py','--binary',binary/'cgx2','--output',out/'reports265.json']),
        ('asan-roots-no-leaks',[py,HERE/'asan_roots.py','--output',out/'asan-roots-no-leaks','--detect-leaks','0']),
        ('asan-roots-leaks',[py,HERE/'asan_roots.py','--output',out/'asan-roots-leaks']),
        ('asan-no-leaks',[py,'crates/cgx2-memory-conformance/scripts/accept_lifetimes_asan.py','--output',out/'asan-no-leaks','--detect-leaks','0']),
        ('asan-leaks',[py,'crates/cgx2-memory-conformance/scripts/accept_lifetimes_asan.py','--output',out/'asan-leaks']),
    ]
    if args.only:
        commands=[(n,c) for n,c in commands if n in args.only.split(',')]
        if not commands:parser.error('unknown check '+args.only)
    checks=[];report={'schema_version':1,'base':'c9d60229ecf3ed2bf76b29525fec0bdda41d0906','checks':checks,'checkpoint_complete':False,'release_complete':False,'dependency_reconciliation':'none; root Cargo files unchanged','environment':{k:env[k] for k in ['RUSTUP_TOOLCHAIN','CARGO_BUILD_JOBS','CARGO_TARGET_DIR','TMPDIR','LIBRARY_PATH']},'bounds':{'wall_seconds':900,'cpu_seconds_per_process':900,'address_space_bytes_per_process':4*1024**3,'retained_combined_bytes':8*1024**2,'aggregate_cgroup_isolation':False},'external_audit_performance':'unrun'}
    def limits():
        resource.setrlimit(resource.RLIMIT_AS,(4*1024**3,4*1024**3));resource.setrlimit(resource.RLIMIT_CPU,(900,900))
    for name,command in commands:
        print('start',name,flush=True)
        command=[str(a) for a in command]
        check_env=env.copy()
        check_limits=limits
        if name.startswith('asan-'):
            # ASan reserves sparse multi-terabyte shadow address space. A 4 GiB
            # RLIMIT_AS prevents its negative control and every test from starting.
            # Bound CPU/wall/output and use ASan's per-process RSS monitor instead.
            check_env['ASAN_OPTIONS']=check_env.get('ASAN_OPTIONS','')+':hard_rss_limit_mb=4096'
            def sanitizer_limits():
                resource.setrlimit(resource.RLIMIT_CPU,(900,900))
                resource.setrlimit(resource.RLIMIT_FSIZE,(64*1024**2,64*1024**2))
            check_limits=sanitizer_limits
        result=supervise(command,cwd=ROOT,env=check_env,limits=check_limits,cap=8*1024**2,timeout=900,merged=True)
        data=result['stdout'];record={'name':name,'command':command,**evidence(result,include_output=False)}
        if name.startswith('asan-'):record['sanitizer_bounds']={'address_space':'unlimited for sparse shadow','rss_monitor_mib_per_process':4096,'rss_polling_is_not_strict_peak_bound':True,'cpu_seconds_per_process':900,'wall_seconds':900,'file_size_bytes':64*1024**2,'retained_combined_bytes':8*1024**2}
        path=out/(name+'.log.gz');path.write_bytes(gzip.compress(data,mtime=0));record['log']=path.name
        record['log_sha256']=hashlib.sha256(data).hexdigest()
        record['status']='passed' if result['reason']=='exit' and result['exit']==0 else 'failed'
        if name=='workspace-tests':
            rows=re.findall(rb'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored',data)
            record['suites']=len(rows)
            for label,i in [('passed',1),('failed',2),('ignored',3)]:record[label]=sum(int(r[i]) for r in rows)
        if name=='owned-result-original' and result['reason']=='exit' and result['exit']==1 and b'shadow-builtin:reject' in data:record['status']='known-superseded-negative';record['passed']=False
        if name in ('asan-leaks','asan-roots-leaks') and result['reason']=='exit' and result['exit']==2:record['status']='blocked'
        checks.append(record);(out/'verification.json').write_text(json.dumps(report,indent=2)+'\n')
        print('finish',name,record['status'],result['exit'],flush=True)
    return int(any(r['status']=='failed' for r in checks))

if __name__=='__main__':raise SystemExit(main())
