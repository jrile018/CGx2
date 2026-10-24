#!/usr/bin/env python3
"""Required offline repair validation, with live bounded parent capture."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import sys
from bounded import run

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--only', help='Run one named check, preserving prior attempts separately')
    args = parser.parse_args()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    target, temporary = ROOT / 'target/release-owned-symbol-compatibility', ROOT / 'target/tmp-owned-symbol-compatibility'
    target.mkdir(parents=True, exist_ok=True)
    temporary.mkdir(parents=True, exist_ok=True)
    env = {**os.environ, 'RUSTUP_TOOLCHAIN': '1.98.1', 'CARGO_BUILD_JOBS': '2',
           'CARGO_TARGET_DIR': str(target), 'TMPDIR': str(temporary),
           'LIBRARY_PATH': '/tmp/cgx2-wave22-lib'}
    env['RUSTFLAGS'] = (env.get('RUSTFLAGS', '') + ' -Clink-arg=-Wl,--threads=2').strip()
    binary = target / 'debug'
    py = sys.executable
    commands = [
        ('build', ['cargo', 'build', '--workspace', '--all-features', '--offline', '--locked']),
        ('compatibility-product', [py, HERE / 'run.py', '--binary-dir', binary, '--output', output / 'compatibility-product.json']),
        ('owned-result-original', [py, 'crates/cgx2-syntax/owned-result-evidence/run.py', '--binary-dir', binary, '--output', output / 'owned-result-original.json']),
        ('owned-result-replay', [py, HERE / 'replay_result.py', '--binary-dir', binary, '--output', output / 'owned-result-replay.json']),
        ('ordinary-product', [py, 'crates/cgx2-syntax/ordinary-owned-evidence/run.py', '--binary-dir', binary, '--output', output / 'ordinary-product.json']),
        ('workspace-tests', ['cargo', 'test', '--workspace', '--all-features', '--offline', '--locked', '--no-fail-fast']),
        ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--offline', '--locked', '--', '-D', 'warnings']),
        ('fmt', ['cargo', 'fmt', '--all', '--check']),
        ('diff', ['git', 'diff', '--check']),
        ('foundation', [py, 'crates/cgx2-syntax/s2b-evidence/run.py', '--binary-dir', binary, '--output', output / 'foundation.json']),
        ('semantic', [py, 'crates/cgx2-sema/tests/semantic_resolution/run.py', '--binary', binary / 'cgx2', '--output', output / 'semantic.json']),
        ('specialization', [py, 'crates/cgx2-sema/tests/source_specialization/run.py', '--binary', binary / 'cgx2', '--output-dir', temporary / 'specialization']),
        ('visibility', [py, 'crates/cgx2-api-index/tests/verify_visibility_cli.py', '--binary', binary / 'cgx2', '--output-dir', temporary / 'visibility']),
        ('binders', [py, 'crates/cgx2-package/scripts/verify_binders.py', '--binary-dir', binary, '--output', output / 'binders.json']),
        ('output', [py, 'crates/cgx2-cli/scripts/verify_output_bounds.py', '--binary', binary / 'cgx2', '--output', output / 'output.json', '--build-address-space-mib', '4096']),
        ('emitters', [py, 'crates/cgx2-emit-python/tests/s2b_product.py', '--binary-dir', binary, '--output', output / 'emitters.json']),
        ('legacy-graph99', [py, 'crates/cgx2-syntax/s2b-memory-evidence/run.py', '--binary-dir', binary, '--output', output / 'legacy-graph99.json']),
        ('native-guards518', [py, 'crates/cgx2-emit-rust/tests/graph_guard_parity/run.py', '--binary-dir', binary, '--output', output / 'native-guards518.json']),
        ('report-layouts265', [py, 'crates/cgx2-cli/tests/memory_report_layout/run.py', '--binary', binary / 'cgx2', '--output', output / 'report-layouts265.json']),
    ]
    if args.only:
        commands = [(name, command) for name, command in commands if name == args.only]
        if not commands:
            parser.error('unknown check: ' + args.only)
    checks = []
    report = {'schema_version': 1, 'base': '3e5be4d4907debad1ae5fa8bdc836208255ddb4d',
              'checks': checks, 'dependency_reconciliation': 'none; root Cargo.lock unchanged',
              'environment': {k: env[k] for k in ['RUSTUP_TOOLCHAIN', 'CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'TMPDIR', 'LIBRARY_PATH', 'RUSTFLAGS']},
              'bounds': {'wall_seconds': 900, 'cpu_seconds': 900, 'address_space_bytes': 4 * 1024 ** 3,
                         'inherited_file_hard_cap_bytes': 512 * 1024 ** 2, 'descendant_live_output_cap_bytes': 1024 ** 2,
                         'aggregate_live_log_cap_bytes': 8 * 1024 ** 2,
                         'monitor_seconds': 0.05, 'process_tree_count': 128},
              'known_superseded_expectation': 'Original 584-check runner expects CGT082 for a valid schema-1 try_owned parameter. Replay excludes its 9 rejection checks explicitly.',
              'release_complete': False}
    for name, command in commands:
        print('start', name, flush=True)
        record = run(command, ROOT, env, output / f'{name}.log')
        record['name'] = name
        data = (output / record['log']).read_bytes()
        if name == 'workspace-tests':
            summaries = re.findall(rb'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored', data)
            record['suites'] = len(summaries)
            record.update({key: sum(int(s[i]) for s in summaries) for key, i in [('passed', 1), ('failed', 2), ('ignored', 3)]})
        record['expected_obsolete_failure'] = name == 'owned-result-original' and record['exit'] == 1 and b'shadow-builtin:reject' in data and record['limit'] is None
        checks.append(record)
        if name in ['specialization', 'visibility']:
            generated = temporary / name
            if (generated / 'report.json').exists():
                (output / (name + '.json')).write_bytes((generated / 'report.json').read_bytes())
                inventory = {str(p.relative_to(generated)): hashlib.sha256(p.read_bytes()).hexdigest()
                             for p in sorted(generated.rglob('*')) if p.is_file()}
                (output / (name + '-artifacts.json')).write_text(json.dumps(inventory, indent=2) + '\n')
        (output / 'verification.json').write_text(json.dumps(report, indent=2) + '\n')
        print('finish', name, 'exit', record['exit'], 'limit', record['limit'], flush=True)
        # Lossless compressed logs keep exact original byte hashes reviewable.
        (output / (record['log'] + '.gz')).write_bytes(gzip.compress(data, mtime=0))
        (output / record['log']).unlink()
    return int(any(r['exit'] != 0 and not r['expected_obsolete_failure'] for r in checks))


if __name__ == '__main__':
    raise SystemExit(main())
