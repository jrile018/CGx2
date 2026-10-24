#!/usr/bin/env python3
"""Force Auto safepoints under nested guards, then check deferred collection."""
import argparse
from pathlib import Path
import shutil
from common import HERE, Journal, normal, observed, stable, digest
from run import roots_ok, frozen_hashes
from validation import auto_inventory, sentinel_status
from integrity import audit
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary-dir',type=Path,required=True)
p.add_argument('--output-dir',type=Path,required=True)
p.add_argument('--standalone-root',type=Path,required=True)
a=p.parse_args();j=Journal(a.output_dir);b=a.binary_dir.resolve();before=frozen_hashes(b)
j.required_commands, j.required_checks = auto_inventory()
try:
    if not audit(j, b, 'before'):raise ValueError('frozen inputs changed')
    outside=a.standalone_root.resolve()/j.output.name;outside.mkdir(parents=True,exist_ok=False)
    check=j.run('outside',['git','rev-parse','--is-inside-work-tree'],cwd=outside);j.check('outside',normal(check,128))
    if not normal(check,128):raise ValueError('inside repository')
    wrapper=j.output/'rustc-two-threads';wrapper.write_text('#!/bin/sh\nexec rustc -C link-arg=-Wl,--threads=2 "$@"\n');wrapper.chmod(0o700)
    probe=j.output/'probe'
    build=j.run('direct-build',[wrapper,'--edition=2021',HERE/'auto_probe.rs','--extern',f'cgx2_runtime={b}/libcgx2_runtime.rlib','-L',f'dependency={b}/deps','-o',probe]);j.check('direct-build',normal(build))
    if normal(build):
        row=j.run('direct-runtime-phases',[probe]);j.check('direct-runtime-phases',normal(row) and row.get('stdout')=='protected=0 exit=0 next_auto=1 manual=0 roots=0\n',evidence_kind='direct-library')
    source=j.output/'auto-guard.cg';shutil.copy2(HERE/'fixtures/auto-guard.cg',source)
    j.report['source_sha256']=digest(source);j.report['probe_sha256']=digest(HERE/'auto_probe.rs')
    checked=j.run('source-check',[b/'cgx2','--diagnostic-format=json','check',source]);j.check('source-check',normal(checked))
    reference=j.run('interpreter',[b/'cgx2','run',source]);memory=observed(reference)
    def verify(tag,row):
        value=observed(row)
        j.check(tag+'-value',normal(row) and row.get('stdout')=='42\n')
        j.check(tag+'-roots',roots_ok(value))
        j.check(tag+'-collections',value is not None and value.get('memory',{}).get('totals',{}).get('collection_count')==0)
        j.check(tag+'-guards',value is not None and value.get('guard_entries')==value.get('guard_exits')==2)
        j.check(tag+'-parity',value is not None and memory is not None and stable(value)==stable(memory))
    verify('interpreter',reference)
    for mode in ('debug','release','standalone'):
        artifact=j.output/mode
        if mode=='standalone':
            emitted=outside/'auto.rs';row=j.run(mode+'-emit',[b/'cgx2c','emit-rust',source,'--out',emitted]);j.check(mode+'-emit',normal(row))
            if not normal(row):continue
            built=j.run(mode+'-build',[wrapper,'--edition=2021',emitted,'-o',artifact],cwd=outside)
        else:
            argv=[b/'cgx2c','build',source,'--out',artifact,'--rustc',wrapper]
            if mode=='release':argv.append('--release')
            built=j.run(mode+'-build',argv)
        j.check(mode+'-build',normal(built))
        if normal(built):
            if mode=='standalone':
                target=outside/'auto';shutil.copy2(artifact,target);artifact=target
            j.report.setdefault('artifacts',{})[mode]={'path':str(artifact),'sha256':digest(artifact)}
            if mode=='standalone':j.report['artifacts'][mode].update(source=str(emitted),source_sha256=digest(emitted))
            j.save()
            actual=j.run(mode+'-run',[artifact],cwd=outside);verify(mode,actual)
except Exception:
    j.exception()
finally:
    audit(j, b, 'after')
    j.report['frozen_before']=before;j.report['frozen_after']=frozen_hashes(b);j.check('frozen-unchanged',j.report['frozen_before']==j.report['frozen_after'])
raise SystemExit(j.finish())
