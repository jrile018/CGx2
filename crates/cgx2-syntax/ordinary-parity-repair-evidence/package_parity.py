#!/usr/bin/env python3
"""Actual package values/errors outside repository, with the same frozen candidate."""
import argparse
import json
from pathlib import Path
import shutil
from common import HERE, Journal, digest, normal, observed, stable
from run import byte_span, primary, roots_ok, frozen_hashes

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary-dir',type=Path,required=True)
p.add_argument('--historical-product',type=Path,required=True)
p.add_argument('--standalone-root',type=Path,required=True)
p.add_argument('--output-dir',type=Path,required=True)
a=p.parse_args();j=Journal(a.output_dir);b=a.binary_dir.resolve();before=frozen_hashes(b)
try:
    outside=a.standalone_root.resolve()/j.output.name;outside.mkdir(parents=True,exist_ok=False)
    row=j.run('outside-repository',['git','rev-parse','--is-inside-work-tree'],cwd=outside)
    j.check('outside-repository',normal(row,128))
    if not normal(row,128):raise ValueError('Standalone path is inside a repository')
    wrapper=j.output/'rustc-two-threads';wrapper.write_text('#!/bin/sh\nexec rustc -C link-arg=-Wl,--threads=2 "$@"\n');wrapper.chmod(0o700)
    for name,original in [('package-value',a.historical_product/'package'),('package-error',HERE/'fixtures/packages/runtime-error')]:
        source=j.output/name;shutil.copytree(original,source)
        j.report.setdefault('source_inputs',{})[name]={p.name:digest(p) for p in source.iterdir() if p.is_file()}
        expected_error=('CGR015',byte_span(source/'lib.cg','false')) if name=='package-error' else None
        checked=j.run(name+'-check',[b/'cgx2','--diagnostic-format=json','check',source]);j.check(name+'-check',normal(checked))
        reference=j.run(name+'-interpreter',[b/'cgx2','--diagnostic-format=json','run',source]);memory=observed(reference)
        def verify(tag,row):
            j.check(tag+'-result',normal(row,1 if expected_error else 0) and
                    (primary(row)==expected_error if expected_error else row.get('stdout')=='2\n'),
                    expected_error=expected_error,actual_error=primary(row))
            j.check(tag+'-cleanup',roots_ok(observed(row)),observation=observed(row))
            j.check(tag+'-deterministic-memory',memory is not None and stable(observed(row))==stable(memory))
        verify(name+'-interpreter',reference)
        for mode in ('debug','release','standalone'):
            tag=name+'-'+mode;artifact=j.output/tag
            if mode=='standalone':
                emitted=outside/(name+'.rs')
                row=j.run(tag+'-emit',[b/'cgx2c','emit-rust','--package',source,'--out',emitted]);j.check(tag+'-emit',normal(row))
                if not normal(row):continue
                built=j.run(tag+'-build',[wrapper,'--edition=2021',emitted,'-o',artifact],cwd=outside)
            else:
                argv=[b/'cgx2c','build','--package',source,'--out',artifact,'--rustc',wrapper]
                if mode=='release':argv.append('--release')
                built=j.run(tag+'-build',argv)
            j.check(tag+'-build',normal(built))
            if normal(built):
                if mode=='standalone':
                    relocated=outside/tag;shutil.copy2(artifact,relocated);artifact=relocated
                j.report.setdefault('artifacts',{})[tag]={'path':str(artifact),'sha256':digest(artifact)}
                actual=j.run(tag+'-run',[artifact],cwd=outside);verify(tag,actual)
        sentinel=j.output/(name+'.py');sentinel.write_bytes(b'prior artifact\n')
        row=j.run(name+'-python',[b/'cgx2c','emit-python','--package',source,'--out',sentinel,'--force'])
        j.check(name+'-python-rejection',normal(row,1) and 'CGE006' in row.get('stderr',''))
        j.check(name+'-python-preservation',sentinel.read_bytes()==b'prior artifact\n')
except Exception:
    j.exception()
finally:
    j.report['frozen_before']=before;j.report['frozen_after']=frozen_hashes(b)
    j.check('frozen-unchanged',j.report['frozen_before']==j.report['frozen_after'])
raise SystemExit(j.finish())
