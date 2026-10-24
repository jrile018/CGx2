#!/usr/bin/env python3
"""Bounded real-source layout reporting checks; ordinary memory remains incomplete."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]


def stable(value):
    if isinstance(value, dict):
        return {key: stable(item) for key, item in value.items()
                if key not in {'pause', 'session', 'file', 'native_report_schema_version'}}
    if isinstance(value, list):
        return [stable(item) for item in value]
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    cases = json.loads((HERE / 'expected.json').read_text())
    checks = []
    reports = {}

    def require(label, condition):
        checks.append({'label': label, 'passed': bool(condition)})
        if not condition:
            raise AssertionError(label)

    with tempfile.TemporaryDirectory(prefix='report-layout-', dir=os.environ['TMPDIR']) as directory:
        work = Path(directory)
        base_env = os.environ.copy()
        base_env.pop('CGX2_MEMORY_REPORT', None)

        def command(label, command_args, exit_code=0, env=None):
            command_args = [str(item) for item in command_args]
            result = subprocess.run(command_args, cwd=work, env=env or base_env,
                                    capture_output=True, text=True, timeout=120)
            # An expected diagnostic exit is exactly 1, never a signal or any
            # other nonzero code. The old product runner only checked != 0.
            checks.append({'label': label, 'command': command_args,
                           'returncode': result.returncode, 'expected_exit': exit_code,
                           'passed': result.returncode == exit_code,
                           'stdout': result.stdout, 'stderr': result.stderr})
            if result.returncode != exit_code:
                raise AssertionError(f'{label}: {result.returncode}: {result.stderr}')
            return result

        runtime_dir = work / 'runtime'
        shutil.copytree(ROOT / 'crates/cgx2-runtime/src', runtime_dir)
        library = work / 'libcgx2_runtime.rlib'
        command('standalone runtime build', ['rustc', '--edition=2021', '--crate-name',
                'cgx2_runtime', '--crate-type', 'rlib', runtime_dir / 'lib.rs', '-o', library])

        for name, expected in cases.items():
            source = HERE / f'{name}.cg'
            label = f'{name}: '
            exit_code = expected['exit']
            command(label + 'check', [binary, 'check', source])
            plain = command(label + 'interpreter', [binary, 'run', source], exit_code)
            observed = command(label + 'source report', [binary, 'memory',
                'failure-report' if exit_code else 'report', source]
                + ([] if exit_code else ['--format=json']), exit_code)
            require(label + 'report leaves interpreter diagnostic unchanged',
                    observed.stderr == plain.stderr)
            source_report = json.loads(observed.stdout)
            source_memory = source_report['memory'] if exit_code else source_report
            if exit_code:
                span = expected['span']
                source_error = (f"{expected['code']}: {source}:{span['start']}..{span['end']}: "
                                f"{expected['message']}\n")
                native_span = expected['native_span']
                native_error = (f"{expected['code']} at {native_span['start']}..{native_span['end']}: "
                                f"{expected['message']}\n")
                require(label + 'exact source diagnostic', plain.stderr == source_error)
                require(label + 'exact source span', source_report['span'] == span)
                operation = source_report['operation_failure']
                require(label + 'exact source roots and edges',
                        operation['rooted_handles'] == expected['rooted_handles']
                        and operation['rooted_edges'] == expected['rooted_edges'])
                cleanup = source_report['final_unwind']
                require(label + 'zero source roots after cleanup',
                        all(cleanup[key] == 0 for key in ['rooted_handles', 'rooted_edges',
                            'runtime_rooted_handles', 'runtime_root_registrations'])
                        and source_report['cleanup_errors'] == [])
                reports[name] = {'source': stable(source_report)}
            else:
                require(label + 'success result and cleanup',
                        plain.stdout == '' and plain.stderr == ''
                        and source_memory['totals']['committed_bytes'] == 0)
                reports[name] = {'source': stable(source_memory)}

            for profile in ['debug', 'release', 'standalone']:
                native = work / f'{name}-{profile}'
                if profile == 'standalone':
                    rust = work / f'{name}.rs'
                    command(label + 'emit rust', [binary, 'emit', source, '--target', 'rust', '--out', rust])
                    command(label + 'standalone rust build', ['rustc', '--edition=2021', rust,
                            '--extern', f'cgx2_runtime={library}', '-o', native])
                else:
                    command(label + profile + ' build', [binary, 'build', source, '--out', native]
                            + (['--release'] if profile == 'release' else []))
                unobserved = command(label + profile + ' without report', [native], exit_code)
                report_path = work / f'{name}-{profile}.json'
                env = base_env.copy()
                env['CGX2_MEMORY_REPORT'] = str(report_path)
                observed = command(label + profile + ' with report', [native], exit_code, env=env)
                require(label + profile + ' report leaves values/diagnostic unchanged',
                        (observed.stdout, observed.stderr) == (unobserved.stdout, unobserved.stderr))
                report = json.loads(report_path.read_text())
                require(label + profile + ' complete deterministic memory parity',
                        stable(report['memory']) == stable(source_memory))
                if exit_code:
                    require(label + profile + ' exact native diagnostic', observed.stderr == native_error)
                    require(label + profile + ' failure lifecycle parity',
                            stable(report) == stable({**source_report, 'span': expected['native_span']}))
                    reports[name][profile] = stable(report)
                else:
                    require(label + profile + ' success result unchanged',
                            observed.stdout == '' and observed.stderr == '')
                    reports[name][profile] = stable(report['memory'])

    files = [HERE / 'expected.json', HERE / 'run.py', ROOT / 'crates/cgx2-cli/src/memory.rs',
             ROOT / 'crates/cgx2-emit-rust/src/memory_runtime.rs.txt']
    files += sorted(HERE.glob('*.cg'))
    files += sorted((ROOT / 'crates/cgx2-runtime/src').glob('*.rs'))
    result = {'status': 'passed', 'scope': 'legacy explicit source layout reporting',
              'ordinary_memory_checkpoint_complete': False, 'release_complete': False,
              'source_cases': len(cases), 'native_profiles': ['debug', 'release', 'standalone'],
              'passed': len(checks), 'checks': checks, 'deterministic_reports': reports,
              'sha256': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                         for path in files}}
    output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value for key, value in result.items()
                      if key not in {'checks', 'deterministic_reports', 'sha256'}}))


if __name__ == '__main__':
    main()
