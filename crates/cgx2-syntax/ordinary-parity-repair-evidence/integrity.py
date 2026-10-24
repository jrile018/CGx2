"""Inventory bytes, membership, and symlink identities of frozen inputs."""
import json
import os
from pathlib import Path
from common import HERE, ROOT, digest

BINARIES = ('cgx2', 'cgx2-native', 'cgx2c', 'cgx2-effect-index', 'cgx2-security', 'cgx2-verify', 'cgx2-notebook')
LIBRARIES = tuple('libcgx2_' + n + '.rlib' for n in ('cli', 'sema', 'package', 'emit_rust', 'runtime'))


def tree_inventory(root):
    root = Path(root)
    result = {}
    def visit(path, ancestors):
        relative = str(path.relative_to(root))
        record = {'kind': 'directory' if path.is_dir() else 'file'}
        if path.is_symlink():
            record.update(link=os.readlink(path), resolved=str(path.resolve()))
        if path.is_dir():
            resolved = path.resolve()
            if resolved in ancestors:
                raise ValueError(f'cyclic frozen directory: {path}')
            result[relative] = record
            for child in sorted(path.iterdir()):
                visit(child, ancestors | {resolved})
        elif path.is_file():
            info=path.stat()
            record.update(sha256=digest(path), bytes=info.st_size, device=info.st_dev,
                          inode=info.st_ino, mode=info.st_mode)
            result[relative] = record
        else:
            raise ValueError(f'missing or non-file frozen input: {path}')
    visit(root, set())
    return result


def compiler_paths():
    paths = {ROOT / 'Cargo.toml', ROOT / 'Cargo.lock'}
    paths.update(ROOT.glob('rust-toolchain*'))
    for pattern in ('*/Cargo.toml', '*/build.rs', '*/src/**/*'):
        paths.update(p for p in (ROOT / 'crates').glob(pattern) if p.is_file())
    return paths


def harness_paths():
    return set(HERE.glob('*.py')) | set(HERE.glob('*.rs')) | {p for p in (HERE / 'fixtures').rglob('*') if p.is_file()}


def helper_paths():
    # The unchanged historical corpus invokes these scripts and sibling probe/
    # supervisor helpers. They are read-only inputs, distinct from this harness.
    specification = ROOT / 'plans/release-completion/s2b-ordinary-memory/regressions.json'
    paths = {specification, HERE.parent / 'ordinary-memory-evidence/run.py',
             HERE.parent / 'ordinary-memory-evidence/test_run.py',
             HERE.parent / 'owned-result-runner-evidence/supervisor.py'}
    for check in json.loads(specification.read_text())['checks']:
        for argument in check['argv']:
            argument = argument.replace('${SOURCE_ROOT}', str(ROOT))
            if argument.endswith('.py'):
                path = Path(argument)
                paths.add(path if path.is_absolute() else ROOT / path)
    for directory in {p.parent for p in paths}:
        paths.update(directory.glob('*.py'))
        paths.update(directory.glob('*.rs'))
    return paths


def file_inventory(paths):
    return {str(p.absolute()): {'sha256': digest(p), 'bytes': p.stat().st_size, 'resolved': str(p.resolve()),
                              'device':p.stat().st_dev, 'inode':p.stat().st_ino, 'mode':p.stat().st_mode,
                              'link': os.readlink(p) if p.is_symlink() else None} for p in sorted(paths)}


def freeze_inputs(binary, toolchain_paths):
    return {'artifacts': tree_inventory(binary),
            'compiler': file_inventory(compiler_paths()),
            'harness': file_inventory(harness_paths()),
            'toolchain': file_inventory(toolchain_paths), 'helpers': file_inventory(helper_paths())}


def verify_inputs(binary, expected):
    errors = []
    actual = {}
    for section in ('artifacts', 'compiler', 'harness', 'toolchain', 'helpers'):
        try:
            if section == 'artifacts':
                values = tree_inventory(binary)
            elif section == 'compiler':
                values = file_inventory(compiler_paths())
            elif section == 'harness':
                values = file_inventory(harness_paths())
            elif section == 'helpers':
                values = file_inventory(helper_paths())
            else:
                values = file_inventory([Path(p) for p in expected[section]])
            actual[section] = values
            previous = expected[section]
            errors += [f'{section}: missing {p}' for p in sorted(previous.keys() - values.keys())]
            errors += [f'{section}: added {p}' for p in sorted(values.keys() - previous.keys())]
            errors += [f'{section}: changed {p}' for p in sorted(values.keys() & previous.keys()) if values[p] != previous[p]]
        except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
            errors.append(f'{section}: {error}')
    return actual, errors


def audit(journal, binary, phase):
    try:
        path = Path(binary).parent / 'inputs.json'
        expected = json.loads(path.read_text())
        actual, errors = verify_inputs(binary, expected)
        from provenance import candidate_errors
        candidate_path = path.parent / 'candidate.json'
        manifest = json.loads((path.parent / 'frozen.json').read_text())
        candidate = json.loads(candidate_path.read_text())
        errors.extend(candidate_errors(candidate))
        if manifest.get('schema_version') != 3 or manifest.get('candidate_sha256') != digest(candidate_path):
            errors.append('missing/stale candidate provenance binding')
        if manifest.get('source_commit') != candidate.get('build', {}).get('source_commit'):
            errors.append('candidate/build source commit mismatch')
        journal.report['candidate_identity'] = {'path': str(candidate_path), 'sha256': digest(candidate_path),
            'build_source_commit': candidate.get('build', {}).get('source_commit'),
            'harness_snapshot_sha256': candidate.get('observing_harness', {}).get('harness_snapshot_sha256')}
        journal.report.setdefault('input_audits', {})[phase] = {
            'manifest': str(path), 'manifest_sha256': digest(path), 'errors': errors, 'actual': actual}
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        errors = [str(error)]
    journal.check('frozen-inputs-' + phase, not errors, errors=errors)
    if errors:
        journal.report['integrity_failed'] = True
    journal.save()
    return not errors
