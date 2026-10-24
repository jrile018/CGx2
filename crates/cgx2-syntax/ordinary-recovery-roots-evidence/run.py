#!/usr/bin/env python3
"""Independent recovery checks against a previously frozen, unchanged matrix."""
import argparse
import difflib
import importlib.util
import json
from pathlib import Path
import shutil
from common import HERE, Journal, digest, normal, observed, stable

legacy = HERE.parent / 'ordinary-parity-repair-evidence'
spec = importlib.util.spec_from_file_location('original_matrix', legacy / 'run.py')
matrix = importlib.util.module_from_spec(spec)
spec.loader.exec_module(matrix)


def roots_ok(v):
    fields = ('baseline_roots', 'final_roots', 'roots_registered', 'roots_released',
              'cleanup_failures', 'guard_entries', 'guard_exits', 'recovered', 'charged_bytes')
    return (isinstance(v, dict) and all(type(v.get(k)) is int and v[k] >= 0 for k in fields)
            and v['baseline_roots'] == v['final_roots'] == v['cleanup_failures'] == 0
            and v['roots_registered'] == v['roots_released']
            and v['guard_entries'] == v['guard_exits'])


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary-dir', type=Path, required=True)
    p.add_argument('--matrix-report', type=Path, required=True)
    p.add_argument('--output-dir', type=Path, required=True)
    p.add_argument('--standalone-root', type=Path, required=True)
    a = p.parse_args()
    j = Journal(a.output_dir)
    b = a.binary_dir.resolve()
    before = matrix.frozen_hashes(b)
    outside = a.standalone_root.resolve() / j.output.name
    outside.mkdir(parents=True, exist_ok=False)
    wrapper = j.output / 'rustc-two-threads'
    wrapper.write_text('#!/bin/sh\nexec rustc -C link-arg=-Wl,--threads=2 "$@"\n')
    wrapper.chmod(0o700)
    allowed = {'-10\n', '0\n', '-1\n', '-2\n', '-3\n', '-4\n', '-5\n', '42\n'}

    def verify(tag, row, reference=None, expected=None, code=None, pre_session=False):
        j.check(tag + ':outcome', normal(row, 1 if code else 0) and
                (matrix.primary(row) is not None and matrix.primary(row)[0] == code if code
                 else row.get('stdout') == expected if expected else row.get('stdout') in allowed))
        value = observed(row)
        j.check(tag + ':cleanup', value == {'session_entered': False, 'metrics': None}
                if pre_session else roots_ok(value))
        if reference:
            j.check(tag + ':parity', row.get('stdout') == reference.get('stdout') and
                    matrix.primary(row) == matrix.primary(reference) and
                    value is not None and stable(value) == stable(observed(reference)))

    def target(name, source, reference, mode, fault=None, code=None):
        tag = f'{name}-{mode}'
        artifact = j.output / tag
        if mode == 'standalone':
            emitted = outside / (tag + '.rs')
            row = j.run(tag + '-emit', [b / 'cgx2c', 'emit-rust', source, '--out', emitted])
            j.check(tag + '-emit', normal(row))
            if not normal(row):
                return
            argv = [wrapper, '--edition=2021', emitted, '-o', artifact]
            if fault:
                argv += ['--cfg=feature="fault-injection"']
            built = j.run(tag + '-build', argv, cwd=outside,
                          extra={'CGX2_TEST_FAIL_RESERVE': str(fault)} if fault else None)
        else:
            argv = [b / 'cgx2c', 'build', source, '--out', artifact, '--rustc', wrapper]
            if mode == 'release':
                argv.append('--release')
            if fault:
                argv += ['--test-fail-reserve', str(fault)]
            built = j.run(tag + '-build', argv)
        j.check(tag + '-build', normal(built))
        if normal(built):
            if mode == 'standalone':
                relocated = outside / tag
                shutil.copy2(artifact, relocated)
                j.report.setdefault('standalone_artifacts', {})[tag] = {
                    'source': str(emitted), 'source_sha256': digest(emitted),
                    'binary': str(relocated), 'binary_sha256': digest(relocated)}
                artifact = relocated
            j.report.setdefault('artifacts', {})[tag] = {'path': str(artifact), 'sha256': digest(artifact)}
            actual = j.run(tag + '-run', [artifact], cwd=outside)
            verify(tag, actual, reference, code=code, pre_session=fault == 1)

    try:
        outside_check = j.run('outside-repository', ['git', 'rev-parse', '--is-inside-work-tree'], cwd=outside)
        j.check('outside-repository', normal(outside_check, 128))
        if not normal(outside_check, 128):
            raise ValueError('not an external cwd')
        j.report['resolved_standalone_cwd'] = str(outside.resolve())
        source = HERE / 'fixtures/sweep.cg'
        baseline = j.run('sweep-unaffected', [b / 'cgx2', 'run', source])
        verify('sweep-unaffected', baseline, expected='42\n')
        emitted = outside / 'unmodified-sweep.rs'
        row = j.run('sweep-trace-emit', [b / 'cgx2c', 'emit-rust', source, '--out', emitted])
        j.check('sweep-trace-emit', normal(row))
        original = emitted.read_text()
        needle = 'self.attempts += 1;'
        assert original.count(needle) == 1
        # Diagnostic-only derived control. The canonical source/artifacts are
        # untouched. Baseline evidence must equal the canonical interpreter.
        derived = original.replace(needle, needle + '\n eprintln!("RESERVE_TRACE {} {:?}", self.attempts, site);')
        trace_source = outside / 'derived-reserve-trace.rs'
        trace_source.write_text(derived)
        (j.output / 'trace.diff').write_text(''.join(difflib.unified_diff(original.splitlines(True), derived.splitlines(True))))
        trace_binary = j.output / 'trace'
        built = j.run('derived-trace-build', [wrapper, '--edition=2021', '--cfg=feature="fault-injection"', trace_source, '-o', trace_binary], cwd=outside)
        j.check('derived-trace-build', normal(built))
        trace = j.run('derived-unaffected-reserves', [trace_binary], cwd=outside)
        verify('derived-unaffected-reserves', trace, baseline, expected='42\n')
        reserves = [line.split(' ', 2)[1:] for line in trace.get('stderr', '').splitlines() if line.startswith('RESERVE_TRACE ')]
        indices = [int(n) for n, _ in reserves]
        j.check('trace-is-contiguous', indices == list(range(1, len(indices) + 1)) and len(indices) > 11)
        j.report['unaffected_reserve_trace'] = {'count': len(indices), 'sites': reserves,
            'kind': 'diagnostic-only derived standalone control; parity checked against canonical baseline',
            'canonical_source_sha256': digest(emitted), 'derived_source_sha256': digest(trace_source),
            'binary_sha256': digest(trace_binary)}
        data = json.loads(a.matrix_report.read_text())
        j.report['original_matrix_sha256'] = digest(a.matrix_report)
        commands = {row['name']: row for row in data['commands']}
        for index in sorted(set(indices + [10, 11, 21, len(indices) + 1, 64])):
            reference = commands[f'source-reserve-{index}']
            verify(f'independent-source-{index}', reference, pre_session=index == 1)
            for mode in ('debug', 'release', 'standalone'):
                tag = f'source-fault-{index}-{mode}-run'
                if tag in commands:
                    verify('independent-' + tag, commands[tag], reference, pre_session=index == 1)
                else:
                    target(f'extra-fault-{index}', source, reference, mode, fault=index)
        for index in range(len(indices) + 1, 65):
            row = commands[f'source-reserve-{index}']
            verify(f'unaffected-tail-{index}', row, baseline, expected='42\n')
        for source in sorted((HERE / 'fixtures').glob('*.cg')):
            if source.name == 'sweep.cg':
                continue
            name = source.stem
            code = {'copy-argument-primary': 'CGR006', 'arm-allocation': 'CGR090'}.get(name)
            reference = j.run(name + '-interpreter', [b / 'cgx2', '--diagnostic-format=json', 'run', source])
            verify(name + '-interpreter', reference, expected='42\n', code=code)
            for mode in ('debug', 'release', 'standalone'):
                target(name, source, reference, mode, code=code)
        # Preserve the unchanged stale375 failure in the original matrix. This
        # separate control uses the already reviewed six-root bookkeeping sum.
        original = (legacy / 'runtime_probe.rs').read_text()
        derived = original.replace('let copy=4*root +', 'let copy=6*root +')
        assert original.count("let copy=4*root +") + original.count("let copy=6*root +") == 1
        corrected = j.output / 'derived-copy-budget.rs'
        corrected.write_text(derived)
        probe = j.output / 'derived-copy-budget'
        built = j.run('derived-copy-budget-build', [wrapper, '--edition=2021', corrected, '--extern',
            f'cgx2_runtime={b}/libcgx2_runtime.rlib', '-L', f'dependency={b}/deps', '-o', probe])
        j.check('derived-copy-budget-build', normal(built))
        row = j.run('derived-copy-budget-controls', [probe])
        j.check('derived-copy-budget-controls', normal(row))
        j.report['derived_copy_budget'] = {'original_sha256': digest(legacy / 'runtime_probe.rs'),
            'derived_sha256': digest(corrected), 'operation_bytes': 439, 'guard_metadata_bytes': 16}
        for line in row.get('stdout', '').splitlines():
            control = json.loads(line)
            j.check('derived:' + control['name'], control['passed'], details=control['details'])
    except Exception:
        j.exception()
    finally:
        j.report['frozen_before'] = before
        j.report['frozen_after'] = matrix.frozen_hashes(b)
        j.check('frozen-unchanged', before == j.report['frozen_after'])
    return j.finish()


if __name__ == '__main__':
    raise SystemExit(main())
