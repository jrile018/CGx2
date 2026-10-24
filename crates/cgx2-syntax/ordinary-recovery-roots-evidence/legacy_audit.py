#!/usr/bin/env python3
"""Derived reserve reconciliation; original legacy failures stay unchanged."""
import argparse
import json
from pathlib import Path
import re
from common import HERE, Journal, normal, stable, digest
from run import roots_ok

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary-dir', type=Path, required=True)
p.add_argument('--raw-report', type=Path, required=True)
p.add_argument('--output-dir', type=Path, required=True)
p.add_argument('--standalone-root', type=Path, required=True)
a = p.parse_args()
j = Journal(a.output_dir)
j.env['CGX2_MANAGED_EVIDENCE'] = '1'
b = a.binary_dir.resolve()
raw_before = digest(a.raw_report)
outside = a.standalone_root.resolve() / j.output.name
outside.mkdir(parents=True, exist_ok=False)


def observed(row):
    lines = [line.split(' ', 1)[1] for line in row.get('stderr', '').splitlines()
             if line.startswith(('CGX2_MANAGED_EVIDENCE ', 'CGX2_MEMORY_EVIDENCE '))]
    return json.loads(lines[0]) if len(lines) == 1 else None

try:
    row = j.run('outside', ['git', 'rev-parse', '--is-inside-work-tree'], cwd=outside)
    j.check('outside', normal(row, 128))
    if not normal(row, 128):
        raise ValueError('not an outside cwd')
    report = json.loads(a.raw_report.read_text())
    raw_failures = [c for c in report['checks'] if not c['passed']]
    j.check('original-eight-failures-retained', report['passed'] == 670 and report['failed'] == 8
            and len(raw_failures) == 8 and all(c['classification'] == 'fault-index-not-reached'
            and c['expected_classification'] == 'uncaught-runtime' for c in raw_failures))
    counts = {}
    for fixture in ('fault-graph-root', 'fault-guard'):
        source = HERE.parent / 'default-managed-evidence/repair-fixtures' / (fixture + '.cg')
        reference = j.run(fixture + '-baseline', [b / 'cgx2', 'run', source])
        j.check(fixture + '-baseline', normal(reference) and reference['stdout'] == '7\n'
                and roots_ok(observed(reference)))
        emitted = outside / (fixture + '.rs')
        row = j.run(fixture + '-emit', [b / 'cgx2c', 'emit-rust', source, '--out', emitted])
        j.check(fixture + '-emit', normal(row))
        original = emitted.read_text()
        assert original.count('self.attempts += 1;') == 1
        derived = original.replace('self.attempts += 1;', 'self.attempts += 1;\n eprintln!("RESERVE_TRACE {} {:?}", self.attempts, site);')
        derived_source = outside / (fixture + '-trace.rs')
        derived_source.write_text(derived)
        artifact = outside / (fixture + '-trace')
        built = j.run(fixture + '-trace-build', ['rustc', '-C', 'link-arg=-Wl,--threads=2', '--edition=2021',
            '--cfg=feature="fault-injection"', derived_source, '-o', artifact], cwd=outside)
        j.check(fixture + '-trace-build', normal(built))
        trace = j.run(fixture + '-trace-run', [artifact], cwd=outside)
        indices = [int(line.split()[1]) for line in trace.get('stderr', '').splitlines() if line.startswith('RESERVE_TRACE ')]
        j.check(fixture + '-trace-parity', normal(trace) and trace['stdout'] == reference['stdout']
                and observed(trace) is not None and observed(reference) is not None
                and stable(observed(trace)) == stable(observed(reference)))
        j.check(fixture + '-trace-contiguous', indices == list(range(1, len(indices) + 1)) and len(indices) >= 7)
        counts[fixture] = len(indices)
        j.report.setdefault('traces', {})[fixture] = dict(count=len(indices),
            original_source_sha256=digest(source), emitted_sha256=digest(emitted),
            derived_source_sha256=digest(derived_source), binary_sha256=digest(artifact),
            kind='diagnostic-only derived control; canonical baseline parity required')
    for outcome in report['outcomes']:
        label = outcome['label']
        matched = re.match(r'(fault-(?:graph-root|guard))-(\d+)', label)
        assert matched, label
        fixture, index = matched[1], int(matched[2])
        guarded = fixture == 'fault-guard'
        last = counts[fixture]
        if index == 1:
            expected = 'uncaught-session-entry'
        elif guarded and index == 2:
            expected = 'uncaught-guard-entry'
        elif index < last:
            expected = 'recovered-inside-expression'
        elif index == last:
            expected = 'uncaught-runtime'
        else:
            expected = 'fault-index-not-reached'
        obs = outcome['observation']
        uncaught = expected.startswith('uncaught-')
        valid = outcome['classification'] == expected and outcome['reason'] == 'exit' and outcome['signal'] is None
        valid = valid and outcome['exit'] == (1 if uncaught else 0)
        if obs is None:
            valid = valid and index == 1
        else:
            valid = valid and roots_ok(obs) and obs['memory']['totals']['collection_count'] == 0
            valid = valid and obs['recovered'] == int(expected == 'recovered-inside-expression')
        diagnostics = outcome['diagnostics']
        valid = valid and (len(diagnostics) == 1 and diagnostics[0]['code'] == 'CGR090'
            and diagnostics[0]['span'] is not None if uncaught else not diagnostics)
        j.check('derived:' + label, valid, actual=outcome['classification'], expected=expected)
    j.report['disposition'] = 'Derived validation only. Original outer135/1 and nested670/8 remain raw failures; one redundant reserve per fixture was removed by inline binding transfer.'
    j.report['original_report_sha256'] = raw_before
    j.check('original-report-unchanged', digest(a.raw_report) == raw_before)
except Exception:
    j.exception()
raise SystemExit(j.finish())
