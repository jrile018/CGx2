#!/usr/bin/env python3
"""Real guard-entry source/target parity. Ordinary memory remains incomplete."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
# Failure marker, occurrence (zero based), and expected allocation error.
CASES = {
    'nested-zero': ('no_gc(', 1, 'BudgetExceeded'),
    'outer-zero': ('no_gc(', 1, 'BudgetExceeded'),
    'inner-zero': None,
    'outer-one-under': ('no_gc(', 1, 'BudgetExceeded'),
    'deep-exact': None,
    'deep-outer-under': ('no_gc(', 2, 'BudgetExceeded'),
    'deep-inner-under': ('no_gc(', 2, 'BudgetExceeded'),
    'call-exact': None,
    'call-outer-under': ('no_gc(', 1, 'BudgetExceeded'),
    'call-inner-under': ('no_gc(', 1, 'BudgetExceeded'),
    'callee-zero': ('no_gc(', 1, 'BudgetExceeded'),
    'caller-zero': ('no_gc(', 1, 'BudgetExceeded'),
    'branch-loop-zero': ('no_gc(', 1, 'BudgetExceeded'),
    'body-failure': ('let local', 0, 'BudgetExceeded'),
    'legacy-exact': None,
    'legacy-one-under': ('let value', 0, 'BudgetExceeded'),
    'legacy-nested-zero': None,
}


def deterministic(value):
    if isinstance(value, dict):
        return {k: deterministic(v) for k, v in value.items()
                if k not in {'pause', 'session', 'file'}}
    if isinstance(value, list):
        return [deterministic(v) for v in value]
    return value


def expected_span(source, spec):
    """Independent indentation/line oracle for these fixed guard/let fixtures."""
    marker, occurrence, _ = spec
    lines = source.splitlines(keepends=True)
    matches = [i for i, line in enumerate(lines) if line.lstrip().startswith(marker)]
    index = matches[occurrence]
    indent = len(lines[index]) - len(lines[index].lstrip())
    start = sum(len(line.encode()) for line in lines[:index]) + indent
    last = index
    end = sum(len(line.encode()) for line in lines[:index + 1])
    if marker == 'no_gc(':
        for i in range(index + 1, len(lines)):
            if not lines[i].strip():
                continue
            if len(lines[i]) - len(lines[i].lstrip()) <= indent:
                end = sum(len(line.encode()) for line in lines[:i]) + len(lines[i]) - len(lines[i].lstrip())
                break
            last = i
        else:
            end = len(source.encode())
    else:
        end = sum(len(line.encode()) for line in lines[:last + 1])
    return {'start': start, 'end': end}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary_dir.resolve() / 'cgx2'
    results = []
    reports = {}
    runtime_hashes = {}
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='graph-guards-', dir=os.environ['TMPDIR']) as directory:
        work = Path(directory)

        def command(label, argv, *, exit_code=0, code=None, env=None):
            completed = subprocess.run([str(a) for a in argv], cwd=work, env=env,
                                       capture_output=True, text=True, timeout=120)
            passed = (completed.returncode == exit_code
                      and (code is None or code in completed.stderr))
            results.append({'label': label, 'command': [str(a) for a in argv],
                            'returncode': completed.returncode, 'passed': passed,
                            'stdout': completed.stdout, 'stderr': completed.stderr})
            if not passed:
                raise AssertionError(f'{label}: {completed.returncode}: {completed.stderr}')
            return completed

        def require(label, condition):
            results.append({'label': label, 'passed': bool(condition)})
            if not condition:
                raise AssertionError(label)

        runtime_dir = work / 'runtime'
        runtime_dir.mkdir()
        for source in sorted((ROOT / 'crates/cgx2-runtime/src').rglob('*.rs')):
            relative = source.relative_to(ROOT / 'crates/cgx2-runtime/src')
            target = runtime_dir / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(source.read_bytes())
            digest = hashlib.sha256(source.read_bytes()).hexdigest()
            require(f'authenticate runtime {relative}', hashlib.sha256(target.read_bytes()).hexdigest() == digest)
            runtime_hashes[str(relative)] = digest
        runtime = work / 'libcgx2_runtime.rlib'
        command('compile authenticated standalone runtime', ['rustc', '--edition=2021',
                '--crate-name', 'cgx2_runtime', '--crate-type=rlib', runtime_dir / 'lib.rs', '-o', runtime])

        for name, spec in CASES.items():
            source = HERE / f'{name}.cg'
            failure = spec is not None
            code = 'CGR013' if failure else None
            command(f'{name}: check', [binary, 'check', source])
            run = command(f'{name}: interpreter', [binary, 'run', source],
                          exit_code=int(failure), code=code)
            report = command(f'{name}: interpreter report', [binary, 'memory',
                'failure-report' if failure else 'report', source] + ([] if failure else ['--format=json']),
                exit_code=int(failure), code=code)
            report = json.loads(report.stdout)
            memory = report['memory'] if failure else report
            reports[name] = {'interpreter': report}
            require(f'{name}: zero collections', memory['totals']['collection_count'] == 0)
            if failure:
                require(f'{name}: independent failure span', report['span'] == expected_span(source.read_text(), spec))
                require(f'{name}: error kind', spec[2] in run.stderr)
                require(f'{name}: clean source unwind', report['final_unwind']['runtime_root_registrations'] == 0
                        and report['cleanup_errors'] == [] and report['rollback_preserved'] is True)
            else:
                require(f'{name}: closed storage', memory['totals']['committed_bytes'] == 0)

            rust = work / f'{name}.rs'
            command(f'{name}: emit Rust', [binary, 'emit', source, '--target', 'rust', '--out', rust])
            for profile in ['debug', 'release', 'standalone']:
                native = work / f'{name}-{profile}'
                if profile == 'standalone':
                    command(f'{name}: standalone build', ['rustc', '--edition=2024', rust,
                            '--extern', f'cgx2_runtime={runtime}', '-o', native])
                else:
                    command(f'{name}: {profile} build', [binary, 'build', source, '--out', native]
                            + (['--release'] if profile == 'release' else []))
                report_path = work / f'{name}-{profile}.json'
                env = dict(os.environ, CGX2_MEMORY_REPORT=str(report_path))
                native_run = command(f'{name}: {profile} run', [native], exit_code=int(failure), code=code, env=env)
                native_report = json.loads(report_path.read_text())
                reports[name][profile] = native_report
                require(f'{name}: {profile} memory parity', deterministic(native_report['memory']) == deterministic(memory))
                if failure:
                    require(f'{name}: {profile} diagnostic parity', native_report['diagnostic_code'] == code
                            and native_report['span'] == report['span'] and spec[2] in native_run.stderr)
                    for field in ['operation_failure', 'final_unwind', 'cleanup_errors', 'rollback_preserved']:
                        require(f'{name}: {profile} {field} parity', deterministic(native_report[field]) == deterministic(report[field]))
                else:
                    require(f'{name}: {profile} success', native_report['status'] == 'ok' and native_run.stderr == '')
            python = work / f'{name}.py'
            python.write_bytes(b'preserve prior artifact\n')
            command(f'{name}: Python rejection', [binary, 'emit', source, '--target', 'python',
                    '--out', python, '--force'], exit_code=1, code='CGE001')
            require(f'{name}: Python preserves artifact', python.read_bytes() == b'preserve prior artifact\n')

    evidence = {'scope': 'native graph guards only; ordinary memory and release remain incomplete',
                'python_diagnostic_contract_gap': {
                    'required': 'CGE006', 'observed': 'CGE001',
                    'status': 'incomplete: frozen legacy Python emitter is outside lane ownership',
                    'normal_exit_and_artifact_preservation': 'checked for all fixtures'},
                'source_cases': len(CASES), 'execution_routes': ['interpreter', 'debug', 'release', 'standalone'],
                'runtime_source_sha256': runtime_hashes,
                'fixture_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(HERE.glob('*.cg'))},
                'passed': sum(row['passed'] for row in results), 'checks': results, 'reports': reports}
    output.write_text(json.dumps(evidence, indent=2) + '\n')
    print(f"{evidence['passed']} checks passed across {len(CASES)} source fixtures")


if __name__ == '__main__':
    main()
