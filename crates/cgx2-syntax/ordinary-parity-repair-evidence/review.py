#!/usr/bin/env python3
"""Replay the eight review findings without modifying another repair lane."""
import argparse
import json
from pathlib import Path
import shutil
from common import ROOT, Journal, digest, normal, observed
from run import roots_ok
from integrity import audit
from validation import inventory_errors

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary-dir',type=Path,required=True)
p.add_argument('--matrix-report',type=Path,required=True)
p.add_argument('--output-dir',type=Path,required=True)
a=p.parse_args(); j=Journal(a.output_dir); b=a.binary_dir.resolve()
j.required_commands={'OM01-admission','OM03-admission','OM02-run','OM04-run','OM05-run','OM08-public-api'}
j.required_checks={'OM01-reject-before-execution','OM03-reject-before-execution',
    'OM06-safe-runtime-copy-source-valid','OM08-functions-public-only','OM08-public-record-retained','matrix-inventory-valid'}
j.required_checks.update(f'{id}-{part}' for id in ('OM02','OM04','OM05') for part in ('value','cleanup'))
j.required_checks.update(f'OM07-{mode}-balanced-evidence' for mode in ('interpreter','debug','release','standalone'))
try:
    if not audit(j,b,'before'):raise ValueError('frozen inputs changed')
    j.toolchain()
    original=ROOT/'plans/release-completion/s2b-ordinary-memory-review/repros'
    source=j.output/'repros'; shutil.copytree(original,source)
    j.report['sources']={str(p.relative_to(source)):digest(p) for p in source.rglob('*') if p.is_file()}
    for id,name in [('OM01','shadow-collect'),('OM03','user-err-region-escape.cg')]:
        row=j.run(id+'-admission',[b/'cgx2','--diagnostic-format=json','check',source/name])
        j.check(id+'-reject-before-execution',normal(row,1),
                scope='Admission disposition only; exact source/package UTF-8 negatives are in matrix report.')
    for id,name,value in [('OM02','legacy-guard-panic.cg','42\n'),('OM04','user-borrow.cg','42\n'),
                          ('OM05','outer-root-assignment.cg','1\n')]:
        row=j.run(id+'-run',[b/'cgx2','--diagnostic-format=json','run',source/name])
        j.check(id+'-value',normal(row) and row.get('stdout')==value)
        j.check(id+'-cleanup',roots_ok(observed(row)),observation=observed(row))
    api=j.run('OM08-public-api',[b/'cgx2','api',source/'api-private.cg','--json'])
    try: data=json.loads(api.get('stdout',''))
    except ValueError: data={}
    names={r.get('name') for r in data.get('functions',[])}
    records={r.get('name') for r in data.get('records',[])}
    j.check('OM08-functions-public-only',normal(api) and names=={'exported'},actual=sorted(names))
    j.check('OM08-public-record-retained',records=={'PublicBox'},actual=sorted(records))
    matrix=json.loads(a.matrix_report.read_text())
    errors=inventory_errors(matrix,set(),set())
    j.check('matrix-inventory-valid',not errors and matrix.get('complete') is True,errors=errors)
    j.report['matrix_reference']={'path':str(a.matrix_report.resolve()),'sha256':digest(a.matrix_report)}
    direct=next(c for c in matrix['checks'] if c['name']=='direct:OM06-safe-same-region-copy')
    j.check('OM06-safe-runtime-copy-source-valid',direct['passed'],evidence_kind='direct-library',details=direct['details'])
    for mode in ('interpreter','debug','release','standalone'):
        copy=next(c for c in matrix['checks'] if c['name']==f'copy-cycle-{mode}-balanced-roots')
        j.check('OM07-'+mode+'-balanced-evidence',copy['passed'],observation=copy.get('observation'))
except Exception:
    j.exception()
finally:
    audit(j,b,'after')
raise SystemExit(j.finish())
