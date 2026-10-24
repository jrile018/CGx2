"""Separate compiler build provenance from the harness which observes it."""
import hashlib
import json
from pathlib import Path
import re
import subprocess


def git(*args, input=None):
    from common import ROOT
    result = subprocess.run(['git', *args], cwd=ROOT, input=input, capture_output=True,
                            timeout=30, check=True)
    return result.stdout


def snapshot(paths, root):
    from common import digest
    return {str(p.relative_to(root)): digest(p) for p in sorted(paths)}


def snapshot_hash(files):
    return hashlib.sha256(json.dumps(files, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def checkout_identity():
    from common import ROOT, HERE
    from integrity import compiler_paths, harness_paths
    compiler = snapshot(compiler_paths(), ROOT)
    harness = snapshot(harness_paths(), HERE)
    return {'head': git('rev-parse', 'HEAD').decode().strip(),
            'status': git('status', '--porcelain=v1', '--untracked-files=normal').decode(),
            'tracked_diff_sha256': hashlib.sha256(git('diff', 'HEAD', '--')).hexdigest(),
            'compiler': compiler, 'compiler_snapshot_sha256': snapshot_hash(compiler),
            'harness': harness, 'harness_snapshot_sha256': snapshot_hash(harness)}


def identity_errors(identity):
    if type(identity) is not dict:
        return ['missing checkout identity']
    errors = []
    if not re.fullmatch('[0-9a-f]{40}', str(identity.get('head', ''))):
        errors.append('missing/invalid checkout commit')
    for name in ('compiler', 'harness'):
        files = identity.get(name)
        if (type(files) is not dict or not files
                or any(type(p) is not str or not re.fullmatch('[0-9a-f]{64}', str(h)) for p, h in files.items())
                or snapshot_hash(files) != identity.get(name + '_snapshot_sha256')):
            errors.append('invalid ' + name + ' snapshot')
    if type(identity.get('status')) is not str or not re.fullmatch('[0-9a-f]{64}', str(identity.get('tracked_diff_sha256', ''))):
        errors.append('missing dirty/diff observation')
    return errors


def recorded_build(prior, invocation_path, commit):
    """Audit original bytes/membership before making any new candidate view."""
    from common import ROOT, digest, normal
    from integrity import compiler_paths, file_inventory, tree_inventory, BINARIES, LIBRARIES
    from validation import inventory_errors
    prior = Path(prior).resolve()
    invocation_path = Path(invocation_path).resolve()
    invocation = json.loads(invocation_path.read_text())
    if (not re.fullmatch('[0-9a-f]{40}', commit) or invocation.get('source_commit') != commit
            or invocation.get('dirty') != '' or type(invocation.get('exit')) is not int or invocation['exit'] != 0):
        raise ValueError('missing/mismatched successful clean build source identity')
    report_path = prior / 'report.json'
    report = json.loads(report_path.read_text())
    required = {'workspace-tests', 'clippy', 'fmt', 'build', 'toolchain-sysroot', 'toolchain-identity', 'cargo-identity'}
    errors = inventory_errors(report, required, required - {'toolchain-sysroot', 'toolchain-identity', 'cargo-identity'})
    if (errors or report.get('exceptions') or report.get('complete') is not True
            or any(r.get('passed') is not True for r in report['checks'])
            or any(not normal(r) for r in report['commands'] if r['name'] in required)):
        raise ValueError('unsuccessful/incomplete original build: ' + repr(errors))
    env = invocation.get('environment', {})
    if env.get('RUSTUP_TOOLCHAIN') != '1.98.1' or env.get('CARGO_BUILD_JOBS') != '2':
        raise ValueError('wrong recorded build toolchain/jobs')
    for name in ('workspace-tests', 'clippy', 'build'):
        row = next(r for r in report['commands'] if r['name'] == name)
        if not {'--offline', '--locked'} <= set(row['argv']):
            raise ValueError('build was not offline/locked: ' + name)
    source = json.loads((prior / 'source.json').read_text())
    current = snapshot(compiler_paths(), ROOT)
    if source != current:
        raise ValueError('compiler source differs from recorded build')
    # Compare every recorded build input with the actual commit, not a nearby
    # checkout label. cat-file --batch keeps this a single bounded git command.
    paths = sorted(source)
    data = git('cat-file', '--batch', input=''.join(f'{commit}:{p}\n' for p in paths).encode())
    offset = 0
    for path in paths:
        end = data.index(b'\n', offset)
        header = data[offset:end].split()
        if len(header) != 3 or header[1] != b'blob':
            raise ValueError('missing committed compiler input: ' + path)
        size = int(header[2]); start = end + 1
        if hashlib.sha256(data[start:start + size]).hexdigest() != source[path]:
            raise ValueError('build input does not match source commit: ' + path)
        offset = start + size + 1
    inputs = json.loads((prior / 'inputs.json').read_text())
    artifacts = tree_inventory(prior / 'frozen')
    if artifacts != inputs['artifacts'] or file_inventory({Path(p) for p in inputs['toolchain']}) != inputs['toolchain']:
        raise ValueError('original artifact/dependency/toolchain identity changed')
    recorded_compiler = {str(Path(p).relative_to(invocation['cwd'])): v['sha256'] for p, v in inputs['compiler'].items()}
    if recorded_compiler != source:
        raise ValueError('original compiler manifest disagrees with recorded build')
    frozen = json.loads((prior / 'frozen.json').read_text())
    if set(frozen['binary_sha256']) != set(BINARIES) or set(frozen['probe_library_sha256']) != set(LIBRARIES):
        raise ValueError('missing canonical binary/library inventory')
    for name, expected in dict(frozen['binary_sha256'], **frozen['probe_library_sha256']).items():
        if digest(prior / 'frozen' / name) != expected:
            raise ValueError('changed original canonical artifact: ' + name)
    # Retain and verify the build's real harness, including its stale label.
    harness = {}
    prefix = Path(invocation['cwd']) / 'crates/cgx2-syntax/ordinary-parity-repair-evidence'
    for path, value in inputs['harness'].items():
        relative = str(Path(path).relative_to(prefix))
        if digest(prior / 'harness-snapshot' / relative) != value['sha256']:
            raise ValueError('changed build harness snapshot: ' + relative)
        harness[relative] = value['sha256']
    if harness.get('verify.py') != invocation.get('script_sha256'):
        raise ValueError('build invocation/harness hash mismatch')
    references = {str(p): digest(p) for p in (invocation_path, report_path, prior / 'source.json',
                                             prior / 'inputs.json', prior / 'frozen.json')}
    return {'source_commit': commit, 'compiler': source, 'compiler_snapshot_sha256': snapshot_hash(source),
            'build_invocation_path': str(invocation_path), 'compiler_source_manifest': str(prior / 'source.json'),
            'original_references': references, 'original_harness': harness,
            'original_invocation': invocation, 'original_build_environment': env,
            'original_frozen_directory': str(prior / 'frozen'),
            'original_counts': {k: len(v) for k, v in inputs.items()},
            'checkout_changes_since_build': git('diff', '--name-status', commit, 'HEAD', '--').decode(),
            'recovery_runner_compatibility_diff': git('diff', commit, 'HEAD', '--',
                'crates/cgx2-syntax/ordinary-recovery-roots-evidence/run.py').decode()}


def candidate_errors(candidate):
    from common import ROOT, HERE, digest
    from integrity import compiler_paths, harness_paths
    errors = []
    if type(candidate) is not dict or candidate.get('schema_version') != 1:
        return ['missing candidate build/harness provenance']
    build = candidate.get('build', {})
    if type(build) is not dict:
        return ['invalid candidate build identity']
    if (not re.fullmatch('[0-9a-f]{40}', str(build.get('source_commit', '')))
            or build.get('source_commit') != build.get('original_invocation', {}).get('source_commit')
            or snapshot(compiler_paths(), ROOT) != build.get('compiler')
            or snapshot_hash(build.get('compiler', {})) != build.get('compiler_snapshot_sha256')):
        errors.append('candidate compiler source identity mismatch')
    refs = build.get('original_references', {})
    if type(refs) is not dict or not refs:
        errors.append('missing original build references')
        refs = {}
    for path, expected in refs.items():
        if digest(path) != expected:
            errors.append('changed original build record: ' + path)
    for field, expected in (('build_invocation_path', build.get('original_invocation')),
                            ('compiler_source_manifest', build.get('compiler'))):
        path = build.get(field)
        if path not in refs or json.loads(Path(path).read_text()) != expected:
            errors.append('missing/mismatched original identity record: ' + field)
    harness = candidate.get('observing_harness')
    errors.extend(identity_errors(harness))
    if harness and snapshot(harness_paths(), HERE) != harness.get('harness'):
        errors.append('observing harness identity mismatch')
    return errors
