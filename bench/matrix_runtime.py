#!/usr/bin/env python3
"""Fail-closed, offline CPU f64/f32 matrix baselines (Torch 2.11.0)."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import struct
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
VERSION = '2.11.0'
COLD_CASES = ((2, 3, 2), (16, 16, 16))
WARM_CASES = ((128, 128, 128), (512, 512, 512))
MAX_MATRIX_ELEMENTS = 1_048_576
TOLERANCE = 1e-9
F32_TOLERANCE = 1e-5


class Invalid(RuntimeError):
    pass


def digest(path):
    h = hashlib.sha256()
    with open(path, 'rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def inputs(m, k, n, dtype='f64'):
    if (m, k, n) == (2, 3, 2):
        return [1., 2., 3., 4., 5., 6.], [7., 8., 9., 10., 11., 12.]
    data = ([(i % 7 - 3) / 8 for i in range(m * k)],
            [(i % 5 - 2) / 7 for i in range(k * n)])
    return tuple([struct.unpack('f', struct.pack('f', v))[0] for v in xs]
                 for xs in data) if dtype == 'f32' else data


def reference(m, k, n, dtype='f64'):
    # Independent scalar double accumulation of inputs rounded to the selected dtype.
    a, b = inputs(m, k, n, dtype)
    return [sum(a[i * k + z] * b[z * n + j] for z in range(k))
            for i in range(m) for j in range(n)]


def literal(data, rows, cols):
    return '[' + ', '.join('[' + ', '.join(repr(float(data[i * cols + j]))
                                      for j in range(cols)) + ']'
                            for i in range(rows)) + ']'


def source(m, k, n, dtype='f64'):
    if min(m, k, n) < 1 or max(m, k, n) > 32 or max(m*k, k*n, m*n) > 1024:
        raise Invalid('cold source cap: dimensions 1..32, at most 1024 elements')
    a, b = inputs(m, k, n, dtype)
    symbol = 'cgx2_torch_matmul_f32' if dtype == 'f32' else 'cgx2_torch_matmul'
    return (f'unsafe foreign native pub fn matmul_{dtype}_cpu(left: String, right: String) -> String '
            f'effects(Foreign) caps(native) = torch_cpu.{symbol} '
            f'runtime "cgx2.libtorch.cpu@{VERSION}" ownership copied\n\n'
            'fn main() -> String effects(Foreign) caps(native):\n'
            f'    matmul_{dtype}_cpu("{literal(a, m, k)}", "{literal(b, k, n)}")\n')


def clone_package(original, destination):
    original = original.resolve(strict=True)
    try:
        shutil.copytree(original, destination, copy_function=os.link)
    except OSError:
        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(original, destination)
    sidecar = destination / 'cgx2-libtorch-closure.lock.json'
    lock = json.loads(sidecar.read_text())
    for library in lock['libraries']:
        path = Path(library['path'])
        if path == original or original in path.parents:
            library['path'] = str(destination / path.relative_to(original))
    # Replace hardlinks atomically; never write through an original package inode.
    replacement = sidecar.with_suffix('.new')
    replacement.write_text(json.dumps(lock, sort_keys=True, separators=(',', ':')) + '\n')
    replacement.replace(sidecar)
    return destination


def write_source(package, shape, dtype='f64'):
    file = package / 'src/main.cg'
    replacement = file.with_suffix('.new')
    replacement.write_text(source(*shape, dtype))
    replacement.replace(file)


def run_timed(command, timeout, env, cwd=ROOT):
    with tempfile.TemporaryDirectory(prefix='cgx2-matrix-time-') as tmp:
        timing = Path(tmp) / 'time.txt'
        wrapped = ['/usr/bin/time', '-f', '%U %S %M', '-o', str(timing), *map(str, command)]
        start = time.perf_counter_ns()
        process = subprocess.Popen(wrapped, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, start_new_session=True)
        timed_out = False
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            try:
                os.killpg(process.pid, 9)
            except ProcessLookupError:
                pass
            stdout, stderr = process.communicate()
        wall = time.perf_counter_ns() - start
        cpu_ns = peak_rss_kib = None
        if timing.exists():
            try:
                user, system, peak = timing.read_text().strip().split()[:3]
                cpu_ns = round((float(user) + float(system)) * 1e9)
                peak_rss_kib = int(peak)
            except (ValueError, IndexError):
                pass
        return {'argv': list(map(str, command)), 'wall_ns': wall, 'cpu_ns': cpu_ns,
                'peak_rss_kib': peak_rss_kib, 'timeout': timed_out,
                'exit_code': process.returncode,
                'stdout': stdout.decode('utf-8', 'replace'),
                'stderr': stderr.decode('utf-8', 'replace')[-4000:]}


def require_run(command, timeout, env, report=None):
    result = run_timed(command, timeout, env)
    if report is not None:
        report.setdefault('setup_runs', []).append(result)
    if result['timeout']:
        raise Invalid(f'timed out: {command[0]}')
    if result['exit_code'] != 0:
        raise Invalid(f'command failed: {command[0]}: {result["stderr"][-500:]}')
    return result


def output_values(payload):
    return [v for row in payload for v in row] if isinstance(payload, list) else payload['output']


def checked_output(run, expected, route, dtype='f64', shape=None):
    try:
        payload = json.loads(run['stdout'])
        if isinstance(payload, list):
            if shape and (len(payload) != shape[0] or any(
                    not isinstance(row, list) or len(row) != shape[2] for row in payload)):
                raise Invalid(f'{route}: wrong output shape')
        else:
            if payload['version'] != VERSION:
                raise Invalid(f'{route}: Torch version mismatch')
            if dtype == 'f32' and payload.get('dtype') != dtype:
                raise Invalid(f'{route}: wrong output dtype')
            if shape and payload.get('shape') != [shape[0], shape[2]]:
                raise Invalid(f'{route}: wrong output shape')
        flat = output_values(payload)
        tolerance = F32_TOLERANCE if dtype == 'f32' else TOLERANCE
        if len(flat) != len(expected) or any(
                isinstance(value, bool) or not isinstance(value, (int, float)) or
                not math.isfinite(value) or
                abs(value - target) > tolerance + tolerance * abs(target)
                for value, target in zip(flat, expected)):
            raise Invalid(f'{route}: output differs from {dtype} scalar reference')
        canonical = ','.join(f'{float(value):.9f}' for value in flat).encode()
        raw = json.dumps(flat, separators=(',', ':'), allow_nan=False).encode()
        return hashlib.sha256(canonical).hexdigest(), hashlib.sha256(raw).hexdigest(), payload
    except (ValueError, TypeError, KeyError) as error:
        raise Invalid(f'{route}: invalid output: {error}') from error


def interval(samples):
    if len(samples) < 5:
        return {'status': 'insufficient_samples', 'n': len(samples),
                'median_ns': statistics.median(samples) if samples else None}
    rng = random.Random(20261023)
    medians = sorted(statistics.median(rng.choices(samples, k=len(samples)))
                     for _ in range(2000))
    return {'status': 'bootstrap_95_percentile', 'n': len(samples),
            'median_ns': statistics.median(samples),
            'median_ci_ns': [medians[49], medians[1949]],
            'p95_ns': sorted(samples)[math.ceil(.95 * len(samples)) - 1]
            if len(samples) >= 20 else None}


def record_run(report, run, shape, route, phase, index=None):
    record = {key: value for key, value in run.items() if key != 'stdout'}
    record.update({'shape': list(shape), 'route': route, 'phase': phase,
                   'stdout_sha256': hashlib.sha256(run['stdout'].encode()).hexdigest(),
                   'stdout_bytes': len(run['stdout'].encode())})
    if index is not None:
        record['index'] = index
    report['runs'].append(record)
    if run['timeout'] or run['exit_code'] != 0:
        record['partial_stdout'] = run['stdout']
        reason = 'timeout' if run['timeout'] else 'nonzero_exit'
        report['exclusions'].append({'shape': list(shape), 'route': route,
                                     'phase': phase, 'index': index, 'reason': reason})
        raise Invalid(f'{route} {phase} {reason} at {shape}')
    return record


def checked_samples(payload, repetitions, route, shape):
    samples = payload.get('samples')
    if not isinstance(samples, list) or len(samples) != repetitions:
        raise Invalid(f'{route} warm sample count mismatch at {shape}')
    for sample in samples:
        if not isinstance(sample, dict) or any(
                not isinstance(sample.get(key), int) or isinstance(sample[key], bool)
                or sample[key] < 0 for key in
                ('wall_ns', 'cpu_ns', 'matmul_wall_ns', 'peak_rss_kib')):
            raise Invalid(f'{route} invalid warm sample at {shape}')
        if sample['wall_ns'] == 0 or sample['matmul_wall_ns'] > sample['wall_ns']:
            raise Invalid(f'{route} invalid warm phase timing at {shape}')
    return samples


def observations(cases):
    observed = []
    for case in cases:
        routes = case['routes']
        row = {'shape': case['shape'], 'question': case['question'],
               'target_status': 'not_scored: selected matrix calls are not inference workloads'}
        if case['question'] == 'cold_source_vs_cpp':
            cpp = routes['cpp']['cold_wall']['median_ns']
            source = routes['source']['cold_wall']['median_ns']
            row['source_cold_over_cpp_observation'] = source / cpp if cpp and source else None
            row['cold_claim'] = 'observations only; no release/debug speed comparison'
        else:
            cpp = routes['cpp']['warm_inclusive_wall']['median_ns']
            facade = routes['facade']['warm_inclusive_wall']['median_ns']
            row['facade_warm_throughput_over_cpp'] = cpp / facade
            cpp_p95 = routes['cpp']['warm_inclusive_wall'].get('p95_ns')
            facade_p95 = routes['facade']['warm_inclusive_wall'].get('p95_ns')
            row['facade_warm_p95_over_cpp'] = (facade_p95 / cpp_p95
                                               if cpp_p95 and facade_p95 else None)
            if 'python' in routes:
                python = routes['python']['warm_inclusive_wall']['median_ns']
                row['facade_warm_throughput_over_python'] = python / facade
        observed.append(row)
    return observed


def package_checks(package, dtype='f64'):
    if any(path.is_symlink() for path in package.rglob('*')):
        raise Invalid('pinned package must contain regular files, not symlinks')
    needed = ('cgx2.toml', 'cgx2-bind.lock.json', 'cgx2-libtorch-closure.lock.json',
              'cgx2-libtorch-evidence.json', 'libcgx2_libtorch_facade.so')
    for name in needed:
        if not (package / name).is_file():
            raise Invalid(f'missing pinned package file: {name}')
    binding = json.loads((package / 'cgx2-bind.lock.json').read_text())
    evidence = json.loads((package / 'cgx2-libtorch-evidence.json').read_text())
    matches = [b for b in binding['bindings'] if b['id'] == 'torch_cpu']
    if len(matches) != 1 or matches[0]['version'] != VERSION or evidence['torch_version'] != VERSION:
        raise Invalid('package Torch version differs from 2.11.0')
    layout = 'tensor-f32-cpu-matrix-v3' if dtype == 'f32' else 'tensor-f64-cpu-matrix-v2'
    if matches[0]['layout'] != layout:
        raise Invalid(f'{dtype} package requires {layout}')
    if evidence['facade_sha256'] != digest(package / 'libcgx2_libtorch_facade.so'):
        raise Invalid('evidence facade hash differs from package bytes')
    return {str(path.relative_to(package)): digest(path)
            for path in sorted(package.rglob('*')) if path.is_file()}


def artifact_provenance(path, artifacts):
    matches = [row for row in artifacts if row.get('executable') and
               Path(row['executable']).resolve() == path.resolve()]
    return {'path': str(path.resolve()), 'sha256': digest(path),
            'profile_status': 'cargo_artifact_verified' if matches else 'unverified-profile',
            'cargo_profile': matches[-1]['profile'] if matches else None}


def build(args, output_dir, env, report):
    # A private target directory ties Cargo artifact profiles to the bytes just built.
    target = output_dir / 'rust-build'
    profile_dir = target / args.build_profile
    cgx2 = Path(args.cgx2) if args.cgx2 else profile_dir / 'cgx2'
    binder = Path(args.binder) if args.binder else profile_dir / 'cgx2-bind'
    artifacts = []
    if not args.cgx2 or not args.binder:
        command = ['cargo', 'build', '--offline', '--locked', '-p', 'cgx2-cli',
                   '--bins', '-p', 'cgx2-bind', '--target-dir', str(target),
                   '--message-format=json']
        if args.build_profile == 'release':
            command.append('--release')
        result = require_run(command, args.build_timeout, env, report)
        artifacts = [json.loads(line) for line in result['stdout'].splitlines()
                     if line.startswith('{')]
        artifacts = [row for row in artifacts if row.get('reason') == 'compiler-artifact']
    if not cgx2.is_file() or not binder.is_file():
        raise Invalid('cgx2 or cgx2-bind binary unavailable')
    report['build_provenance']['rust_binaries'] = {
        name: artifact_provenance(path, artifacts) for name, path in
        [('cgx2', cgx2), ('cgx2-native', cgx2.with_name('cgx2-native')), ('cgx2-bind', binder)]}
    build_dir = output_dir / 'cpp-build'
    command = ['cmake', '-S', str(ROOT / 'bench/matrix_runtime_cpp'), '-B', str(build_dir),
               f'-DCMAKE_PREFIX_PATH={args.torch_root}',
               f'-DCMAKE_CXX_COMPILER={args.compiler}',
               f'-DCGX2_EXPECT_TORCH_VERSION={VERSION}', '-DCMAKE_BUILD_TYPE=Release',
               '-DCMAKE_EXPORT_COMPILE_COMMANDS=ON']
    require_run(command, args.build_timeout, env, report)
    require_run(['cmake', '--build', str(build_dir), '--config', 'Release', '-j2'],
                args.build_timeout, env, report)
    cpp = build_dir / 'cgx2_matrix_baseline'
    if not cpp.is_file():
        raise Invalid('hand-written LibTorch C++ baseline unavailable')
    report['build_provenance']['cpp'] = {
        'profile': 'Release', 'sha256': digest(cpp),
        'compile_commands': json.loads((build_dir / 'compile_commands.json').read_text()),
        'link_command': (build_dir / 'CMakeFiles/cgx2_matrix_baseline.dir/link.txt').read_text(),
        'cmake_cache': (build_dir / 'CMakeCache.txt').read_text()}
    for name, path in [('cgx2', cgx2), ('cgx2-native', cgx2.with_name('cgx2-native'))]:
        linkage = require_run(['ldd', path], args.timeout, env, report)
        report.setdefault('python_linkage', {})[name] = {
            'method': 'ldd transitive ELF linkage (not a /proc mapped-library census)',
            'libpython_present': 'libpython' in linkage['stdout'],
            'output': linkage['stdout'], 'sha256': digest(path)}
    return cgx2.resolve(), binder.resolve(), cpp.resolve()


def validated_run(report, command, timeout, env, shape, route, phase, expected, index=None):
    run = run_timed(command, timeout, env)
    record = record_run(report, run, shape, route, phase, index)
    try:
        canonical, raw, payload = checked_output(
            run, expected, route, report['conditions']['dtype'], shape)
    except Invalid:
        record['invalid_stdout'] = run['stdout']
        report['exclusions'].append({'shape': list(shape), 'route': route,
                                     'phase': phase, 'index': index,
                                     'reason': 'wrong_output'})
        raise
    values = output_values(payload)
    record['reference_max_abs_error'] = max(abs(x-y) for x, y in zip(values, expected))
    record['canonical_output_sha256'] = canonical
    record['raw_output_sha256'] = raw
    return record, canonical, raw, payload


def same_outputs(report, case, outputs, phase, dtype='f64'):
    tolerance = F32_TOLERANCE if dtype == 'f32' else TOLERANCE
    values = list(outputs.values())
    if any(len(a) != len(b) or any(
            abs(x-y) > tolerance + tolerance * abs(y) or
            abs(x-y) > tolerance + tolerance * abs(x)
            for x, y in zip(a, b))
           for a in values for b in values):
        report['exclusions'].append({'shape': case['shape'], 'phase': phase,
                                     'reason': 'route_tolerance_mismatch'})
        raise Invalid(f'{phase} route values differ at {case["shape"]}')
    case.setdefault('numeric_checks', {})[phase] = 'all route pairs within fixed tolerance'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--torch-root', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--compiler', default='/usr/bin/c++')
    parser.add_argument('--cgx2', type=Path)
    parser.add_argument('--binder', type=Path)
    parser.add_argument('--dtype', choices=['f64', 'f32'], default='f64',
                        help='explicit precision; f32 requires a pinned v3 package')
    parser.add_argument('--build-profile', choices=['release', 'debug'], default='release')
    parser.add_argument('--python-torch', type=Path,
                        help='optional Python with compatible CPU Torch 2.11.0')
    parser.add_argument('--cold-reps', type=int, default=3)
    parser.add_argument('--warm-reps', type=int, default=30)
    parser.add_argument('--warmup', type=int, default=5)
    parser.add_argument('--timeout', type=int, default=300)
    parser.add_argument('--build-timeout', type=int, default=600)
    parser.add_argument('--cold-budget-seconds', type=int, default=240)
    args = parser.parse_args()
    if min(args.cold_reps, args.warm_reps, args.timeout, args.build_timeout,
           args.cold_budget_seconds) < 1 or args.warmup < 0:
        parser.error('repetitions and timeouts must be positive; warmup may be zero')
    args.out.parent.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.update({'OMP_NUM_THREADS': '1', 'MKL_NUM_THREADS': '1',
                'OPENBLAS_NUM_THREADS': '1', 'NUMEXPR_NUM_THREADS': '1',
                'CUDA_VISIBLE_DEVICES': '',
                'LD_LIBRARY_PATH': str(args.package.resolve())})
    report = {
        'schema_version': 3, 'status': 'invalid', 'pinned_torch_version': VERSION,
        'build_provenance': {
            'requested_rust_profile': args.build_profile,
            'environment': {key: value for key, value in env.items() if key in
                            ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER',
                             'RUSTC_WORKSPACE_WRAPPER', 'LIBRARY_PATH', 'LD_LIBRARY_PATH',
                             'CXXFLAGS', 'CXX', 'CC', 'CFLAGS', 'RUSTUP_TOOLCHAIN', 'TMPDIR') or
                            key.startswith('CARGO_PROFILE_')},
            'command': [sys.executable, *sys.argv],
            'comparison': 'selected observations only; no release/debug speed claim'},
        'inputs': {'package': str(args.package), 'torch_root': str(args.torch_root),
                   'compiler': str(args.compiler),
                   'python_torch': str(args.python_torch) if args.python_torch else None},
        'scope_exclusions': ['GPU', 'LLM authoring', 'inference workloads',
                             'arbitrary CGx2 source shapes'],
        'conditions': {'cold_shapes': list(COLD_CASES), 'warm_shapes': list(WARM_CASES),
                       'matrix_element_limit': MAX_MATRIX_ELEMENTS,
                       'source_dimension_limit': 32, 'source_element_limit': 1024,
                       'device': 'CPU', 'dtype': args.dtype, 'row_major': True,
                       'input_rounding': 'IEEE binary32 before scalar reference and all routes'
                       if args.dtype == 'f32' else 'IEEE binary64',
                       'intraop_threads': 1, 'interop_threads': 1,
                       'tolerance_abs': F32_TOLERANCE if args.dtype == 'f32' else TOLERANCE,
                       'tolerance_rel': F32_TOLERANCE if args.dtype == 'f32' else TOLERANCE,
                       'cold_reps': args.cold_reps, 'warm_reps': args.warm_reps,
                       'warmup': args.warmup, 'timeout_seconds': args.timeout,
                       'build_timeout_seconds': args.build_timeout,
                       'cold_budget_seconds': args.cold_budget_seconds},
        'interval_method': {'median': '2000 bootstrap resamples, percentile 95% CI; n>=5',
                            'bootstrap_seed': 20261023,
                            'bootstrap_percentile_indexes': [49, 1949],
                            'p95': 'nearest rank; n>=20, otherwise unavailable'},
        'phases': {'cold_source_validation': 'measured separately as whole cgx2 check process',
                   'cold_source_loader': 'included in whole process; independently unmeasured',
                   'cold_source_adapter_copy': 'unmeasured',
                   'warm_facade_loader': 'direct C ABI; full pinned Rust closure checks excluded',
                   'warm_matmul': 'measured inside each call',
                   'warm_adapter_copy_release': 'measured only as aggregate inclusive-minus-matmul'},
        'host': {'platform': platform.platform(), 'logical_cpus': os.cpu_count(),
                 'machine': platform.machine(),
                 'cpu_model': next((line.split(':', 1)[1].strip() for line in
                                    Path('/proc/cpuinfo').read_text().splitlines()
                                    if line.startswith('model name')), 'unavailable')
                 if Path('/proc/cpuinfo').is_file() else 'unavailable'},
        'runs': [], 'cases': [], 'errors': [], 'exclusions': [], 'unavailable': []}
    try:
        if platform.system() != 'Linux' or not Path('/usr/bin/time').is_file():
            raise Invalid('Linux and /usr/bin/time required for per-process resources')
        package = args.package.resolve(strict=True)
        torch_root = args.torch_root.resolve(strict=True)
        report['package_hashes'] = package_checks(package, args.dtype)
        for command in (['git', 'rev-parse', 'HEAD'], ['git', 'status', '--porcelain'],
                        ['rustc', '-vV'], ['cargo', '-V'], ['cmake', '--version'],
                        [args.compiler, '--version']):
            require_run(command, args.build_timeout, env, report)
        for library in ('libtorch.so', 'libtorch_cpu.so', 'libc10.so'):
            if digest(package / library) != digest(torch_root / 'lib' / library):
                raise Invalid(f'baseline LibTorch library differs pinned package: {library}')
        for shape in (*COLD_CASES, *WARM_CASES):
            m, k, n = shape
            if max(m * k, k * n, m * n) > MAX_MATRIX_ELEMENTS:
                raise Invalid(f'shape exceeds facade element limit: {shape}')
        with tempfile.TemporaryDirectory(prefix='cgx2-matrix-bench-') as temporary:
            workspace = Path(temporary)
            measured_package = clone_package(package, workspace / 'package')
            closure_files = ('cgx2-libtorch-closure.lock.json',
                             'libcgx2_libtorch_facade.so', 'libtorch.so',
                             'libtorch_cpu.so', 'libc10.so')
            closure_hashes = {name: digest(measured_package / name)
                              for name in closure_files}
            measured_env = dict(env, LD_LIBRARY_PATH=str(measured_package))
            cgx2, binder, cpp = build(args, workspace, measured_env, report)
            tool_files = (cgx2, binder, cpp, cgx2.with_name('cgx2-native'),
                          ROOT / 'Cargo.lock', ROOT / 'bench/matrix_runtime.py',
                          ROOT / 'bench/matrix_runtime_cpp/main.cpp',
                          ROOT / 'bench/matrix_runtime_cpp/CMakeLists.txt',
                          ROOT / 'bench/matrix_runtime_torch.py',
                          torch_root / 'include/torch/csrc/api/include/torch/torch.h',
                          torch_root / 'include/torch/csrc/api/include/torch/version.h',
                          Path(sys.executable).resolve(), Path(shutil.which('cargo')).resolve(),
                          Path(shutil.which('rustc')).resolve(),
                          Path(shutil.which('cmake')).resolve())
            report['toolchain_hashes'] = {str(path): digest(path) for path in tool_files}
            report['toolchain_hashes'][str(Path(args.compiler).resolve())] = digest(
                Path(args.compiler).resolve())
            facade = measured_package / 'libcgx2_libtorch_facade.so'
            require_run([binder, 'check', measured_package], args.timeout, measured_env, report)
            python = args.python_torch.resolve(strict=True) if args.python_torch else None
            if python:
                report['toolchain_hashes'][str(python)] = digest(python)
            else:
                report['unavailable'].append('direct Python torch.matmul: no interpreter selected')

            for shape in COLD_CASES:
                m, k, n = shape
                write_source(measured_package, shape, args.dtype)
                expected = reference(*shape, args.dtype)
                case = {'shape': list(shape), 'question': 'cold_source_vs_cpp',
                        'routes': {}, 'output_hashes': {}}
                report['cases'].append(case)
                case['source_sha256'] = digest(measured_package / 'src/main.cg')
                commands = {
                    'source': [cgx2, 'run', measured_package],
                    'cpp': [cpp, 'baseline', 'cold', m, k, n, args.warmup,
                            args.warm_reps, facade, args.dtype]}
                hashes = {}
                for route, command in commands.items():
                    _, canonical, raw, payload = validated_run(
                        report, command, args.timeout, measured_env, shape,
                        route, 'validation', expected)
                    hashes[route] = output_values(payload)
                    case['output_hashes'][route] = {'canonical_sha256': canonical,
                                                    'raw_sha256': raw}
                same_outputs(report, case, hashes, 'validation', args.dtype)
                check = run_timed([cgx2, 'check', measured_package], args.timeout,
                                  measured_env)
                record_run(report, check, shape, 'source', 'check_manifest_closure')
                for route, command in commands.items():
                    samples = []
                    begun = time.monotonic()
                    for index in range(args.cold_reps):
                        if time.monotonic() - begun >= args.cold_budget_seconds:
                            report['runs'].append({'shape': list(shape), 'route': route,
                                                   'phase': 'cold', 'index': index,
                                                   'status': 'stopped_budget'})
                            report['exclusions'].append({'shape': list(shape), 'route': route,
                                                         'phase': 'cold', 'index': index,
                                                         'reason': 'stopped_budget'})
                            break
                        record, canonical, _, payload = validated_run(
                            report, command, args.timeout, measured_env, shape,
                            route, 'cold', expected, index)
                        same_outputs(report, case, {'validated': hashes[route],
                                     route: output_values(payload)}, 'cold', args.dtype)
                        samples.append(record['wall_ns'])
                    case['routes'][route] = {'cold_wall': interval(samples)}

            for shape in WARM_CASES:
                m, k, n = shape
                expected = reference(*shape, args.dtype)
                case = {'shape': list(shape), 'question': 'warm_facade_vs_cpp',
                        'routes': {}, 'output_hashes': {}}
                report['cases'].append(case)
                commands = {
                    'cpp': [cpp, 'baseline', 'warm', m, k, n, args.warmup,
                            args.warm_reps, facade, args.dtype],
                    'facade': [cpp, 'facade', 'warm', m, k, n, args.warmup,
                               args.warm_reps, facade, args.dtype]}
                if python:
                    commands['python'] = [python, ROOT / 'bench/matrix_runtime_torch.py',
                                          'warm', m, k, n, args.warmup, args.warm_reps,
                                          '--dtype', args.dtype]
                hashes = {}
                for route, command in commands.items():
                    validation_command = list(command)
                    validation_command[2] = 'cold'
                    _, canonical, raw, payload = validated_run(
                        report, validation_command, args.timeout, measured_env,
                        shape, route, 'validation', expected)
                    if route == 'python' and Path(payload['torch_root']).resolve() != torch_root:
                        raise Invalid('Python Torch installation differs pinned header/library root')
                    hashes[route] = output_values(payload)
                    case['output_hashes'][route] = {'canonical_sha256': canonical,
                                                    'raw_sha256': raw}
                same_outputs(report, case, hashes, 'validation', args.dtype)
                warm_outputs = {}
                for route, command in commands.items():
                    record, canonical, raw, payload = validated_run(
                        report, command, args.timeout, measured_env, shape,
                        route, 'warm', expected)
                    warm_outputs[route] = output_values(payload)
                    same_outputs(report, case, {'validated': hashes[route],
                                 route: warm_outputs[route]}, 'warm_repeat', args.dtype)
                    if route == 'python' and Path(payload['torch_root']).resolve() != torch_root:
                        raise Invalid('Python Torch installation differs pinned header/library root')
                    record['samples'] = payload.get('samples')
                    samples = checked_samples(payload, args.warm_reps, route, shape)
                    case['output_hashes'][route]['warm_raw_sha256'] = raw
                    case['routes'][route] = {
                        'warm_inclusive_wall': interval([s['wall_ns'] for s in samples]),
                        'warm_matmul_wall': interval([s['matmul_wall_ns'] for s in samples]),
                        'warm_non_matmul_wall': interval(
                            [s['wall_ns'] - s['matmul_wall_ns'] for s in samples])}
                same_outputs(report, case, warm_outputs, 'warm', args.dtype)
            if package_checks(package, args.dtype) != report['package_hashes'] or any(
                    digest(measured_package / name) != value
                    for name, value in closure_hashes.items()):
                raise Invalid('pinned package or measured closure changed during run')
            report['status'] = 'valid'
            report['summary'] = {
                'scope': f'two cold source shapes and two larger warm {args.dtype} CPU shapes',
                'source_cold_vs_cpp': 'compare whole-process cold wall intervals',
                'facade_warm_vs_cpp': 'compare transfer-inclusive per-call intervals',
                'phase_limits': 'cold check is a separate process; warm non-matmul aggregates creation, copy, release and timer overhead',
                'observations': observations(report['cases'])}
    except (Invalid, OSError, KeyError, TypeError, ValueError,
            json.JSONDecodeError) as error:
        report['errors'].append(str(error))
    args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    print(f'{report["status"]}: {args.out}')
    for error in report['errors']:
        print(error, file=sys.stderr)
    return 0 if report['status'] == 'valid' else 1


if __name__ == '__main__':
    sys.exit(main())
