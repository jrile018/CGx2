#!/usr/bin/env python3
"""Reproducible product acceptance, without timings or external services."""
import argparse
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
NEGATIVE = {
    'call-arity': ('fn id<T>(value: T) -> T:\n    value\nfn main() -> i64:\n    id<i64, Bool>(1)\n', 'CGT031'),
    'duplicate-binder': ('fn id<T, T>(value: T) -> T:\n    value\n', 'CGS113'),
    'constraint': ('fn f<T: Unknown>(x: T) -> T:\n    x\n', 'CGT039'),
    'unsatisfied': ('fn eq<T: Eq>(x: T) -> Bool:\n    x == x\nfn bad<U>(x: U) -> Bool:\n    eq(x)\n', 'CGT039'),
    'illegal-operation': ('fn bad<T: Eq>(x: T) -> T:\n    x + x\n', 'CGT003'),
    'missing-show': ('fn bad<T: Eq>(x: T) -> String:\n    show(x)\n', 'CGT039'),
    'arity': ('record Box<T>:\n    value: T\nfn main() -> Unit:\n    let x: Box<i64, Bool> = Box(value: 1)\n', 'CGT031'),
    'unknown-binder': ('fn bad<T>(x: U) -> T:\n    x\n', 'CGT002'),
    'ambiguous': ('fn make<T>() -> Option<T>:\n    Option<T>.None()\nfn main() -> Unit:\n    let x = make()\n', 'CGT040'),
    'conflict': ('fn same<T>(a: T, b: T) -> T:\n    a\nfn main() -> i64:\n    same(1, true)\n', 'CGT003'),
    'nominal': ('record Box<T>:\n    value: T\nfn main() -> Box<Bool>:\n    Box<i64>(value: 1)\n', 'CGT003'),
    'private': ('record Hidden:\n    value: i64\npub fn leak<T>(x: T) -> List<Hidden>:\n    []\n', 'CGT022'),
    'recursive-shape': ('record Loop<T>:\n    next: Loop<T>\n', 'CGT030'),
    'polymorphic-recursion': ('fn expand<T>(x: T) -> i64:\n    expand<List<T>>([x])\nfn main() -> i64:\n    expand(1)\n', 'CGT030'),
    'mixed': ('fn main() -> Unit:\n    let xs = [1, true]\n', 'CGT003'),
    'empty': ('fn main() -> Unit:\n    let xs = []\n', 'CGT040'),
    'unavailable': ('fn main() -> absent.Box<i64>:\n    1\n', 'CGT002'),
    'owned-element': ('fn bad(values: List<owned<i64>>) -> Unit:\n    return\n', 'CGT002'),
    'array-growth': ('fn main() -> Unit:\n    let a: Array<i64> = [1]\n    let b = append(a, 2)\n', 'CGT039'),
    'collection-size': ('fn main() -> Unit:\n    let xs = [' + ','.join('1' for _ in range(65537)) + ']\n', 'CGT030'),
}
RUNTIME = {'runtime-depth.cg': 'CGR011', 'runtime-overflow.cg': 'CGR005', 'runtime-division.cg': 'CGR006',
           'runtime-contract.cg': 'CGR015', 'runtime-work.cg': 'CGR010'}

def implementation_hash():
    tree = hashlib.sha256()
    for crate in ['cgx2-syntax', 'cgx2-sema', 'cgx2-package', 'cgx2-api-index', 'cgx2-cli', 'cgx2c', 'cgx2-emit-rust', 'cgx2-emit-python', 'cgx2-notebook-kernel']:
        for path in sorted((ROOT / 'crates' / crate).rglob('*')):
            if path.is_file() and path.suffix in {'.rs', '.txt', '.toml'}:
                tree.update(str(path.relative_to(ROOT)).encode())
                tree.update(b'\0')
                tree.update(path.read_bytes())
                tree.update(b'\0')
    return tree.hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary-dir', type=Path, default=ROOT / 'target/release-s2b-foundation/debug')
    parser.add_argument('--output', type=Path, default=HERE / 'report.json')
    options = parser.parse_args()
    os.environ.setdefault('RUSTUP_TOOLCHAIN', '1.98.1')
    os.environ.setdefault('CARGO_BUILD_JOBS', '2')
    binary = (options.binary_dir / 'cgx2').resolve()
    records = []
    with tempfile.TemporaryDirectory(prefix='cgx2-s2b-', dir=os.environ.get('TMPDIR')) as directory:
        scratch = Path(directory).resolve()
        def run(label, command, expected=None, error=None):
            completed = subprocess.run([str(x) for x in command], cwd=scratch, text=True,
                                       capture_output=True, timeout=120, env=os.environ.copy())
            passed = ((completed.returncode == 0 and completed.stdout == expected) if error is None
                      else completed.returncode != 0 and error in completed.stderr)
            records.append({'name': label, 'passed': passed, 'exit_code': completed.returncode,
                            'stdout': completed.stdout, 'error_class': error,
                            'stderr': completed.stderr[-12000:] if not passed else ''})
            if not passed:
                raise AssertionError(f'{label}: {completed.returncode}\n{completed.stdout}\n{completed.stderr[-12000:]}')
            return completed
        expected = json.loads((HERE / 'expected.json').read_text())
        for fixture, output in expected.items():
            source = HERE / fixture
            run(f'{fixture}:interpreter', [binary, 'run', source], output)
            for release in [False, True]:
                executable = scratch / (source.stem + ('-release' if release else '-debug'))
                command = [binary, 'build', source, '--out', executable]
                if release:
                    command.append('--release')
                result = subprocess.run([str(x) for x in command], cwd=scratch, capture_output=True, text=True, timeout=120)
                if result.returncode:
                    raise AssertionError(f'{fixture}:native build\n{result.stderr}')
                run(f'{fixture}:native-{"release" if release else "debug"}', [executable], output)
            emitted = scratch / (source.stem + ('.pyz' if source.is_dir() else '.py'))
            result = subprocess.run([str(binary), 'emit', str(source), '--target', 'python', '--out', str(emitted)], cwd=scratch, capture_output=True, text=True, timeout=120)
            if result.returncode:
                raise AssertionError(result.stderr)
            run(f'{fixture}:python', ['python3', emitted], output)
        for fixture, code in RUNTIME.items():
            source = HERE / fixture
            run(f'{fixture}:interpreter', [binary, 'run', source], error=code)
            for release in [False, True]:
                executable = scratch / (source.stem + ('-release' if release else '-debug'))
                command = [binary, 'build', source, '--out', executable]
                if release:
                    command.append('--release')
                subprocess.run([str(x) for x in command], cwd=scratch, check=True, capture_output=True, timeout=120)
                run(f'{fixture}:native-{"release" if release else "debug"}', [executable], error=code)
            emitted = scratch / (source.stem + '.py')
            subprocess.run([str(binary), 'emit', str(source), '--target', 'python', '--out', str(emitted)], cwd=scratch, check=True, capture_output=True, timeout=120)
            run(f'{fixture}:python', ['python3', emitted], error=code)
        for name, (source, code) in NEGATIVE.items():
            path = scratch / (name + '.cg')
            path.write_text(source)
            run(f'{name}:check', [binary, 'check', path], error=code)
            for target in ['rust', 'python', 'native']:
                artifact = scratch / (name + '-' + target + '.out')
                artifact.write_bytes(b'preserved artifact\n')
                command = [binary, 'build', path, '--out', artifact, '--force'] if target == 'native' else [binary, 'emit', path, '--target', target, '--out', artifact, '--force']
                run(f'{name}:{target}:preserves-artifact', command, error=code)
                assert artifact.read_bytes() == b'preserved artifact\n'
        for name, code in [('package-constraint', 'CGT039'), ('package-private', 'CGT022'), ('package-import', 'CGT008')]:
            package = scratch / name
            shutil.copytree(HERE / 'package', package)
            entry = package / 'src/main.cg'
            content = entry.read_text()
            if name == 'package-constraint':
                content += '\nfn bad<U>(value: U) -> U effects() caps():\n    foundation.left.identity<U>(value)\n'
            elif name == 'package-private':
                left = package / 'src/left.cg'
                left.write_text(left.read_text() + '\nrecord Secret<T>:\n    value: T\n')
                content = content.replace('foundation.left.Box<i64>(value: 42)', 'foundation.left.Secret<i64>(value: 42)')
            else:
                content = content.replace('import foundation.right\n', '')
            entry.write_text(content)
            run(f'{name}:check', [binary, 'check', package], error=code)
            for target in ['rust', 'python', 'native']:
                artifact = scratch / (name + '-' + target + '.out')
                artifact.write_bytes(b'preserved artifact\n')
                command = [binary, 'build', package, '--out', artifact, '--force'] if target == 'native' else [binary, 'emit', package, '--target', target, '--out', artifact, '--force']
                run(f'{name}:{target}:preserves-artifact', command, error=code)
                assert artifact.read_bytes() == b'preserved artifact\n'
        source = HERE / 'package'
        one = subprocess.run([str(binary), 'api', '--json', str(source)], cwd=scratch, capture_output=True, check=True).stdout
        two = subprocess.run([str(binary), 'api', '--json', str(source)], cwd=scratch, capture_output=True, check=True).stdout
        assert one == two
        api = json.loads(one)
        # The original fixture calls generics only from private main. Public
        # metadata must omit those calls without removing internal evidence.
        assert all(not m['api'].get('instantiations') for m in api['modules'])
        public_package = scratch / 'public-api-package'
        shutil.copytree(source, public_package)
        public_entry = public_package / 'src/main.cg'
        public_entry.write_text(public_entry.read_text().replace('fn main()', 'pub fn main()'))
        public_one = subprocess.run([str(binary), 'api', '--json', str(public_package)], cwd=scratch, capture_output=True, check=True).stdout
        public_two = subprocess.run([str(binary), 'api', '--json', str(public_package)], cwd=scratch, capture_output=True, check=True).stdout
        assert public_one == public_two
        public_api = json.loads(public_one)
        instances = [i for m in public_api['modules'] for i in m['api'].get('instantiations', [])]
        assert {i['callee'] for i in instances} == {'foundation.left.identity', 'foundation.left.equal'}
        assert all(i['owner'] == 'foundation.main.main' for i in instances)
        assert all(i['type_arguments'] == ['foundation.left.Box<i64>'] for i in instances)
        records.append({'name': 'checked-api-deterministic-public-instantiations', 'passed': True})
        syntax = subprocess.run([str(binary), 'api', '--json', '--syntax-only', str(HERE / 'package/src/left.cg')], cwd=scratch, capture_output=True, check=True).stdout
        data = json.loads(syntax)
        assert data['semantics_status'] == 'syntax-only'
        assert data['records'][0]['type_parameters'][0]['constraints'] == ['Eq', 'Show']
        records.append({'name': 'syntax-api-generic-status', 'passed': True})
        rust = scratch / 'standalone.rs'
        subprocess.run([str(binary), 'emit', str(HERE / 'application.cg'), '--target', 'rust', '--out', str(rust)], cwd=scratch, capture_output=True, check=True)
        executable = scratch / 'standalone-rust'
        subprocess.run(['rustc', '--edition=2024', str(rust), '-o', str(executable)], cwd=scratch, capture_output=True, check=True)
        run('standalone-emitted-rust', [executable], '42\n')
    report = {'schema_version': 1, 'stage': 'S2b foundation only', 'release_complete': False,
              'passed': len(records), 'failed': 0, 'checks': records,
              'fixture_sha256': {str(p.relative_to(HERE)): hashlib.sha256(p.read_bytes()).hexdigest()
                                 for p in sorted(HERE.rglob('*')) if p.suffix in {'.cg', '.toml'}},
              'implementation_tree_sha256': implementation_hash(),
              'runner_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'external_acceptance': 'unrun; remaining S2b source integrations open'}
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    print(f'{len(records)} checks passed; foundation only')

if __name__ == '__main__':
    main()
