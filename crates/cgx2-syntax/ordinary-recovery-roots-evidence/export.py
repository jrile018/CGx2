#!/usr/bin/env python3
"""Archive every attempt, source and raw stream; large binaries keep hashes."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import shutil


def fingerprint(p):
    sha = hashlib.sha256()
    size = 0
    with p.open('rb') as stream:
        for block in iter(lambda: stream.read(1 << 20), b''):
            sha.update(block)
            size += len(block)
    return {'bytes': size, 'sha256': sha.hexdigest()}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--scratch-dir', type=Path, required=True)
    p.add_argument('--output-dir', type=Path, required=True)
    a = p.parse_args()
    scratch = a.scratch_dir.resolve()
    out = a.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    manifest = {}
    missing = []

    def export(source, relative):
        key = str(relative) + '.gz'
        if key in manifest:
            return
        if not source.exists():
            missing.append({'original': str(source), 'archive': 'completed-tmp-attempts.tar.gz',
                            'reason': 'own completed tmp files byte-verified into archive before quota relief'})
            return
        target = out / key
        target.parent.mkdir(parents=True, exist_ok=True)
        with source.open('rb') as incoming, target.open('wb') as raw:
            with gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as zipped:
                shutil.copyfileobj(incoming, zipped)
        manifest[key] = {'original_path': str(source), **fingerprint(source), 'gzip': fingerprint(target)}

    suffixes = {'.json', '.cg', '.cgnb', '.toml', '.py', '.rs', '.diff', '.log', '.txt', '.gz'}
    for source in sorted(scratch.rglob('*')):
        relative = source.relative_to(scratch)
        if source.is_symlink() or 'frozen' in relative.parts or 'api-view' in relative.parts:
            continue
        if relative == Path('completed-tmp-attempts.tar.gz'):
            continue
        if source.is_file() and source.suffix in suffixes:
            export(source, Path('attempts') / relative)
    for source in sorted(scratch.rglob('report.json')):
        report = json.loads(source.read_text())
        for command in report.get('commands', []):
            argv = command.get('argv', [])
            for argument in argv:
                if argument.endswith('.rs'):
                    source_input = Path(argument)
                    if not source_input.is_absolute():
                        source_input = Path(command.get('cwd', scratch)) / source_input
                    if source_input.is_file():
                        export(source_input, Path('rustc-inputs') / source.parent.relative_to(scratch) /
                               str(command.get('name', command.get('label', 'source'))) / source_input.name)
            if 'emit-rust' in argv and '--out' in argv:
                emitted = Path(argv[argv.index('--out') + 1])
                if emitted.suffix == '.rs':
                    export(emitted, Path('emitted') / source.parent.relative_to(scratch) /
                           (str(command.get('name', command.get('label', 'source'))) + '.rs'))
        for name, artifact in report.get('standalone_artifacts', {}).items():
            export(Path(artifact['source']), Path('standalone') / source.parent.relative_to(scratch) / (name + '.rs'))
    # The quota archive includes both complete inputs and the exact partial
    # failed copy, including binary bytes. Its adjacent JSON contains raw hashes.
    archive = scratch / 'completed-tmp-attempts.tar.gz'
    if archive.exists():
        shutil.copy2(archive, out / archive.name)
        assert fingerprint(archive) == fingerprint(out / archive.name)
        manifest[archive.name] = {'original_path': str(archive), **fingerprint(archive)}
    (out / 'manifest.json').write_text(json.dumps({'schema_version': 1, 'files': manifest,
        'archived_external_references': missing}, indent=2) + '\n')
    print(json.dumps({'exported_files': len(manifest), 'archived_external_references': len(missing)}))


if __name__ == '__main__':
    main()
