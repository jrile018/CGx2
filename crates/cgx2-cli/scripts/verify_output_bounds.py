#!/usr/bin/env python3
"""Capped product checks for the S2b interpreter output repair; no timings."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import shlex
import signal
import subprocess
import tempfile

CAP = 1_048_576
CRATE = Path(__file__).resolve().parents[1]
FIXTURE = CRATE / 'tests/fixtures/output-bound/package'


def boundary_source(size, package):
    name = 'Long' + 'x' * 596
    prefix = 'out.main.' if package else ''
    items = ', '.join([f'{prefix}{name}(x: 1)'] * 1600)
    empty = f'{prefix}Outer(xs: [{items}], pad: )'
    pad = 'x' * (size - len(empty.encode()))
    expected = f'{prefix}Outer(xs: [{items}], pad: {pad})\n'.encode()
    source = ('module out.main\n' if package else '') + (
        f'record {name}:\n    x: i64\nrecord Outer:\n'
        f'    xs: List<{name}>\n    pad: String\nfn main() -> Outer:\n'
        f'    let v = {name}(x: 1)\n    Outer(xs: [{", ".join(["v"] * 1600)}], pad: "{pad}")\n'
    )
    return source, expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--release-only', action='store_true')
    parser.add_argument('--build-address-space-mib', type=int, choices=[2048, 4096], default=2048)
    args = parser.parse_args()
    binary = args.binary.resolve()
    rows = []

    with tempfile.TemporaryDirectory(prefix='cgx2-output-product-') as temporary:
        scratch = Path(temporary)
        rustc = subprocess.run(['rustup', 'which', 'rustc'], capture_output=True,
                               text=True, timeout=10, check=True).stdout.strip()
        wrapper = scratch / 'rustc-bounded-linker'
        # The host's lld default thread pool exhausts the capped address space.
        # Use the pinned compiler directly and keep the linker to two threads.
        wrapper.write_text('#!/bin/sh\nexec ' + shlex.quote(rustc)
                           + ' -Awarnings -Clink-arg=-Wl,--threads=2 "$@"\n')
        wrapper.chmod(0o755)

        def run(label, command, expected=None, error=None, build=False, independent=False):
            # Bounds apply to the complete child process tree through inheritance.
            # Build processes need a larger virtual address space than executions.
            def limits():
                resource.setrlimit(resource.RLIMIT_AS, ((args.build_address_space_mib if build else 512) * 1024**2,) * 2)
                resource.setrlimit(resource.RLIMIT_CPU, ((60 if build else 15),) * 2)
                resource.setrlimit(resource.RLIMIT_FSIZE, (16 * 1024**2,) * 2)
                resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

            with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
                try:
                    child = subprocess.Popen([str(x) for x in command], cwd=scratch,
                                             stdout=stdout, stderr=stderr, preexec_fn=limits,
                                             start_new_session=True)
                    code = child.wait(timeout=120 if build else 20)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
                    code = 'timeout'
                stdout.seek(0)
                stderr.seek(0)
                out, err = stdout.read(), stderr.read()
            passed = ((code == 0 and (expected is None or out == expected)) if error is None
                      else (isinstance(code, int) and code > 0 and not out and error.encode() in err))
            row = dict(label=label, command=[str(x) for x in command],
                       status='passed' if passed else 'failed', independent_target=independent,
                       returncode=code, stdout_bytes=len(out),
                       stdout_sha256=hashlib.sha256(out).hexdigest(),
                       stderr_bytes=len(err), stderr=err[:2048].decode(errors='replace'))
            rows.append(row)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(dict(checks=rows), indent=2) + '\n')
            print(f'{row["status"]}: {label}', flush=True)
            return passed

        def package_at(root, source):
            (root / 'src').mkdir(parents=True)
            (root / 'src/main.cg').write_text(source)
            (root / 'cgx2.toml').write_text(
                'schema_version = 1\nname = "out"\nentry = "src/main.cg"\n'
                'sources = ["src/main.cg"]\n')

        # Reproduce the exact reviewed package and a standalone admitted equivalent.
        reviewed = (FIXTURE / 'src/main.cg').read_text()
        standalone = scratch / 'reviewed.cg'
        standalone.write_text(reviewed.split('\n', 1)[1])
        for label, source in [('reviewed-package', FIXTURE), ('reviewed-standalone', standalone)]:
            if not args.release_only:
                run(label + ':interpreter', [binary, 'run', source], error='CGR010')
            for release in ([True] if args.release_only else [False, True]):
                executable = scratch / (label + ('-release' if release else '-debug'))
                command = [binary, 'build', source, '--out', executable, '--rustc', wrapper]
                if release:
                    command.append('--release')
                if run(label + ':build-' + str(release), command, build=True, independent=True):
                    run(label + ':native-' + str(release), [executable], error='CGR010', independent=True)
            if not args.release_only:
                emitted = scratch / (label + ('.pyz' if source.is_dir() else '.py'))
                if run(label + ':emit-python', [binary, 'emit', source, '--target', 'python', '--out', emitted], independent=True):
                    run(label + ':python', ['python3', emitted], error='CGR010', independent=True)

        if not args.release_only:
            for package in [False, True]:
                for size in [CAP - 1, CAP, CAP + 1]:
                    label = f'{"package" if package else "standalone"}-{size}'
                    source, expected = boundary_source(size, package)
                    path = scratch / label
                    if package:
                        package_at(path, source)
                    else:
                        path = path.with_suffix('.cg')
                        path.write_text(source)
                    run(label + ':interpreter', [binary, 'run', path],
                        expected=expected if size <= CAP else None, error='CGR010' if size > CAP else None)

            nested = scratch / 'unicode-nested.cg'
            nested.write_text('record Boxed:\n    text: String\nvariant Reply:\n'
                              '    Item(Array<List<Boxed>>)\nfn main() -> String:\n'
                              '    let text = "é\\n\\\"\\\\🙂"\n'
                              '    let result = Reply.Item(array([[Boxed(text: text)]]))\n'
                              '    concat(show(result), show(length(text)))\n')
            nested_expected = 'Reply.Item(Array[[Boxed(text: é\n"\\🙂)]])5\n'.encode()
            run('unicode-nested:interpreter', [binary, 'run', nested], expected=nested_expected)
            for release in [False, True]:
                executable = scratch / ('unicode-nested-' + str(release))
                command = [binary, 'build', nested, '--out', executable, '--rustc', wrapper]
                if release:
                    command.append('--release')
                if run('unicode-nested:build-' + str(release), command, build=True, independent=True):
                    run('unicode-nested:native-' + str(release), [executable], expected=nested_expected, independent=True)
            emitted = scratch / 'unicode-nested.py'
            if run('unicode-nested:emit-python', [binary, 'emit', nested, '--target', 'python', '--out', emitted], independent=True):
                run('unicode-nested:python', ['python3', emitted], expected=nested_expected, independent=True)

            # Record the independent qualified-standalone admission failure honestly.
            run('qualified-standalone-admission', [binary, 'run', FIXTURE / 'src/main.cg'],
                error='CGR010', independent=True)

            # Rejected compile/emit must preserve an existing requested artifact.
            bad = scratch / 'bad.cg'
            bad.write_text('fn main() -> i64:\n    true\n')
            for target in ['native', 'rust', 'python']:
                artifact = scratch / ('preserve-' + target)
                artifact.write_bytes(b'existing artifact\n')
                command = ([binary, 'build', bad, '--out', artifact, '--force'] if target == 'native'
                           else [binary, 'emit', bad, '--target', target, '--out', artifact, '--force'])
                run('rejected-' + target, command, error='CGT003')
                rows[-1]['artifact_preserved'] = artifact.read_bytes() == b'existing artifact\n'
                if not rows[-1]['artifact_preserved']:
                    rows[-1]['status'] = 'failed'

    report = dict(base='b595724e1b086c604220cc5b165c141e65a0531f',
                  fixture_sha256=hashlib.sha256((FIXTURE / 'src/main.cg').read_bytes()).hexdigest(),
                  limits=dict(execution_address_space_mib=512, execution_cpu_seconds=15,
                              execution_wall_seconds=20, build_address_space_mib=args.build_address_space_mib,
                              build_cpu_seconds=60, build_wall_seconds=120, file_bytes=16 * 1024**2),
                  toolchain=os.environ.get('RUSTUP_TOOLCHAIN'), release_only=args.release_only, checks=rows)
    report['native_compiler'] = dict(path=rustc, flags=['-Awarnings', '-Clink-arg=-Wl,--threads=2'])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    return 1 if any(row['status'] != 'passed' for row in rows) else 0


if __name__ == '__main__':
    raise SystemExit(main())
