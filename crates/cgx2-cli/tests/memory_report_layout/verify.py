#!/usr/bin/env python3
"""Run owned, offline report-layout verification with private build/temp paths."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
EVIDENCE = HERE / 'evidence'


def main():
    env = os.environ.copy()
    env.update(RUSTUP_TOOLCHAIN='1.98.1', CARGO_BUILD_JOBS='2',
               CARGO_TARGET_DIR=str(ROOT / 'target/release-report-layout'),
               TMPDIR=str(ROOT / 'target/tmp-report-layout'))
    if Path('/tmp/cgx2-wave22-lib').is_dir():
        env['LIBRARY_PATH'] = '/tmp/cgx2-wave22-lib'
    for directory in [env['CARGO_TARGET_DIR'], env['TMPDIR'], str(EVIDENCE)]:
        Path(directory).mkdir(parents=True, exist_ok=True)
    binary = Path(env['CARGO_TARGET_DIR']) / 'debug/cgx2'
    steps = [
        ('package-tests', ['cargo', 'test', '-p', 'cgx2-cli', '-p', 'cgx2-emit-rust',
                           '--all-features', '--offline']),
        ('clippy', ['cargo', 'clippy', '-p', 'cgx2-cli', '-p', 'cgx2-emit-rust',
                    '--all-targets', '--all-features', '--offline', '--', '-D', 'warnings']),
        ('fmt', ['cargo', 'fmt', '--all', '--check']),
        ('diff', ['git', 'diff', '--check']),
        ('build', ['cargo', 'build', '-p', 'cgx2-cli', '--all-features', '--offline']),
        ('product', [sys.executable, str(HERE / 'run.py'), '--binary', str(binary),
                     '--output', str(EVIDENCE / 'product.json')]),
        ('existing-graph-product', [sys.executable,
            str(ROOT / 'crates/cgx2-syntax/s2b-memory-evidence/run.py'),
            '--binary-dir', str(binary.parent), '--output', str(EVIDENCE / 'existing-graph-product.json')]),
    ]
    results = []
    for label, command in steps:
        completed = subprocess.run(command, cwd=ROOT, env=env, capture_output=True,
                                   text=True, timeout=600)
        log = completed.stdout + completed.stderr
        log_path = EVIDENCE / f'{label}.log'
        log_path.write_text(log)
        row = {'label': label, 'command': command, 'returncode': completed.returncode,
               'passed': completed.returncode == 0, 'log': log_path.name,
               'log_sha256': hashlib.sha256(log.encode()).hexdigest()}
        if label == 'package-tests':
            suites = [tuple(map(int, match)) for match in re.findall(
                r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', log)]
            row.update(suites=len(suites), passed_tests=sum(s[0] for s in suites),
                       failed_tests=sum(s[1] for s in suites), ignored_tests=sum(s[2] for s in suites))
        results.append(row)
        print(f'{label}: exit {completed.returncode}', flush=True)
        if completed.returncode:
            break
    summary = {'status': 'passed' if len(results) == len(steps) and all(r['passed'] for r in results) else 'failed',
               'scope': 'owned report-layout slice only', 'ordinary_memory_checkpoint_complete': False,
               'release_complete': False, 'environment': {key: env[key] for key in
                   ['RUSTUP_TOOLCHAIN', 'CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'TMPDIR']},
               'checks': results}
    (EVIDENCE / 'verification.json').write_text(json.dumps(summary, indent=2) + '\n')
    if summary['status'] != 'passed':
        raise SystemExit(1)


if __name__ == '__main__':
    main()
