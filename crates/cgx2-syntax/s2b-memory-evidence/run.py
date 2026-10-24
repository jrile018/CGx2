#!/usr/bin/env python3
"""Bounded graph/payload source evidence; ordinary-function checkpoint stays incomplete."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
CASES = {
    'chunked-cycle': None,
    'initialized-object': None,
    'graph-no-gc': None,
    'graph-budget-error': 'CGR013',
    'graph-byte-error': 'CGR015',
}
RUNTIME = ['lib.rs', 'budget.rs', 'heap.rs', 'graph.rs', 'scope.rs',
           'fault.rs', 'owned.rs', 'region.rs', 'report.rs']


def deterministic(report):
    if isinstance(report, dict):
        return {k: deterministic(v) for k, v in report.items()
                if k not in {'pause', 'session', 'file'}}
    if isinstance(report, list):
        return [deterministic(v) for v in report]
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    options = parser.parse_args()
    binary = options.binary_dir.resolve() / 'cgx2'
    results = []
    options.output = options.output.resolve()
    options.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='cgx2-memory-', dir=os.environ.get('TMPDIR')) as directory:
        work = Path(directory)

        def command(label, args, *, success=True, code=None, env=None):
            completed = subprocess.run([str(arg) for arg in args], cwd=work,
                                       env=env, capture_output=True, text=True, timeout=120)
            accepted = ((completed.returncode == 0) == success
                        and (code is None or code in completed.stderr))
            results.append({'label': label, 'command': [str(arg) for arg in args],
                            'returncode': completed.returncode, 'passed': accepted,
                            'stdout': completed.stdout, 'stderr': completed.stderr})
            if not accepted:
                raise AssertionError(f'{label}: {completed.returncode}: {completed.stderr}')
            return completed

        def require(label, condition):
            results.append({'label': label, 'passed': bool(condition)})
            if not condition:
                raise AssertionError(label)

        runtime_dir = work / 'runtime'
        runtime_dir.mkdir()
        runtime_hash = hashlib.sha256()
        for name in RUNTIME:
            data = (ROOT / 'crates/cgx2-runtime/src' / name).read_bytes()
            (runtime_dir / name).write_bytes(data)
            runtime_hash.update(name.encode() + b'\0' + data + b'\0')
        library = work / 'libcgx2_runtime.rlib'
        command('standalone authenticated runtime', ['rustc', '--edition=2021', '--crate-name',
                'cgx2_runtime', '--crate-type', 'rlib', runtime_dir / 'lib.rs', '-o', library])

        for name, code in CASES.items():
            source = HERE / f'{name}.cg'
            command(f'{name}: check', [binary, 'check', source])
            command(f'{name}: interpreter', [binary, 'run', source], success=code is None, code=code)
            source_report = command(f'{name}: source report', [binary, 'memory',
                'failure-report' if code else 'report', source] + ([] if code else ['--format=json']),
                success=code is None, code=code)
            source_report = json.loads(source_report.stdout)
            source_memory = source_report['memory'] if code else source_report
            require(f'{name}: source cleanup counters', source_memory['totals']['committed_bytes'] == 0
                    if code is None else source_report['final_unwind']['runtime_root_registrations'] == 0)
            if name == 'graph-no-gc':
                require('source no_gc counts', source_memory['totals']['collection_count'] == 0)
            for release in [False, True]:
                profile = 'release' if release else 'debug'
                native = work / f'{name}-{profile}'
                command(f'{name}: native {profile} build', [binary, 'build', source, '--out', native]
                        + (['--release'] if release else []))
                report_path = work / f'{name}-{profile}.json'
                env = os.environ.copy()
                env['CGX2_MEMORY_REPORT'] = str(report_path)
                command(f'{name}: native {profile} run', [native], success=code is None, code=code, env=env)
                report = json.loads(report_path.read_text())
                require(f'{name}: native {profile} report parity',
                        deterministic(report['memory']) == deterministic(source_memory))
                if code:
                    require(f'{name}: native {profile} error cleanup',
                            report['final_unwind']['runtime_root_registrations'] == 0
                            and report['cleanup_errors'] == [])
            rust = work / f'{name}.rs'
            command(f'{name}: emit rust', [binary, 'emit', source, '--target', 'rust', '--out', rust])
            standalone = work / f'{name}-standalone'
            command(f'{name}: standalone rust build', ['rustc', '--edition=2021', rust,
                '--extern', f'cgx2_runtime={library}', '-o', standalone])
            command(f'{name}: standalone rust run', [standalone], success=code is None, code=code)
            python = work / f'{name}.py'
            python.write_bytes(b'preserve this artifact\n')
            command(f'{name}: python rejection', [binary, 'emit', source, '--target', 'python',
                    '--out', python, '--force'], success=False)
            require(f'{name}: python artifact preservation', python.read_bytes() == b'preserve this artifact\n')

        original = (HERE / 'chunked-cycle.cg').read_text()
        negative_cases = {
            'too-many-refs': (original.replace('1048576, 65536', '1048576, 65537'), 'CGM002'),
            'too-many-bytes': (original.replace('1048576, 65536', '1048577, 65536'), 'CGM002'),
            'bad-byte': (original.replace('1048551, 41', '1048551, 256'), 'CGM002'),
            'wrong-layout': (original.replace('memory.set_graph_reference', 'memory.set_reference'), 'CGM002'),
            'slot-bounds': (original.replace('graph, 65535, graph', 'graph, 65536, graph'), 'CGM002'),
            'collect-in-guard': ((HERE / 'graph-no-gc.cg').read_text().replace(
                '        memory.drop(graph)', '        memory.collect(a)\n        memory.drop(graph)'), 'CGO004'),
        }
        for name, (source, diagnostic) in negative_cases.items():
            path = work / f'{name}.cg'
            path.write_text(source)
            command(f'{name}: source rejection', [binary, 'check', path], success=False, code=diagnostic)
            artifact = work / f'{name}.rs'
            artifact.write_bytes(b'preserve rejection\n')
            command(f'{name}: native rejection', [binary, 'emit', path, '--target', 'rust',
                    '--out', artifact, '--force'], success=False, code=diagnostic)
            require(f'{name}: artifact preservation', artifact.read_bytes() == b'preserve rejection\n')

    summary = {'status': 'passed', 'checkpoint_complete': False,
               'scope': 'explicit graph/payload source operations only',
               'ordinary_ownership_lowering': 'incomplete',
               'ordinary_default_managed_values': 'incomplete',
               'source_recoverable_transfers': 'incomplete',
               'runtime_sha256': runtime_hash.hexdigest(),
               'source_sha256': {name: hashlib.sha256((HERE / f'{name}.cg').read_bytes()).hexdigest() for name in CASES},
               'source_cases': len(CASES), 'rejected_source_cases': len(negative_cases),
               'passed': sum(row['passed'] for row in results), 'checks': results}
    options.output.write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps({k: v for k, v in summary.items() if k not in {'checks', 'source_sha256'}}))


if __name__ == '__main__':
    main()
