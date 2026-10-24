#!/usr/bin/env python3
"""Source specialization conformance with real targets and an external cwd; no timings."""
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    artifacts = args.output_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=False)
    rows = []
    failures = []

    with tempfile.TemporaryDirectory(prefix='cgx2-specialization-cwd-', dir='/tmp') as external:
        cwd = Path(external)
        assert not cwd.is_relative_to(ROOT)

        def run(label, command, expected=None, error=None):
            command = [str(x) for x in command]
            try:
                result = subprocess.run(command, cwd=cwd, capture_output=True, text=True,
                                        timeout=120)
                passed = ((result.returncode == 0 and
                           (expected is None or result.stdout == expected)) if error is None
                          else result.returncode > 0 and error in result.stderr and not result.stdout)
                row = dict(label=label, command=command, cwd=str(cwd), passed=passed,
                           exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr)
            except (subprocess.TimeoutExpired, OSError) as exc:
                row = dict(label=label, command=command, cwd=str(cwd), passed=False, failure=str(exc))
            rows.append(row)
            if not row['passed']:
                failures.append(label)
            return row['passed']

        def check(label, condition):
            rows.append(dict(label=label, passed=bool(condition)))
            if not condition:
                failures.append(label)

        def positive(label, source, expected):
            run(label + ':check', [binary, 'check', source])
            run(label + ':interpreted', [binary, 'run', source], expected)
            for release in [False, True]:
                profile = 'release' if release else 'debug'
                executable = artifacts / (label + '-' + profile)
                command = [binary, 'build', source, '--out', executable]
                if release:
                    command.append('--release')
                if run(label + ':build-' + profile, command):
                    run(label + ':native-' + profile, [executable], expected)
            rust = artifacts / (label + '.rs')
            native = artifacts / (label + '-rust')
            if run(label + ':emit-rust', [binary, 'emit', source, '--target', 'rust', '--out', rust]):
                if run(label + ':compile-rust', ['rustc', '--edition=2021', '--crate-name', 'fixture',
                                               rust, '-o', native]):
                    run(label + ':standalone-rust', [native], expected)
            python = artifacts / (label + ('.pyz' if source.is_dir() else '.py'))
            if run(label + ':emit-python', [binary, 'emit', source, '--target', 'python', '--out', python]):
                run(label + ':python', ['python3', python], expected)
            if run(label + ':api', [binary, 'api', source, '--json']):
                check(label + ':private-metadata-omitted', 'Secret' not in rows[-1]['stdout'])

        def negative(label, source, error):
            for action in ['check', 'api', 'run']:
                run(label + ':' + action, [binary, action, source], error=error)
            for target in ['debug', 'release', 'rust', 'python']:
                artifact = artifacts / (label + '-rejected-' + target)
                sentinel = b'previous reviewed artifact\n'
                artifact.write_bytes(sentinel)
                command = ([binary, 'build', source] if target in ['debug', 'release'] else
                           [binary, 'emit', source, '--target', target])
                if target == 'release':
                    command.append('--release')
                run(label + ':reject-' + target, command + ['--out', artifact, '--force'], error=error)
                check(label + ':preserved-' + target, artifact.read_bytes() == sentinel)

        reviewed = ROOT / 'crates/cgx2-api-index/tests/fixtures'
        for label, source, expected in [
            ('qualified', HERE / 'qualified.cg', '[out.main.Long(x: é)]\n'),
            ('nested-private', HERE / 'nested-private.cg', '7\n'),
            ('package-private', HERE / 'package', '7\n'),
            ('public-interfaces', HERE / 'public-interfaces.cg', '7\n'),
            ('visibility-source', reviewed / 's2bf-visibility.cg', '7\n'),
            ('visibility-package', reviewed / 's2bf-package', '7\n'),
            ('public-source', reviewed / 's2bf-public.cg', '7\n'),
            ('public-package', reviewed / 's2bf-public-package', '7\n'),
            ('public-values', reviewed / 's2bf-public-values.cg', '7\n'),
        ]:
            positive(label, source, expected)

        collisions = artifacts / 'collisions.cg'
        collisions.write_text('module T\nrecord T:\n    value: i64\npub fn id<T>(x: T) -> T effects() caps():\n'
                              '    x\nfn main() -> i64:\n    let hidden = id<T>(T(value: 4))\n    id<i64>(7)\n')
        positive('collisions', collisions, '7\n')
        for label, declaration in [
            ('public-function', 'pub fn leak<T>(x: T) -> Secret:\n    Secret(value: 1)\n'),
            ('public-record', 'pub record Leak<T>:\n    value: Secret\n'),
            ('public-variant', 'pub variant Leak<T>:\n    Value(Secret)\n'),
        ]:
            source = artifacts / (label + '.cg')
            source.write_text('record Secret:\n    value: i64\n' + declaration)
            negative(label, source, 'CGT022')
        dishonest = artifacts / 'dishonest.cg'
        dishonest.write_text('pub fn dishonest<T>(x: T) -> i64 effects() caps():\n'
                             '    arg_count()\nfn main() -> i64:\n    dishonest<i64>(1)\n')
        negative('dishonest', dishonest, 'CGT012')
        for label, ty, error in [('import-private', 'Secret', 'CGT022'),
                                  ('import-unknown', 'Absent', 'CGT002')]:
            package = artifacts / label
            shutil.copytree(HERE / 'package', package)
            (package / 'src/main.cg').write_text('module app.main\nimport dep.library\nfn main() -> i64:\n'
                                                f'    dep.library.unused<dep.library.{ty}>(7)\n')
            negative(label, package, error)
        package = artifacts / 'dishonest-import'
        shutil.copytree(HERE / 'package', package)
        dep = package / 'src/dep.cg'
        dep.write_text(dep.read_text().replace('    x\n', '    let hidden = arg_count()\n    x\n', 1))
        negative('dishonest-import', package, 'CGT012')

    report = dict(base='c2a039db3ea0b4e457a9f2e0c7626db77094d313',
                  kind='source/target conformance; no performance or release acceptance',
                  binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  environment={k: os.environ.get(k) for k in ['RUSTUP_TOOLCHAIN', 'CARGO_BUILD_JOBS',
                               'CARGO_TARGET_DIR', 'TMPDIR', 'LIBRARY_PATH']},
                  checks=rows, failures=failures)
    (artifacts / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(f'{sum(r["passed"] for r in rows)}/{len(rows)} passed; {len(failures)} failures')
    return bool(failures)


if __name__ == '__main__':
    raise SystemExit(main())
