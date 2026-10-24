#!/usr/bin/env python3
"""Replay unchanged runners and separately assert this repair's acceptance."""
import argparse
import json
import os
from pathlib import Path
import shutil
import sys
from common import HERE, ROOT, Journal, normal, digest

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--verification', type=Path, required=True)
p.add_argument('--output-dir', type=Path, required=True)
p.add_argument('--standalone-root', type=Path, required=True)
a = p.parse_args()
j = Journal(a.output_dir)
b = a.verification.resolve() / 'frozen'
scratch = j.output
outside = a.standalone_root.resolve()

try:
    outside.mkdir(parents=True, exist_ok=False)
    manifest = json.loads((b.parent / 'frozen.json').read_text())
    # API's older manifest layout is a separate immutable view. It does not add
    # metadata to the raw matrix's binary directory or alter its expectations.
    api = j.output / 'api-view'
    api.mkdir()
    for name in dict(manifest['binary_sha256'], **manifest['probe_library_sha256']):
        (api / name).symlink_to(b / name)
    (api / 'deps').symlink_to(b / 'deps', target_is_directory=True)
    (api / 'manifest.json').write_text(json.dumps({'artifact_sha256': dict(
        manifest['binary_sha256'], **manifest['probe_library_sha256'])}, indent=2) + '\n')
    base = [sys.executable, HERE / 'replay.py', 'ordinary-parity-repair-evidence']
    for name, argv in [
        ('matrix', [*base, 'run.py', '--binary-dir', b, '--output-dir', scratch / 'matrix-01', '--standalone-root', outside]),
        ('auto', [*base, 'auto.py', '--binary-dir', b, '--output-dir', scratch / 'auto-01', '--standalone-root', outside]),
        ('historical', [*base, 'replay.py', '--binary-dir', b, '--output-dir', scratch / 'historical-01']),
        ('api', [sys.executable, HERE / 'replay.py', 'ordinary-api-repair-evidence', 'run.py', '--binary-dir', api, '--output-dir', scratch / 'api-01']),
        ('admission', [sys.executable, HERE.parent / 'ordinary-admission-repair-evidence/run.py', '--binary-dir', b, '--output-dir', outside / 'admission-01']),
        ('package', [*base, 'package_parity.py', '--binary-dir', b, '--output-dir', scratch / 'package-01', '--historical-product', scratch / 'historical-01/product', '--standalone-root', outside]),
        ('review', [*base, 'review.py', '--binary-dir', b, '--output-dir', scratch / 'review-01', '--matrix-report', scratch / 'matrix-01/report.json']),
        ('recovery', [sys.executable, HERE / 'run.py', '--binary-dir', b, '--output-dir', scratch / 'recovery-01', '--matrix-report', scratch / 'matrix-01/report.json', '--standalone-root', outside]),
    ]:
        row = j.run(name, argv, suite=True)
        # Keep genuine unrelated matrix failures red. They are described and
        # checked separately by the final audit, never reclassified as passes.
        j.check(name, normal(row))
        if name == 'admission' and (outside / 'admission-01').exists():
            shutil.copytree(outside / 'admission-01', scratch / 'admission-01',
                            ignore=shutil.ignore_patterns('outside'))
    j.report['frozen_artifact_sha256_after'] = {name: digest(b / name) for name in
        dict(manifest['binary_sha256'], **manifest['probe_library_sha256'])}
    j.check('frozen-artifacts-unchanged', j.report['frozen_artifact_sha256_after'] ==
        dict(manifest['binary_sha256'], **manifest['probe_library_sha256']))
    dependencies = {p.name: digest(p) for p in (b / 'deps').iterdir()
                    if p.is_file() and p.suffix in ('.rlib', '.rmeta', '.so')}
    j.check('frozen-dependencies-unchanged', dependencies == manifest['dependency_sha256'])
    sources = json.loads((b.parent / 'source.json').read_text())
    j.check('compiler-source-unchanged', sources == {p: digest(ROOT / p) for p in sources})
except Exception:
    j.exception()
raise SystemExit(j.finish())
