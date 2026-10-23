#!/usr/bin/env python3
"""Fail-closed, offline CPU float64 matrix baseline harness (Torch 2.11.0)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
VERSION = '2.11.0'
CASES = ((2, 3, 2), (16, 16, 16))
TOLERANCE = 1e-9


class Invalid(RuntimeError):
    pass


def digest(path):
    h = hashlib.sha256()
    with open(path, 'rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def inputs(m, k, n):
    if (m, k, n) == (2, 3, 2):
        return [1., 2., 3., 4., 5., 6.], [7., 8., 9., 10., 11., 12.]
    return ([(i % 7 - 3) / 8 for i in range(m * k)],
            [(i % 5 - 2) / 7 for i in range(k * n)])


def reference(m, k, n):
    a, b = inputs(m, k, n)
    return [sum(a[i * k + z] * b[z * n + j] for z in range(k))
            for i in range(m) for j in range(n)]


def literal(data, rows, cols):
    return '[' + ', '.join('[' + ', '.join(repr(float(data[i * cols + j]))
                                      for j in range(cols)) + ']'
                            for i in range(rows)) + ']'


def source(m, k, n):
    a, b = inputs(m, k, n)
    return ('unsafe foreign native pub fn matmul_f64_cpu(left: String, right: String) -> String '
            'effects(Foreign) caps(native) = torch_cpu.cgx2_torch_matmul '
            f'runtime "cgx2.libtorch.cpu@{VERSION}" ownership copied\n\n'
            'fn main() -> String effects(Foreign) caps(native):\n'
            f'    matmul_f64_cpu("{literal(a, m, k)}", "{literal(b, k, n)}")\n')


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


def write_source(package, shape):
    file = package / 'src/main.cg'
    replacement = file.with_suffix('.new')
    replacement.write_text(source(*shape))
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


def require_run(command, timeout, env):
    result = run_timed(command, timeout, env)
    if result['timeout']:
        raise Invalid(f'timed out: {command[0]}')
    if result['exit_code'] != 0:
        raise Invalid(f'command failed: {command[0]}: {result["stderr"][-500:]}')
    return result


def checked_output(run, expected, route):
    try:
        payload = json.loads(run['stdout'])
        if isinstance(payload, list):
            flat = [v for row in payload for v in row]
        else:
            if payload['version'] != VERSION:
                raise Invalid(f'{route}: Torch version mismatch')
            flat = payload['output']
        if len(flat) != len(expected) or any(
                not isinstance(value, (int, float)) or
                abs(value - target) > TOLERANCE + TOLERANCE * abs(target)
                for value, target in zip(flat, expected)):
            raise Invalid(f'{route}: output differs from float64 reference')
        canonical = ','.join(f'{float(value):.9f}' for value in flat).encode()
        return hashlib.sha256(canonical).hexdigest(), payload
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
            'p95_ns': sorted(samples)[max(0, int(.95 * len(samples)) - 1)]
            if len(samples) >= 20 else None}


def observations(cases):
    observed = []
    for case in cases:
        routes = case['routes']
        if not {'source', 'cpp', 'facade'} <= routes.keys():
            continue
        cpp_cold = routes['cpp']['cold_wall']['median_ns']
        source_cold = routes['source']['cold_wall']['median_ns']
        cpp_warm = routes['cpp']['warm_inclusive_wall']['median_ns']
        facade_warm = routes['facade']['warm_inclusive_wall']['median_ns']
        row = {'shape': case['shape'], 'source_cold_over_cpp': source_cold / cpp_cold,
               'facade_warm_throughput_over_cpp': cpp_warm / facade_warm,
               'provisional_throughput_target': 0.90,
               'provisional_p95_latency_ceiling': 1.15,
               'target_status': 'not_scored: small matrix calls, not compute-dominant inference'}
        cpp_p95 = routes['cpp']['warm_inclusive_wall'].get('p95_ns')
        facade_p95 = routes['facade']['warm_inclusive_wall'].get('p95_ns')
        row['facade_warm_p95_over_cpp'] = (facade_p95 / cpp_p95
                                            if cpp_p95 and facade_p95 else None)
        if 'python' in routes:
            python_warm = routes['python']['warm_inclusive_wall']['median_ns']
            row['facade_warm_throughput_over_python'] = python_warm / facade_warm
        observed.append(row)
    return observed


def package_checks(package):
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
    if matches[0]['layout'] != 'tensor-f64-cpu-matrix-v2':
        raise Invalid('matrix v2 package required')
    if evidence['facade_sha256'] != digest(package / 'libcgx2_libtorch_facade.so'):
        raise Invalid('evidence facade hash differs from package bytes')
    return {str(path.relative_to(package)): digest(path)
            for path in sorted(package.rglob('*')) if path.is_file()}


def build(args, output_dir, env):
    cgx2 = Path(args.cgx2) if args.cgx2 else ROOT / 'target/debug/cgx2'
    binder = Path(args.binder) if args.binder else ROOT / 'target/debug/cgx2-bind'
    if not args.cgx2 or not args.binder:
        command = ['cargo', 'build', '--offline', '--locked', '-p', 'cgx2-cli',
                   '--bins', '-p', 'cgx2-bind']
        require_run(command, args.build_timeout, env)
    if not cgx2.is_file() or not binder.is_file():
        raise Invalid('cgx2 or cgx2-bind binary unavailable')
    build_dir = output_dir / 'cpp-build'
    command = ['cmake', '-S', str(ROOT / 'bench/matrix_runtime_cpp'), '-B', str(build_dir),
               f'-DCMAKE_PREFIX_PATH={args.torch_root}',
               f'-DCMAKE_CXX_COMPILER={args.compiler}',
               f'-DCGX2_EXPECT_TORCH_VERSION={VERSION}', '-DCMAKE_BUILD_TYPE=Release']
    require_run(command, args.build_timeout, env)
    require_run(['cmake', '--build', str(build_dir), '--config', 'Release', '-j2'],
                args.build_timeout, env)
    cpp = build_dir / 'cgx2_matrix_baseline'
    if not cpp.is_file():
        raise Invalid('hand-written LibTorch C++ baseline unavailable')
    return cgx2.resolve(), binder.resolve(), cpp.resolve()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--torch-root', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--compiler', default='/usr/bin/c++')
    parser.add_argument('--cgx2', type=Path)
    parser.add_argument('--binder', type=Path)
    parser.add_argument('--python-torch', type=Path, help='optional Python with compatible CPU Torch 2.11.0')
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
    env.update({'OMP_NUM_THREADS': '1', 'MKL_NUM_THREADS': '1', 'OPENBLAS_NUM_THREADS': '1',
                'NUMEXPR_NUM_THREADS': '1', 'CUDA_VISIBLE_DEVICES': '',
                'LD_LIBRARY_PATH': str(args.package.resolve())})
    report = {'schema_version': 1, 'status': 'invalid', 'pinned_torch_version': VERSION,
              'inputs': {'package': str(args.package), 'torch_root': str(args.torch_root),
                         'compiler': str(args.compiler),
                         'python_torch': str(args.python_torch) if args.python_torch else None},
              'cases': [], 'runs': [], 'summary': {}, 'errors': [], 'exclusions': [],
              'unavailable': ['GPU', 'LLM task gate', 'exact source phase decomposition'],
              'settings': {'threads': 1, 'device': 'cpu', 'dtype': 'float64',
                           'row_major': True, 'numeric_tolerance': TOLERANCE,
                           'cold_reps': args.cold_reps, 'warm_reps': args.warm_reps,
                           'warmup': args.warmup, 'timeout_seconds': args.timeout,
                           'cold_budget_seconds': args.cold_budget_seconds},
              'host': {'platform': platform.platform(), 'logical_cores': os.cpu_count(),
                       'machine': platform.machine()}, 'toolchain_hashes': {},
              'package_hashes': {}}
    try:
        if platform.system() != 'Linux' or not Path('/usr/bin/time').is_file():
            raise Invalid('Linux and /usr/bin/time are required for per-process resources')
        package = args.package.resolve(strict=True)
        torch_root = args.torch_root.resolve(strict=True)
        report['package_hashes'] = package_checks(package)
        for library in ('libtorch.so', 'libtorch_cpu.so', 'libc10.so'):
            if digest(package / library) != digest(torch_root / 'lib' / library):
                raise Invalid(f'baseline LibTorch library differs from pinned package: {library}')
        with tempfile.TemporaryDirectory(prefix='cgx2-matrix-bench-') as temporary:
            workspace = Path(temporary)
            measured_package = clone_package(package, workspace / 'package')
            measured_env = dict(env, LD_LIBRARY_PATH=str(measured_package))
            cgx2, binder, cpp = build(args, workspace, measured_env)
            tool_files = (cgx2, binder, cpp, cgx2.with_name('cgx2-native'),
                          ROOT / 'Cargo.lock', ROOT / 'bench/matrix_runtime_cpp/main.cpp',
                          torch_root / 'include/torch/csrc/api/include/torch/torch.h',
                          torch_root / 'include/torch/csrc/api/include/torch/version.h',
                          Path(sys.executable).resolve(),
                          Path(shutil.which('cargo')).resolve(),
                          Path(shutil.which('rustc')).resolve(),
                          Path(shutil.which('cmake')).resolve())
            report['toolchain_hashes'] = {str(path): digest(path) for path in tool_files}
            report['toolchain_hashes'][str(Path(args.compiler).resolve())] = digest(Path(args.compiler).resolve())
            facade = measured_package / 'libcgx2_libtorch_facade.so'
            require_run([binder, 'check', measured_package], args.timeout, measured_env)
            python = args.python_torch.resolve(strict=True) if args.python_torch else None
            if python:
                report['toolchain_hashes'][str(python)] = digest(python)
            else:
                report['unavailable'].append('direct Python torch.matmul: no interpreter selected')
            for shape in CASES:
                m, k, n = shape
                write_source(measured_package, shape)
                case = {'shape': list(shape), 'reference_hash': None, 'routes': {}}
                report['cases'].append(case)
                expected = reference(*shape)
                commands = {
                    'source': [cgx2, 'run', measured_package],
                    'cpp': [cpp, 'baseline', 'cold', m, k, n, args.warmup, args.warm_reps, facade],
                    'facade': [cpp, 'facade', 'cold', m, k, n, args.warmup, args.warm_reps, facade],
                }
                if python:
                    commands['python'] = [python, ROOT / 'bench/matrix_runtime_torch.py',
                                          'cold', m, k, n, args.warmup, args.warm_reps]
                hashes = {}
                for route, command in commands.items():
                    validation = run_timed(command, args.timeout, measured_env)
                    report['runs'].append({'shape': list(shape), 'route': route,
                                           'phase': 'validation', **validation})
                    if validation['timeout'] or validation['exit_code'] != 0:
                        report['exclusions'].append({'shape': list(shape), 'route': route,
                                                     'phase': 'validation', 'reason':
                                                     'timeout' if validation['timeout'] else 'nonzero_exit'})
                        raise Invalid(f'{route} validation failed or timed out at {shape}')
                    hashes[route], payload = checked_output(validation, expected, route)
                    if route == 'python' and Path(payload['torch_root']).resolve() != torch_root:
                        raise Invalid('Python Torch installation differs from pinned header/library root')
                if len(set(hashes.values())) != 1:
                    raise Invalid(f'output hashes differ for shape {shape}: {hashes}')
                case['reference_hash'] = hashes['source']
                check = run_timed([cgx2, 'check', measured_package], args.timeout, measured_env)
                report['runs'].append({'shape': list(shape), 'route': 'source',
                                       'phase': 'check_manifest_closure', **check})
                if check['timeout'] or check['exit_code'] != 0:
                    report['exclusions'].append({'shape': list(shape), 'route': 'source',
                                                 'phase': 'check_manifest_closure', 'reason':
                                                 'timeout' if check['timeout'] else 'nonzero_exit'})
                    raise Invalid(f'source check failed or timed out at {shape}')
                for route in ('source', 'cpp', 'facade', 'python'):
                    if route not in commands:
                        continue
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
                        run = run_timed(commands[route], args.timeout, measured_env)
                        run.update({'shape': list(shape), 'route': route, 'phase': 'cold',
                                    'index': index})
                        report['runs'].append(run)
                        if run['timeout'] or run['exit_code'] != 0:
                            report['exclusions'].append({'shape': list(shape), 'route': route,
                                                         'phase': 'cold', 'index': index,
                                                         'reason': 'timeout' if run['timeout'] else 'nonzero_exit'})
                            raise Invalid(f'{route} cold run failed or timed out at {shape}')
                        hashed, _ = checked_output(run, expected, route)
                        if hashed != hashes[route]:
                            raise Invalid(f'{route} cold hash changed at {shape}')
                        samples.append(run['wall_ns'])
                    case['routes'][route] = {'cold_wall': interval(samples)}
                for route in ('cpp', 'facade', 'python'):
                    if route not in commands:
                        continue
                    command = list(commands[route])
                    command[2 if route != 'python' else 2] = 'warm'
                    warm = run_timed(command, args.timeout, measured_env)
                    warm.update({'shape': list(shape), 'route': route, 'phase': 'warm'})
                    report['runs'].append(warm)
                    if warm['timeout'] or warm['exit_code'] != 0:
                        report['exclusions'].append({'shape': list(shape), 'route': route,
                                                     'phase': 'warm', 'reason':
                                                     'timeout' if warm['timeout'] else 'nonzero_exit'})
                        raise Invalid(f'{route} warm run failed or timed out at {shape}')
                    hashed, payload = checked_output(warm, expected, route)
                    if hashed != hashes[route] or len(payload.get('samples', [])) != args.warm_reps:
                        raise Invalid(f'{route} warm sample count/hash mismatch at {shape}')
                    warm['samples'] = payload['samples']
                    case['routes'][route]['warm_inclusive_wall'] = interval(
                        [s['wall_ns'] for s in payload['samples']])
                    case['routes'][route]['warm_matmul_wall'] = interval(
                        [s['matmul_wall_ns'] for s in payload['samples']])
                    case['routes'][route]['warm_non_matmul_wall'] = interval(
                        [s['wall_ns'] - s['matmul_wall_ns'] for s in payload['samples']])
            report['status'] = 'valid'
            report['summary'] = {'scope': 'two CPU float64 matrix shapes only; source ceiling 32x32',
                                 'source_cold_vs_cpp': 'compare case cold wall intervals',
                                 'facade_warm_vs_cpp': 'compare transfer-inclusive wall intervals',
                                 'phase_limits': 'source check is a separate full check, not a subtractable phase; warm non-matmul aggregates input creation, output copy, releases and timer overhead; source adapter/copy individually unmeasured',
                                 'observations': observations(report['cases'])}
    except (Invalid, OSError, KeyError, json.JSONDecodeError) as error:
        report['errors'].append(str(error))
    args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    print(f'{report["status"]}: {args.out}')
    for error in report['errors']:
        print(error, file=sys.stderr)
    return 0 if report['status'] == 'valid' else 1


if __name__ == '__main__':
    sys.exit(main())
