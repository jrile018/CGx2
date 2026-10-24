#!/usr/bin/env python3
"""Compile emitted Rust in /tmp, without repository libraries or working directory."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
spec=importlib.util.spec_from_file_location('ordinary_memory_runner',HERE/'run.py')
runner=importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--product-dir',type=Path,required=True)
    p.add_argument('--output-dir',type=Path,required=True)
    args=p.parse_args();product=args.product_dir.resolve();out=args.output_dir.resolve()
    out.mkdir(parents=True,exist_ok=False)
    work=Path(tempfile.mkdtemp(prefix='cgx2-ordinary-memory-standalone-',dir='/tmp'))
    report={'schema_version':1,'working_directory':str(work),'outside_repository':not work.is_relative_to(ROOT),'commands':[],'checks':[]}
    baseline=json.loads((product/'product.json').read_text())
    env=dict(os.environ,RUSTUP_TOOLCHAIN='1.98.1',TMPDIR=str(work),CGX2_MEMORY_EVIDENCE='1',PYTHONDONTWRITEBYTECODE='1')
    def run(name,argv,extra=None):
        record={'name':name,'argv':[str(a) for a in argv],'cwd':str(work),**runner.evidence(runner.supervise([str(a) for a in argv],cwd=str(work),env=dict(env,**(extra or {})),limits=runner.limits,cap=2*1024**2,timeout=180))}
        report['commands'].append(record);return record
    def check(name,passed):report['checks'].append({'name':name,'passed':bool(passed)})
    check('actual-cwd-outside-repository',report['outside_repository'])
    for name,expected in [('region','2\n'),('guard','42\n'),('session','42\n'),('copy-cycle','42\n'),('nested-guards','42\n'),('package','2\n')]:
        source=work/(name+'.rs');shutil.copy2(product/'outside'/(name+'.rs'),source)
        binary=work/name
        built=run(name+'-compile',['rustc','--edition=2021','-C','link-arg=-Wl,--threads=2',source,'-o',binary])
        check(name+'-compile',runner.normal(built))
        if runner.normal(built):
            actual=run(name+'-run',[binary])
            check(name+'-value',runner.normal(actual) and actual['stdout']==expected)
            expected_observation=runner.observed(next(r for r in baseline['commands'] if r['name']==name+'-interpreter'))
            actual_observation=runner.observed(actual)
            check(name+'-memory',actual_observation is not None and runner.stable(actual_observation)==runner.stable(expected_observation))
    fault=work/'session-fault'
    built=run('session-fault-compile',['rustc','--edition=2021','-C','link-arg=-Wl,--threads=2','--cfg=feature="fault-injection"',work/'session.rs','-o',fault],{'CGX2_TEST_FAIL_RESERVE':'1'})
    check('session-fault-compile',runner.normal(built))
    if runner.normal(built):
        actual=run('session-fault-run',[fault]);check('session-fault-value',runner.normal(actual) and actual['stdout']=='-10\n')
        check('session-fault-unavailable-metrics',runner.observed(actual)=={'session_entered':False,'metrics':None})
    report['passed']=sum(c['passed'] for c in report['checks']);report['failed']=sum(not c['passed'] for c in report['checks'])
    (out/'standalone.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'passed':report['passed'],'failed':report['failed']}))
    return int(bool(report['failed']))
if __name__=='__main__':raise SystemExit(main())
