#!/usr/bin/env python3
"""Final hashed red/green proof; rustc only, canonical freeze untouched."""
import json
from pathlib import Path
import shutil
import sys
from common import ROOT, Journal, digest, normal

j = Journal(Path(sys.argv[1]))
b = Path(sys.argv[2]).resolve()
try:
    stage = j.output / 'baseline-runtime'
    stage.mkdir()
    for source in (ROOT / 'crates/cgx2-runtime/src').glob('*.rs'):
        shutil.copy2(source, stage / source.name)
    row = j.run('reviewed-base-graph', ['git', 'show',
        '89464a76c9bd84a3b090ec58a14ca6be0043c6ea:crates/cgx2-runtime/src/graph.rs'])
    j.check('reviewed-base-graph', normal(row))
    (stage / 'graph.rs').write_text(row['stdout'])
    j.report['baseline_source_sha256'] = {p.name:digest(p) for p in stage.glob('*.rs')}
    source = ROOT / 'crates/cgx2-runtime/tests/ordinary_recovery_roots.rs'
    j.report['test_source_sha256'] = digest(source)
    row = j.run('baseline-library-build', ['rustc', '--edition=2021', '--crate-name=cgx2_runtime',
        '--crate-type=rlib', '--cfg=feature="fault-injection"', '-C', 'metadata=recovery_red_control',
        stage / 'lib.rs', '-o', stage / 'libcgx2_runtime.rlib'])
    j.check('baseline-library-build', normal(row))
    j.report['baseline_library_sha256'] = digest(stage / 'libcgx2_runtime.rlib')
    canonical_before = digest(b / 'libcgx2_runtime.rlib')
    for name, library, expected in [('red', stage / 'libcgx2_runtime.rlib', 101),
                                     ('green', b / 'libcgx2_runtime.rlib', 0)]:
        artifact = j.output / name
        row = j.run(name + '-test-build', ['rustc', '--edition=2021', '--test',
            '--cfg=feature="fault-injection"', '-D', 'warnings', '-C', 'link-arg=-Wl,--threads=2',
            source, '--extern', f'cgx2_runtime={library}', '-o', artifact])
        j.check(name + '-test-build', normal(row))
        j.report.setdefault('test_artifact_sha256', {})[name] = digest(artifact)
        row = j.run(name + '-test-run', [artifact, '--nocapture'])
        j.check(name + '-expected-result', normal(row, expected) and
                ('2 passed; 2 failed' if name == 'red' else '4 passed; 0 failed') in row['stdout'])
    j.check('canonical-runtime-unchanged', canonical_before == digest(b / 'libcgx2_runtime.rlib'))
    j.report['canonical_runtime_sha256'] = canonical_before
    j.report['scope'] = 'Direct runtime control only; reviewed base graph plus unchanged runtime files, final identical test source, separate rlib. No Cargo or canonical artifact mutation.'
except Exception:
    j.exception()
raise SystemExit(j.finish())
