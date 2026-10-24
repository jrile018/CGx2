#!/usr/bin/env python3
"""Revalidate retained observations without rewriting their original disposition."""
import argparse
import copy
import gzip
import hashlib
import json
from pathlib import Path
import sys
from unittest.mock import patch

import common
from common import HERE, ROOT, Journal, digest, evidence
from provenance import git
import validation
from validation import command_errors, inventory_errors, sentinel_status


def retained_inventory():
    return {str(p.relative_to(HERE)): digest(p) for directory in ('evidence', 'hardening-evidence')
            for p in sorted((HERE / directory).rglob('*')) if p.is_file()}


def sentinel_code(name):
    return 'CGE006' if name in ('package-value-python', 'package-error-python') else 'CGT095'


def archive_audit(directory):
    records = json.loads((directory / 'manifest.json').read_text())
    errors = []
    for record in records:
        path = directory / record['archive']; compressed = path.read_bytes(); raw = gzip.decompress(compressed)
        if (len(compressed) != record['compressed_bytes']
                or hashlib.sha256(compressed).hexdigest() != record['compressed_sha256']
                or len(raw) != record['bytes'] or hashlib.sha256(raw).hexdigest() != record['sha256']):
            errors.append(record['archive'])
    return len(records), errors


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output-dir', type=Path, required=True)
    p.add_argument('--reviewed-base', required=True)
    args = p.parse_args(); j = Journal(args.output_dir)
    try:
        before = retained_inventory()
        (j.output / 'retained-evidence.json').write_text(json.dumps(before, indent=2) + '\n')
        j.report['retained_files'] = len(before)
        archives = ROOT / 'plans/release-completion/s2b-ordinary-memory-review/evidence/finalization-combined'
        count, errors = archive_audit(archives)
        j.check('coordinator-archives-exact', count == 536 and not errors, count=count, errors=errors)
        old_validation = {'__file__': str(HERE / 'validation.py')}
        exec(git('show', args.reviewed_base + ':crates/cgx2-syntax/ordinary-parity-repair-evidence/validation.py'), old_validation)
        old_common = {'__file__': str(HERE / 'common.py')}
        exec(git('show', args.reviewed_base + ':crates/cgx2-syntax/ordinary-parity-repair-evidence/common.py'), old_common)
        sentinel = j.output / 'diagnostic-control-sentinel'; sentinel.write_bytes(b'prior artifact\n')
        differential = []
        for name, text in [('duplicate', 'CGE006: unsupported\n' * 2),
                           ('mixed', 'CGXC005: failure\nCGE006: unsupported\n'),
                           ('prose', 'CGXC005: unrelated mentions CGE006\n')]:
            row = j.run('adversarial-' + name, [sys.executable, '-c', f'import sys;sys.stderr.write({text!r});sys.exit(1)'])
            old = old_validation['sentinel_status'](row, sentinel, b'prior artifact\n', 'CGE006')
            new = sentinel_status(row, sentinel, b'prior artifact\n', 'CGE006')
            differential.append({'name': name, 'input': row, 'old': old, 'new': new})
            j.check('differential-' + name, old == (True, True) and new == (False, True))
        for name, code in [('timeout', 'import time;time.sleep(3)'),
                           ('signal', 'import os,signal;os.kill(os.getpid(),signal.SIGTERM)'),
                           ('overflow', 'print("x"*4096)')]:
            argv = [sys.executable, '-c', code]
            actual = evidence(common.supervise(argv, cwd=str(ROOT), env=j.env,
                              limits=common.process_limits, cap=128, timeout=0.1))
            actual.update(name=name, argv=argv, cwd=str(ROOT))
            outcomes = {}
            for label, factory in [('old', old_common['Journal']), ('new', Journal)]:
                sub = factory(j.output / (name + '-' + label))
                sub.required_commands = {name}; sub.report['commands'] = [copy.deepcopy(actual)]
                sub.check('independent-proof', True)
                if label == 'old':
                    with patch.object(validation, 'inventory_errors', old_validation['inventory_errors']):
                        status = sub.finish()
                else:
                    status = sub.finish()
                outcomes[label] = {'exit': status, 'complete': sub.report['complete']}
            differential.append({'name': name, 'input': actual, **outcomes})
            j.check('differential-' + name, outcomes == {'old': {'exit': 0, 'complete': True},
                                                                       'new': {'exit': 1, 'complete': False}})
        j.report['old_new_differential'] = differential
        retained = []; diagnostics = []; commands = 0
        for path in sorted((HERE / 'hardening-evidence').rglob('report.json.gz')):
            report = json.loads(gzip.decompress(path.read_bytes()))
            if not isinstance(report.get('commands'), list): continue
            required = report.get('required_inventory', {})
            errors = inventory_errors(report, set(required.get('commands', [])), set(required.get('checks', [])))
            retained.append({'path': str(path.relative_to(HERE)), 'sha256': digest(path),
                'original_complete': report.get('complete'), 'original_passed': report.get('passed'),
                'original_failed': report.get('failed'), 'new_validation_errors': errors,
                'original_exceptions': report.get('exceptions'),
                'failed_checks': [r for r in report.get('checks', []) if r.get('passed') is not True]})
            for row in report['commands']:
                commands += 1
                if '--force' not in row.get('argv', []): continue
                code = sentinel_code(row['name'])
                valid, _ = sentinel_status(row, sentinel, b'prior artifact\n', code)
                argv = row['argv']; output = Path(argv[argv.index('--out') + 1])
                available = output.is_file()
                diagnostics.append({'report': str(path.relative_to(HERE)), 'name': row['name'],
                    'diagnostic_valid': valid, 'actual_sentinel_path': str(output),
                    'actual_sentinel_available': available,
                    'actual_sentinel_preserved': output.read_bytes() == b'prior artifact\n' if available else None,
                    'original_preservation_checks': [r for r in report.get('checks', [])
                        if r['name'] in (row['name'] + '-preserved', row['name'].removesuffix('-python') + '-python-preservation')]})
        j.report['retained_reports'] = retained
        j.report['retained_command_count'] = commands
        j.report['retained_sentinel_observations'] = diagnostics
        j.check('retained-diagnostics-valid', len(diagnostics) == 14 and all(r['diagnostic_valid'] for r in diagnostics), count=len(diagnostics))
        j.check('no-invalid-available-retained-sentinel', all(r['actual_sentinel_preserved'] is not False for r in diagnostics))
        j.check('no-new-invalid-complete-retained-outcome', not any(r['original_complete'] is True and r['new_validation_errors'] for r in retained))
        j.check('retained-evidence-unchanged', before == retained_inventory())
        j.report['limitations'] = ['Original stale candidate labels are retained verbatim; diagnostics/outcomes audited, not relabeled.',
            'Unavailable old sentinel paths are unobserved; original preservation checks retained, no fresh byte-preservation claim.']
    except Exception:
        j.exception()
    return j.finish()


if __name__ == '__main__':
    raise SystemExit(main())
