#!/usr/bin/env python3
"""Required Cargo checks, then final build and immutable artifact freeze."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output-dir',type=Path,required=True)
    args=p.parse_args()
    out=args.output_dir.resolve();out.mkdir(parents=True,exist_ok=False)
    env=dict(os.environ,RUSTUP_TOOLCHAIN='1.98.1',CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(ROOT/'target/release-ordinary-memory'),TMPDIR=str(ROOT/'target/tmp-ordinary-memory'),LIBRARY_PATH='/tmp/cgx2-wave22-lib',PYTHONDONTWRITEBYTECODE='1')
    rows=[]
    commands=[('workspace',['cargo','test','--workspace','--all-features','--offline','--locked','--no-fail-fast']),('clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--offline','--locked','--','-D','warnings']),('fmt',['cargo','fmt','--all','--check']),('diff',['git','diff','--check']),('evidence-selftests',[sys.executable,str(HERE/'test_run.py')]),('build',['cargo','build','--workspace','--all-features','--offline','--locked']),('freeze',[sys.executable,str(HERE.parent/'managed-core-repair-evidence/freeze.py'),'--binary-dir',str(ROOT/'target/release-ordinary-memory/debug'),'--output-dir',str(out/'frozen'),'--manifest',str(out/'frozen.json')])]
    for name,argv in commands:
        log=out/(name+'.log');started=time.monotonic()
        with log.open('wb') as stream:
            result=subprocess.run(argv,cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT)
        data=log.read_bytes();rows.append({'name':name,'argv':argv,'exit':result.returncode,'signal':-result.returncode if result.returncode<0 else None,'log':log.name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'duration_seconds_for_test_supervision':time.monotonic()-started})
        (out/'verification.json').write_text(json.dumps({'schema_version':1,'environment':{k:env[k] for k in ['RUSTUP_TOOLCHAIN','CARGO_BUILD_JOBS','CARGO_TARGET_DIR','TMPDIR','LIBRARY_PATH']},'commands':rows,'ordinary_memory_complete':False},indent=2)+'\n')
        print(json.dumps({'name':name,'exit':result.returncode}),flush=True)
        if result.returncode:return 1
    paths = {ROOT/'Cargo.toml', ROOT/'Cargo.lock'}
    paths.update(ROOT.glob('rust-toolchain*'))
    paths.update((ROOT/'crates').glob('*/Cargo.toml'))
    paths.update((ROOT/'crates').glob('*/build.rs'))
    paths.update((ROOT/'crates').glob('*/src/**/*.rs'))
    files = {str(path.relative_to(ROOT)):hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(paths) if path.is_file()}
    aggregate = hashlib.sha256(json.dumps(files,sort_keys=True,separators=(',',':')).encode()).hexdigest()
    (out/'compiler-source.json').write_text(json.dumps({'schema_version':1,'reviewed_base':'0d321d3d1b0341121b8363a5a87eeaf674c20eec','files':files,'aggregate_sha256':aggregate},indent=2)+'\n')
    return 0
if __name__=='__main__':raise SystemExit(main())
