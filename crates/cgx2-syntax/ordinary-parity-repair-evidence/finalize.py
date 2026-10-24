#!/usr/bin/env python3
"""Fresh bounded matrices against one audited compiler; product failures stay red."""
import argparse
import json
from pathlib import Path
import sys
from common import HERE, Journal, normal
from integrity import audit


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--coordinator-dir', type=Path, required=True)
    p.add_argument('--build-source-commit', required=True)
    p.add_argument('--reviewed-base', required=True)
    p.add_argument('--scratch-dir', type=Path, required=True)
    p.add_argument('--standalone-root', type=Path, required=True)
    args = p.parse_args()
    scratch = args.scratch_dir.resolve(); supplied = args.coordinator_dir.resolve()
    j = Journal(scratch / 'driver')
    names = {'audit', 'freeze', 'matrix', 'auto', 'package', 'review', 'historical', 'quality'}
    j.required_commands = set(names); j.required_checks = set(names)
    def invoke(name, script, *extra):
        row = j.run(name, [sys.executable, HERE / script, '--output-dir', scratch / name, *extra], suite=True)
        j.check(name, normal(row))
        report = scratch / name / 'report.json'
        if report.is_file():
            data = json.loads(report.read_text())
            j.report.setdefault('raw_results', {})[name] = {k: data.get(k) for k in
                ('passed', 'failed', 'complete', 'exceptions', 'coverage_errors', 'product_summary', 'historical_failures')}
            if data.get('complete') is not True:
                j.report.setdefault('prerequisite_failures', []).append('incomplete child report: ' + name)
            j.save()
        else:
            j.report.setdefault('prerequisite_failures', []).append('missing child report: ' + name)
            j.save()
        return normal(row)
    try:
        if not invoke('audit', 'finalization_audit.py', '--reviewed-base', args.reviewed_base):
            raise ValueError('retained observations or original archives failed audit')
        if not invoke('freeze', 'freeze.py', '--verification-dir', supplied / 'verification',
                      '--build-invocation', supplied / 'coordinator-invocation.json',
                      '--build-source-commit', args.build_source_commit):
            raise ValueError('candidate identity audit/freeze failed')
        binary = scratch / 'freeze/frozen'
        common = ['--binary-dir', binary, '--standalone-root', args.standalone_root]
        invoke('matrix', 'run.py', *common)
        invoke('auto', 'auto.py', *common)
        invoke('package', 'package_parity.py', *common, '--historical-product', supplied / 'historical/product')
        invoke('review', 'review.py', '--binary-dir', binary, '--matrix-report', scratch / 'matrix/report.json')
        invoke('historical', 'replay.py', *common)
        invoke('quality', 'quality.py', '--verification-dir', scratch / 'freeze',
               '--historical-inventory', scratch / 'audit/retained-evidence.json')
        audit(j, binary, 'final')
    except Exception:
        j.exception()
    return j.finish()


if __name__ == '__main__':
    raise SystemExit(main())
