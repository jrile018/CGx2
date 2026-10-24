#!/usr/bin/env python3
"""Final checks without Cargo or frozen artifact mutation."""
import argparse
import ast
import json
from pathlib import Path
import sys
from common import HERE, ROOT, Journal, digest, normal
from run import frozen_hashes
from integrity import audit

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--verification-dir',type=Path,required=True)
p.add_argument('--output-dir',type=Path,required=True)
p.add_argument('--historical-inventory',type=Path,required=True)
a=p.parse_args();j=Journal(a.output_dir)
try:
    audit(j, a.verification_dir/'frozen', 'final')
    previous=json.loads(a.historical_inventory.read_text())
    from finalization_audit import retained_inventory
    current=retained_inventory()
    j.check('retained-historical-evidence-immutable',current==previous)
    helpers=[HERE.parent/'owned-result-runner-evidence/supervisor.py', HERE.parent/'ordinary-memory-evidence/run.py', HERE.parent/'ordinary-memory-evidence/test_run.py', ROOT/'plans/release-completion/s2b-ordinary-memory/regressions.json']
    j.report['read_only_helper_sha256']={str(p.relative_to(ROOT)):digest(p) for p in helpers}
    j.required_commands={'primary-selftests','supplemental-selftests','adversarial-selftests','historical-selftests','diff'}
    j.required_checks.update(j.required_commands)
    for name,argv in [('primary-selftests',[sys.executable,HERE/'test_harness.py']),
                      ('supplemental-selftests',[sys.executable,HERE/'hardening-evidence/explicit_scenarios.py']),
                      ('adversarial-selftests',[sys.executable,HERE/'test_finalization.py']),
                      ('historical-selftests',[sys.executable,HERE.parent/'ordinary-memory-evidence/test_run.py']),
                      ('diff',['git','diff','--check'])]:
        row=j.run(name,argv);j.check(name,normal(row))
    for path in HERE.glob('*.py'):
        ast.parse(path.read_text(),filename=str(path))
    j.check('python-syntax',True)
    manifest=json.loads((a.verification_dir/'frozen.json').read_text())
    expected=dict(manifest['binary_sha256'],**manifest['probe_library_sha256'])
    actual=frozen_hashes(a.verification_dir/'frozen')
    j.check('all-frozen-executables-and-canonical-libraries-unchanged',actual==expected)
    j.report['frozen_after_all_runs']=actual
    original=json.loads((a.verification_dir/'source.json').read_text())
    current={p:digest(ROOT/p) for p in original}
    j.check('compiler-runtime-and-manifests-unchanged',current==original)
    # Retain exact source text for final harness checks, including later adjunct scripts.
    for path in list(HERE.glob('*.py'))+list(HERE.glob('*.rs')):
        (j.output/path.name).write_bytes(path.read_bytes())
except Exception:
    j.exception()
raise SystemExit(j.finish())
