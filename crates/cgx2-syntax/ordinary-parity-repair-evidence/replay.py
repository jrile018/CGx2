#!/usr/bin/env python3
"""Run the unchanged 117 checks and 18 regressions, retaining historical reconciliation."""
import argparse
import json
from pathlib import Path
import sys
import shutil
from common import HERE, Journal, normal, digest
from integrity import audit
from validation import inventory_errors

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary-dir', type=Path, required=True)
p.add_argument('--output-dir', type=Path, required=True)
p.add_argument('--standalone-root', type=Path, required=True)
a=p.parse_args()
j=Journal(a.output_dir)
j.required_commands = {'outside-repository', 'unchanged-ordinary-memory-117-and-18-regressions'}
j.required_checks = {'outside-repository', 'historical-process-exit', 'exact-regression-inventory',
                     'unchanged-117-plus19-checks', 'historical-identities', 'frozen-history-unchanged'}
try:
    if not audit(j, a.binary_dir.resolve(), 'before'):raise ValueError('frozen inputs changed')
    outside=a.standalone_root.resolve()/j.output.name
    outside.mkdir(parents=True, exist_ok=False)
    row=j.run('outside-repository', ['git','rev-parse','--is-inside-work-tree'], cwd=outside)
    j.check('outside-repository', normal(row,128))
    if not normal(row,128):raise ValueError('historical standalone cwd is inside repository')
    j.toolchain(cwd=outside)
    runner=HERE.parent/'ordinary-memory-evidence/run.py'
    j.report['historical_runner_sha256']=digest(runner)
    row=j.run('unchanged-ordinary-memory-117-and-18-regressions',
              [sys.executable,runner,'--binary-dir',a.binary_dir.resolve(),'--output-dir',outside/'product','--regressions'],suite=True)
    j.check('historical-process-exit',normal(row))
    product=outside/'product/product.json'
    if product.exists():
        data=json.loads(product.read_text())
        j.report['product_report_sha256']=digest(product)
        j.report['product_summary']={k:data[k] for k in ('passed','failed')}
        regression=[r for r in data['commands'] if r['name'].startswith('regression-')]
        specification=json.loads((HERE.parents[2]/'plans/release-completion/s2b-ordinary-memory/regressions.json').read_text())
        required={'regression-'+r['name'] for r in specification['checks']}
        j.check('exact-regression-inventory',len(regression)==18 and {r['name'] for r in regression}==required,count=len(regression))
        j.check('unchanged-117-plus19-checks',len(data['checks'])==136,count=len(data['checks']))
        errors=inventory_errors(data,required,set())
        j.check('historical-identities',not errors,errors=errors)
        j.report['historical_failures']=[r for r in data['checks'] if not r['passed']]
        j.check('frozen-history-unchanged',data['frozen_before']==data['frozen_after'])
    else:
        j.check('historical-report-present',False)
except Exception:
    j.exception()
finally:
    audit(j, a.binary_dir.resolve(), 'after')
    if 'outside' in globals() and (outside/'product').exists():
        # Retain raw reports and exact sources, without copying rebuildable executables.
        for path in (outside/'product').rglob('*'):
            if path.is_file() and path.suffix in ('.json','.cg','.toml','.py','.rs'):
                destination=j.output/'product'/path.relative_to(outside/'product')
                destination.parent.mkdir(parents=True,exist_ok=True)
                shutil.copy2(path,destination)
        executables={};emitted={}
        def record_commands(value):
            if isinstance(value,dict):
                argv=value.get('argv')
                if isinstance(argv,list) and argv and isinstance(argv[0],str):
                    executable=Path(argv[0])
                    if executable.is_absolute() and executable.is_file():
                        executables[str(executable)]={'sha256':digest(executable),'resolved':str(executable.resolve())}
                    for argument in argv:
                        if isinstance(argument,str) and argument.endswith('.rs'):
                            source=Path(argument)
                            if source.is_absolute() and source.is_file():emitted[str(source)]=digest(source)
                for child in value.values():record_commands(child)
            elif isinstance(value,list):
                for child in value:record_commands(child)
        for path in (outside/'product').rglob('*.json'):
            try:record_commands(json.loads(path.read_text()))
            except (OSError,ValueError):pass  # Raw malformed report remains retained; no pass is inferred.
        j.report['retained_historical_executable_sha256']=executables
        j.report['retained_historical_emitted_source_sha256']=emitted
        j.save()
raise SystemExit(j.finish())
