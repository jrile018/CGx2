#!/usr/bin/env python3
"""Private, sequential release-loader measurement; run from repository root."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[4]
WORK = ROOT / 'target/wave24-loader'
OUT = Path(__file__).resolve().parent
PACKAGE = WORK / 'package'


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def prepare():
    original = Path('/tmp/cgx2-wave23-boundary-f32')
    shutil.copytree(original, PACKAGE)
    path = PACKAGE / 'cgx2-libtorch-closure.lock.json'
    lock = json.loads(path.read_text())
    for pin in lock['libraries']:
        source = Path(pin['path'])
        if source.is_relative_to(original):
            pin['path'] = str(PACKAGE / source.relative_to(original))
    path.write_text(json.dumps(lock, indent=2) + '\n')


def run(label, binary, trace, reps):
    destination = OUT / f'{label}.json'
    if destination.exists():
        raise FileExistsError(f'refusing to overwrite evidence: {destination}')
    env = dict(os.environ)
    for key in ('LD_PRELOAD', 'LD_AUDIT', 'CGX2_LOADER_TRACE'):
        env.pop(key, None)
    env.update(LD_LIBRARY_PATH=str(PACKAGE), OMP_NUM_THREADS='1',
               MKL_NUM_THREADS='1', OPENBLAS_NUM_THREADS='1')
    if trace:
        env['CGX2_LOADER_TRACE'] = '1'
    manifest = {
        'label': label, 'binary': str(binary), 'binary_sha256': digest(binary),
        'native_helper_sha256': digest(binary.with_name('cgx2-native')),
        'environment': {key: env[key] for key in (
            'LD_LIBRARY_PATH', 'OMP_NUM_THREADS', 'MKL_NUM_THREADS',
            'OPENBLAS_NUM_THREADS', 'CGX2_LOADER_TRACE') if key in env},
        'hashes': {str(p): digest(p) for p in PACKAGE.rglob('*') if p.is_file()},
        'closure_hashes': {pin['path']: digest(Path(pin['path'])) for pin in
                           json.loads((PACKAGE / 'cgx2-libtorch-closure.lock.json').read_text())['libraries']},
        'cargo_lock_sha256': digest(ROOT / 'Cargo.lock'),
        'rustc': subprocess.check_output(['rustc', '+1.98.1', '-vV'], text=True),
        'runs': [],
    }
    for executable in (binary, binary.with_name('cgx2-native')):
        for tool in ('ldd', 'readelf'):
            args = [tool, str(executable)] if tool == 'ldd' else [tool, '-d', str(executable)]
            result = subprocess.run(args, capture_output=True, text=True, check=True)
            manifest[f'{executable.name}_{tool}'] = result.stdout
            assert 'libpython' not in result.stdout.lower()
    for index in range(reps):
        start = time.perf_counter_ns()
        record = {'argv': [str(binary), 'run', str(PACKAGE)], 'timeout': False}
        try:
            result = subprocess.run(record['argv'], env=env, capture_output=True,
                                    text=True, timeout=120)
            record.update(exit_code=result.returncode, stdout=result.stdout,
                          stderr=result.stderr)
        except subprocess.TimeoutExpired as error:
            record.update(timeout=True, exit_code=None,
                          stdout=(error.stdout or b'').decode(),
                          stderr=(error.stderr or b'').decode())
        record['wall_ns'] = time.perf_counter_ns() - start
        try:
            value = json.loads(record['stdout'])
            if isinstance(value, str):
                value = json.loads(value)
            flat = [x for row in value for x in row]
            record['numeric_ok'] = len(flat) == 4 and all(
                math.isclose(x, y, abs_tol=1e-5, rel_tol=1e-5)
                for x, y in zip(flat, (58, 64, 139, 154)))
        except (ValueError, TypeError):
            record['numeric_ok'] = False
        manifest['runs'].append(record)
        destination.write_text(json.dumps(manifest, indent=2) + '\n')
        print(label, index, record['wall_ns'], record['exit_code'], record['numeric_ok'], flush=True)
        if record['exit_code'] != 0 or not record['numeric_ok']:
            raise RuntimeError('failed sample retained; comparison stopped')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('label')
    parser.add_argument('binary', type=Path)
    parser.add_argument('--trace', action='store_true')
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--reps', type=int, default=5)
    args = parser.parse_args()
    if args.prepare:
        prepare()
    run(args.label, args.binary.resolve(), args.trace, args.reps)
