#!/usr/bin/env python3
"""Exercise restored source names on interpreter, native and standalone targets."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import tempfile
from bounded import run

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
RUNTIME = ['lib.rs', 'budget.rs', 'heap.rs', 'graph.rs', 'scope.rs', 'fault.rs', 'owned.rs', 'region.rs', 'report.rs']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary_dir.resolve() / 'cgx2'
    args.output.parent.mkdir(parents=True, exist_ok=True)
    records = []
    report = {'schema_version': 1, 'checks': records, 'release_complete': False,
              'sources_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                                 for p in sorted((HERE / 'fixtures').glob('*.cg'))}}
    with tempfile.TemporaryDirectory(prefix='symbol-parity-', dir=os.environ['TMPDIR']) as tmp:
        work = Path(tmp)
        env = {**os.environ, 'CGX2_OWNED_EVIDENCE': '1'}

        def command(label, cmd, exit=0, contains=None):
            result = run(cmd, work, env, work / f'{len(records)}.log', timeout=120)
            output = (work / result['log']).read_text()
            result.update(label=label, output=output,
                          passed=result['exit'] == exit and not result['limit']
                          and (contains is None or contains in output))
            records.append(result)
            args.output.write_text(json.dumps(report, indent=2) + '\n')
            assert result['passed'], result
            return output

        def assertion(label, condition):
            records.append({'label': label, 'passed': bool(condition)})
            assert condition, label

        def observed(output):
            lines = output.splitlines()
            prefix = 'CGX2_OWNED_EVIDENCE '
            evidence = [json.loads(line[len(prefix):]) for line in lines if line.startswith(prefix)]
            return '\n'.join(line for line in lines if not line.startswith(prefix)), evidence

        compiler = command('pinned-rustc', ['rustup', 'which', 'rustc']).strip()
        wrapper = work / 'rustc-bounded-linker'
        wrapper.write_text('#!/bin/sh\nexec ' + shlex.quote(compiler) + ' -Clink-arg=-Wl,--threads=2 "$@"\n')
        wrapper.chmod(0o700)
        runtime = work / 'runtime'
        runtime.mkdir()
        report['runtime_inventory_sha256'] = {}
        for name in RUNTIME:
            data = (ROOT / 'crates/cgx2-runtime/src' / name).read_bytes()
            (runtime / name).write_bytes(data)
            report['runtime_inventory_sha256'][name] = hashlib.sha256(data).hexdigest()
        rlib = work / 'libcgx2_runtime.rlib'
        command('standalone-runtime', [wrapper, '--edition=2021', '--crate-name', 'cgx2_runtime', '--crate-type', 'rlib', runtime / 'lib.rs', '-o', rlib])
        for source in sorted((HERE / 'fixtures').glob('*.cg')):
            name = source.stem
            plain = name == 'plain-symbols'
            command(name + ':check', [binary, 'check', source])
            output = command(name + ':interpreter', [binary, 'run', source])
            assertion(name + ':value', observed(output)[0] == '42')
            if plain:
                assertion(name + ':copied-route', 'CGX2_OWNED_EVIDENCE' not in output)
            else:
                observation = json.loads(next(line.split(' ', 1)[1] for line in output.splitlines() if line.startswith('CGX2_OWNED_EVIDENCE ')))
                assertion(name + ':cleanup', observation['allocations'] == observation['releases'] and observation['live_bytes'] == 0)
            for release in [False, True]:
                artifact = work / f'{name}-{release}'
                command(artifact.name + ':build', [binary, 'build', source, '--out', artifact, '--rustc', wrapper] + (['--release'] if release else []))
                native = command(artifact.name + ':run', [artifact])
                assertion(artifact.name + ':parity', observed(native) == observed(output))
            emitted, standalone = work / f'{name}.rs', work / f'{name}-standalone'
            command(name + ':emit-rust', [binary, 'emit', source, '--target', 'rust', '--out', emitted])
            command(name + ':standalone-compile', [wrapper, emitted, '--edition=2024', '--extern', f'cgx2_runtime={rlib}', '-o', standalone])
            generated = command(name + ':standalone-run', [standalone])
            assertion(name + ':standalone-parity', observed(generated) == observed(output))
            python = work / f'{name}.py'
            if plain:
                command(name + ':python-emit', [binary, 'emit', source, '--target', 'python', '--out', python])
                python_output = command(name + ':python-run', ['python3', python])
                assertion(name + ':python-value', python_output == '42\n')
            else:
                python.write_bytes(b'previous artifact\n')
                command(name + ':python-reject', [binary, 'emit', source, '--target', 'python', '--out', python, '--force'], exit=1, contains='CGE006')
                assertion(name + ':python-preserved', python.read_bytes() == b'previous artifact\n')
    report['passed'] = sum(r['passed'] for r in records)
    report['failed'] = sum(not r['passed'] for r in records)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(f"symbol compatibility: {report['passed']} passed, {report['failed']} failed")


if __name__ == '__main__':
    main()
