#!/usr/bin/env python3
"""Bounded repair checks; historical regressions are explicit opt-in, never edited."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import resource
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / 'owned-result-runner-evidence'))
from supervisor import supervise, evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True, help='New attempt directory')
    parser.add_argument('--only', default='build,repair-selftests,managed-product,python-syntax,rustfmt-driver,diff',
                        help='Comma-separated check names; default is the bounded repair')
    args = parser.parse_args()
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    target, temporary = ROOT / 'target/release-managed-evidence-repair', ROOT / 'target/tmp-managed-evidence-repair'
    for path in (target, temporary):
        path.mkdir(parents=True, exist_ok=True)
    env = {**os.environ, 'RUSTUP_TOOLCHAIN': '1.98.1', 'CARGO_BUILD_JOBS': '2',
           'CARGO_TARGET_DIR': str(target), 'TMPDIR': str(temporary),
           'LIBRARY_PATH': '/tmp/cgx2-wave22-lib'}
    binary, py = target / 'debug', sys.executable
    commands = [
        ('build', ['cargo', 'build', '-p', 'cgx2-cli', '-p', 'cgx2c', '--features', 'cgx2-cli/failure-injection-test', '--offline', '--locked']),
        ('repair-selftests', [py, '-m', 'unittest', 'discover', '-s', HERE, '-p', 'test_repair_runner.py']),
        ('managed-product', [py, HERE / 'run.py', '--binary-dir', binary, '--output-dir', out / 'managed-product', '--repair-only']),
        ('python-syntax', [py, '-c', 'import ast,sys; [ast.parse(open(p).read(),filename=p) for p in sys.argv[1:]]',
                           HERE / 'run.py', HERE / 'verify.py', HERE / 'repair_runner.py', HERE / 'test_repair_runner.py']),
        ('rustfmt-driver', ['rustfmt', '--check', HERE / 'fault_driver.rs']),
        ('diff', [py, '-c', 'import subprocess; subprocess.run(["git","diff","--check"],check=True); subprocess.run(["git","diff","--cached","--check"],check=True)']),
        ('fmt', ['cargo', 'fmt', '--all', '--check']),
        ('workspace-tests', ['cargo', 'test', '--workspace', '--all-features', '--offline', '--locked', '--no-fail-fast']),
        ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--offline', '--locked', '--', '-D', 'warnings']),
        ('owned-symbol195', [py, 'crates/cgx2-syntax/owned-symbol-compatibility-evidence/run.py', '--binary-dir', binary, '--output', out / 'owned-symbol195.json']),
        ('ordinary256', [py, 'crates/cgx2-syntax/ordinary-owned-evidence/run.py', '--binary-dir', binary, '--output', out / 'ordinary256.json']),
        ('foundation', [py, 'crates/cgx2-syntax/s2b-evidence/run.py', '--binary-dir', binary, '--output', out / 'foundation.json']),
        ('semantic', [py, 'crates/cgx2-sema/tests/semantic_resolution/run.py', '--binary', binary / 'cgx2', '--output', out / 'semantic.json']),
        ('specialization', [py, 'crates/cgx2-sema/tests/source_specialization/run.py', '--binary', binary / 'cgx2', '--output-dir', out / 'specialization']),
        ('visibility', [py, 'crates/cgx2-api-index/tests/verify_visibility_cli.py', '--binary', binary / 'cgx2', '--output-dir', out / 'visibility']),
        ('binders', [py, 'crates/cgx2-package/scripts/verify_binders.py', '--binary-dir', binary, '--output', out / 'binders.json']),
        ('output', [py, 'crates/cgx2-cli/scripts/verify_output_bounds.py', '--binary', binary / 'cgx2', '--output', out / 'output.json', '--build-address-space-mib', '4096']),
        ('emitters', [py, 'crates/cgx2-emit-python/tests/s2b_product.py', '--binary-dir', binary, '--output', out / 'emitters.json']),
        ('graph99', [py, 'crates/cgx2-syntax/s2b-memory-evidence/run.py', '--binary-dir', binary, '--output', out / 'graph99.json']),
        ('guards518', [py, 'crates/cgx2-emit-rust/tests/graph_guard_parity/run.py', '--binary-dir', binary, '--output', out / 'guards518.json']),
        ('reports265', [py, 'crates/cgx2-cli/tests/memory_report_layout/run.py', '--binary', binary / 'cgx2', '--output', out / 'reports265.json']),
        ('owned-result-original', [py, 'crates/cgx2-syntax/owned-result-evidence/run.py', '--binary-dir', binary, '--output', out / 'owned-result-original.json']),
        ('owned-result-reconciled', [py, 'plans/release-completion/s2b-owned-result-review/reconcile.py', '--binary-dir', binary, '--output-dir', out / 'reconciled']),
    ]
    selected = args.only.split(',')
    unknown = set(selected) - {name for name, _ in commands}
    if unknown:
        parser.error('unknown checks: ' + ','.join(sorted(unknown)))
    commands = [(name, command) for name, command in commands if name in selected]
    checks = []
    report = dict(schema_version=2, base='3c1cfe2aeba2f7135e4069709bf633c082d2aab0', checks=checks,
                  checkpoint_complete=False, release_complete=False,
                  environment={key: env[key] for key in ('RUSTUP_TOOLCHAIN', 'CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'TMPDIR', 'LIBRARY_PATH')},
                  bounds=dict(wall_seconds=900, cpu_seconds_per_process=900,
                              address_space_bytes_per_process=4 * 1024**3, file_size_bytes_per_process=512 * 1024**2,
                              retained_combined_bytes=8 * 1024**2, aggregate_cgroup_isolation=False),
                  root_manifests_sha256={name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in ('Cargo.toml', 'Cargo.lock')},
                  runner_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  leak_negative_control='historical host block remains open; not rerun', external_audit_performance='unrun')

    def limits():
        for kind, value in ((resource.RLIMIT_AS, 4 * 1024**3), (resource.RLIMIT_CPU, 900), (resource.RLIMIT_FSIZE, 512 * 1024**2)):
            resource.setrlimit(kind, (value, value))

    for name, command in commands:
        print('start', name, flush=True)
        argv = [str(a) for a in command]
        result = supervise(argv, cwd=ROOT, env=env, limits=limits, cap=8 * 1024**2, timeout=900, merged=True)
        record = dict(name=name, command=argv, cwd=str(ROOT), **evidence(result, include_output=False))
        data = result['stdout']
        log = out / (name + '.log.gz')
        log.write_bytes(gzip.compress(data, mtime=0))
        record.update(log=log.name, log_sha256=hashlib.sha256(data).hexdigest(),
                      passed=result['reason'] == 'exit' and result['exit'] == 0)
        record['status'] = 'passed' if record['passed'] else 'failed'
        if name == 'owned-result-original' and result['reason'] == 'exit' and result['exit'] == 1 and b'shadow-builtin:reject' in data:
            record['status'] = 'known-superseded-negative'
            record['passed'] = False
        checks.append(record)
        (out / 'verification.json').write_text(json.dumps(report, indent=2) + '\n')
        print('finish', name, record['status'], result['exit'], flush=True)
    return int(any(r['status'] == 'failed' for r in checks))


if __name__ == '__main__':
    raise SystemExit(main())
