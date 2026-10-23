#!/usr/bin/env python3
"""Prepare a reviewable lock from an ALREADY supplied environment; never installs.

Run using the same interpreter/environment selected at Rust build time. The runtime
identity input comes from the compiled example's --identity, including its actual
mapped libpython image, which may differ from this standalone Python executable.
"""
import argparse
import hashlib
from importlib import metadata
import json
from pathlib import Path
import platform
import sys
import sysconfig
import shutil

EXCLUDED = {'__pycache__', 'site-packages', 'dist-packages'}


def file_hash(path):
    if not path.is_file() or path.stat().st_size > 268435456:
        raise ValueError('artifact must be a regular file of at most 256 MiB: ' + str(path))
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def tree_hash(root):
    files = []
    pending = [root]
    count, size = 0, 0
    while pending:
        for path in pending.pop().iterdir():
            if path.name in EXCLUDED:
                continue
            count += 1
            if count > 16384 or path.is_symlink():
                raise ValueError('unsupported stdlib tree (symlink/entry limit): ' + str(path))
            if path.is_dir():
                pending.append(path)
                continue
            if path.suffix in {'.pyc', '.pyo'} or not path.is_file():
                raise ValueError('unsupported stdlib file: ' + str(path))
            size += path.stat().st_size
            if size > 268435456:
                raise ValueError('stdlib tree exceeds 256 MiB')
            name = path.relative_to(root).as_posix()
            if '\n' in name or '\0' in name:
                raise ValueError('unsupported stdlib filename')
            files.append((name, file_hash(path)))
    digest = hashlib.sha256()
    for name, value in sorted(files):
        digest.update((name + '\0' + value + '\n').encode('utf-8'))
    return digest.hexdigest()


def copy_stdlib(source, destination):
    """Copy trusted stdlib into an explicit private artifact; no writes to Python itself."""
    if destination.exists():
        raise ValueError('stdlib snapshot destination must not exist: ' + str(destination))
    destination.mkdir(parents=True)
    pending = [(source, destination)]
    while pending:
        src, dst = pending.pop()
        for path in src.iterdir():
            if path.name in EXCLUDED or path.suffix in {'.pyc', '.pyo'}:
                continue
            target = dst / path.name
            if path.is_dir():
                if path.is_symlink():
                    raise ValueError('stdlib directory symlink is unsupported: ' + str(path))
                target.mkdir()
                pending.append((path, target))
            elif path.is_file():
                shutil.copyfile(path, target)  # regular file symlinks copied as bytes
            else:
                raise ValueError('unsupported stdlib entry: ' + str(path))
    return destination.resolve(strict=True)


def distribution(name, expected_version, dependencies):
    dist = metadata.distribution(name)
    if dist.version != expected_version:
        raise ValueError(f'{name}: expected {expected_version}, installed {dist.version}')
    root = Path(dist.locate_file('')).resolve(strict=True)
    sources, meta = [], None
    for entry in dist.files or []:
        path = Path(dist.locate_file(entry))
        if entry.name == 'METADATA':
            meta = {'path': str(path.relative_to(root)), 'sha256': file_hash(path)}
        if path.suffix != '.py':
            continue
        relative = path.relative_to(root)
        parts = list(relative.with_suffix('').parts)
        package = parts[-1] == '__init__'
        if package:
            parts.pop()
        if not parts or not all(part.isidentifier() for part in parts):
            raise ValueError('nonmodule .py file in distribution: ' + str(path))
        sources.append({'module': '.'.join(parts), 'path': str(relative),
                        'is_package': package, 'sha256': file_hash(path)})
    if not meta or not sources:
        raise ValueError('distribution requires wheel METADATA and Python sources: ' + name)
    return {'name': name, 'version': dist.version, 'root': str(root), 'metadata': meta,
            'dependencies': dependencies, 'sources': sorted(sources, key=lambda s: s['module'])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime-identity', required=True, type=Path)
    parser.add_argument('--environment-id', required=True)
    parser.add_argument('--dateutil-version', required=True)
    parser.add_argument('--six-version', required=True)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--stdlib-snapshot', required=True, type=Path,
                        help='new private directory for a sealed stdlib copy, outside installed Python')
    args = parser.parse_args()
    runtime = json.loads(args.runtime_identity.read_text())
    if (Path(sys.executable).resolve() != Path(runtime['interpreter']['path'])
            or sys.version != runtime['version']
            or Path(sys.prefix).resolve() != Path(runtime['prefix'])
            or Path(sys.base_prefix).resolve() != Path(runtime['base_prefix'])):
        raise ValueError('preparation interpreter differs from compiled embedded runtime')
    stdlib = Path(sysconfig.get_path('stdlib')).resolve(strict=True)
    stdlib = copy_stdlib(stdlib, args.stdlib_snapshot)
    runtime['stdlib'] = [{'path': str(stdlib), 'sha256': tree_hash(stdlib)}]
    scripts = Path(__file__).resolve().parent
    adapter = {'name': 'cgx2-dateutil-adapter', 'version': '1', 'root': str(scripts),
               'metadata': {'path': 'dateutil_adapter.METADATA',
                            'sha256': file_hash(scripts / 'dateutil_adapter.METADATA')},
               'dependencies': ['python-dateutil'],
               'sources': [{'module': 'cgx2_dateutil_adapter',
                            'path': 'cgx2_dateutil_adapter.py', 'is_package': False,
                            'sha256': file_hash(scripts / 'cgx2_dateutil_adapter.py')}]}
    lock = {'schema_version': 1, 'environment_id': args.environment_id,
            'os': sys.platform, 'arch': {'AMD64': 'x86_64', 'aarch64': 'aarch64'}.get(
                platform.machine(), platform.machine()), 'runtime': runtime,
            'distributions': [adapter,
                              distribution('python-dateutil', args.dateutil_version, ['six']),
                              distribution('six', args.six_version, [])],
            'calls': {'dateutil.isoformat': {
                'module': 'cgx2_dateutil_adapter', 'attribute': 'isoformat',
                'positional': [{'kind': 'string'}], 'keywords': {},
                'returns': {'kind': 'string'}, 'effect': 'Foreign',
                'capability': 'ForeignCall', 'ownership': 'copied'}}}
    # Explicit output, atomic replacement, no edits to installed distributions.
    temporary = args.output.with_suffix(args.output.suffix + '.tmp')
    temporary.write_text(json.dumps(lock, indent=2, sort_keys=True) + '\n')
    temporary.replace(args.output)
    print('Prepared lock for operator review:', args.output)


if __name__ == '__main__':
    main()
