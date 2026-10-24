#!/usr/bin/env python3
"""Build once, then freeze canonical executables and probe libraries."""
import argparse
import json
import shutil
from common import ROOT, HERE, Journal, digest, normal
from pathlib import Path
from integrity import BINARIES, LIBRARIES, compiler_paths, freeze_inputs, harness_paths


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output-dir', required=True)
    p.add_argument('--prior-workspace-report', type=__import__('pathlib').Path)
    a = p.parse_args()
    j = Journal(a.output_dir)
    build_identity = j.report.get('source_identity')
    if not build_identity:
        j.report['incomplete_reason'] = 'missing build checkout identity'
        return j.finish()
    j.report['build_identity'] = build_identity
    j.required_commands = {'workspace-tests', 'clippy', 'fmt', 'build', 'toolchain-sysroot', 'toolchain-identity', 'cargo-identity'}
    if a.prior_workspace_report:
        j.required_commands.remove('workspace-tests')
    j.required_checks = {'workspace-tests', 'clippy', 'fmt', 'build', 'toolchain-identities', 'frozen'}
    try:
        for name, cmd in [
            ('workspace-tests', ['cargo', 'test', '--workspace', '--all-features', '--offline', '--locked', '--no-fail-fast']),
            ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--offline', '--locked', '--', '-D', 'warnings']),
            ('fmt', ['cargo', 'fmt', '--all', '--check']),
            ('build', ['cargo', 'build', '--workspace', '--all-features', '--offline', '--locked']),
        ]:
            if name == 'workspace-tests' and a.prior_workspace_report:
                prior = json.loads(a.prior_workspace_report.read_text())
                if (prior.get('source_identity', {}).get('compiler') != build_identity['compiler']
                        or prior.get('environment') != j.report['environment']):
                    raise ValueError('prior workspace checks have missing/stale source or build identity')
                row = next(r for r in prior['commands'] if r['name'] == name)
                j.report['prior_workspace_evidence'] = {'path': str(a.prior_workspace_report.resolve()), 'sha256': digest(a.prior_workspace_report), 'command': row}
            else:
                row = j.run(name, cmd, suite=True)
            j.check(name, normal(row))
            if name == 'build' and not normal(row):
                return j.finish()
        source = Path(j.env['CARGO_TARGET_DIR']) / 'debug'
        frozen = j.output / 'frozen'
        frozen.mkdir()
        manifest = {'schema_version': 2, 'source_commit': build_identity['head'],
                    'compiler_snapshot_sha256': build_identity['compiler_snapshot_sha256'],
                    'harness_snapshot_sha256': build_identity['harness_snapshot_sha256'],
                    'source_directory': str(source), 'binary_directory': str(frozen),
                    'checks_after_cargo': True, 'binary_sha256': {}, 'probe_library_sha256': {}}
        for name in BINARIES + LIBRARIES:
            before = digest(source / name)
            shutil.copy2(source / name, frozen / name)
            assert before == digest(frozen / name) == digest(source / name)
            key = 'probe_library_sha256' if name.endswith('.rlib') else 'binary_sha256'
            manifest[key][name] = before
        (frozen / 'deps').symlink_to(source / 'deps', target_is_directory=True)
        (j.output / 'frozen.json').write_text(json.dumps(manifest, indent=2) + '\n')
        sysroot = j.run('toolchain-sysroot', ['rustc', '--print', 'sysroot'])
        identity = j.run('toolchain-identity', ['rustc', '--version', '--verbose'])
        cargo_identity = j.run('cargo-identity', ['cargo', '--version', '--verbose'])
        j.required_success_commands.update(('toolchain-sysroot', 'toolchain-identity', 'cargo-identity'))
        j.check('toolchain-identities', normal(sysroot) and normal(identity) and normal(cargo_identity) and 'rustc 1.98.1' in identity.get('stdout', ''))
        if not j.report['checks'][-1]['passed']:
            j.report.setdefault('prerequisite_failures', []).append('build toolchain identities')
            raise ValueError('required build toolchain identity failed')
        toolroot = Path(sysroot['stdout'].strip())
        toolchain = {toolroot/'bin/rustc', toolroot/'bin/cargo'}
        toolchain.update((toolroot/'lib').glob('libLLVM*.so*'))
        toolchain.update((toolroot/'lib').glob('librustc_driver*.so'))
        toolchain.update(p for p in (toolroot/'lib/rustlib').glob('*/lib/*') if p.is_file())
        inputs = freeze_inputs(frozen, toolchain)
        (j.output/'inputs.json').write_text(json.dumps(inputs, indent=2)+'\n')
        for path in harness_paths():
            snapshot = j.output/'harness-snapshot'/path.relative_to(HERE)
            snapshot.parent.mkdir(parents=True, exist_ok=True)
            snapshot.write_bytes(path.read_bytes())
        paths = compiler_paths()
        for pattern in ('*/Cargo.toml', '*/build.rs', '*/src/**/*.rs'):
            paths.update((ROOT / 'crates').glob(pattern))
        files = {str(p.relative_to(ROOT)): digest(p) for p in sorted(paths) if p.is_file()}
        if files != build_identity['compiler']:
            raise ValueError('compiler inputs changed during build')
        (j.output / 'source.json').write_text(json.dumps(files, indent=2) + '\n')
        j.check('frozen', True)
    except Exception:
        j.exception()
    return j.finish()


if __name__ == '__main__':
    raise SystemExit(main())
