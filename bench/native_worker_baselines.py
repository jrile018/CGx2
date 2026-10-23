"""Frozen Wave 24 fresh-process native and isolated Python evidence (Linux)."""

import argparse
from datetime import datetime, timezone
import gzip
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import shutil
import signal
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'bench/fixtures/native-worker/v1'
BASE = '425cb0a5467d98b2cf9565be19067b2f39f23fff'
THREADS = {key: '1' for key in ('OMP_NUM_THREADS', 'MKL_NUM_THREADS',
           'OPENBLAS_NUM_THREADS', 'NUMEXPR_NUM_THREADS', 'RAYON_NUM_THREADS')}
CONTROLS = dict(THREADS, LC_ALL='C', PYTHONHASHSEED='0')
RUST_FLAGS = ['--edition=2024', '-C', 'opt-level=3']


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def capture(argv):
    return subprocess.check_output(argv, cwd=ROOT, text=True).strip()


def artifact(path):
    path = Path(path).resolve()
    return {'path': str(path), 'sha256': digest(path), 'bytes': path.stat().st_size}


def archive_assembly(assembly, path):
    """Lossless deterministic packaging of large disassembly evidence."""
    with Path(path).open('xb') as output:
        output.write(gzip.compress(json.dumps(assembly, sort_keys=True).encode(), mtime=0))
    return artifact(path)


def source_state():
    # Freeze every tracked non-bench input, including Cargo.lock, not just HEAD.
    paths = capture(['git', 'ls-files']).splitlines()
    paths = [p for p in paths if not p.startswith('bench/')]
    observed = {p: digest(ROOT / p) for p in paths}
    frozen = {p: hashlib.sha256(subprocess.check_output(
        ['git', 'show', f'{BASE}:{p}'], cwd=ROOT)).hexdigest() for p in paths}
    if observed != frozen:
        raise ValueError('non-bench source differs from frozen base')
    return {'base': BASE, 'head': capture(['git', 'rev-parse', 'HEAD']),
            'tracked_non_bench_sha256': observed,
            'cargo_lock_sha256': digest(ROOT / 'Cargo.lock')}


def provenance(binary, manifest=None, role=None):
    actual = artifact(binary)
    status = 'unverified-profile'
    if manifest:
        record = json.loads(Path(manifest).read_text())
        source_state()  # Never accept a verified label against changed source.
        flags = ['cargo', '+1.98.1', 'build', '--offline', '--locked', '--release',
                 '-p', 'cgx2c', '-p', 'cgx2-cli']
        if (record.get('base') != BASE or record.get('build_prefix') != flags
                or record.get('rustc_version') != capture(['rustc', '+1.98.1', '-V'])
                or record.get('cargo_lock_sha256') != digest(ROOT / 'Cargo.lock')
                or record.get('binaries', {}).get(role, {}).get('sha256') != actual['sha256']):
            raise ValueError('build provenance mismatch')
        status = 'verified-release-build-record'
    return dict(actual, profile=status)


def schedule(count=30, warmups=2):
    rng = random.Random(240024)
    rows = []
    for case in ('scalar_aot', 'python_child'):
        for index in range(-warmups, count):
            order = ['cgx2', 'baseline']
            rng.shuffle(order)
            rows.append({'case': case, 'pair': index, 'warmup': index < 0,
                         'args': ['a', 'b'][:index % 3] if case == 'scalar_aot' else [],
                         'order': order})
    return rows


def assess(row, expected='', code=0, category=None):
    if row['status'] != 'completed':
        return False
    return (row['exit_status'] == code and row['stdout'] == expected
            and (category in row['stderr'] if category else not row['stderr']))


def run(argv, env, directory, timeout=30):
    """Retain failures; time wrapper reaps children and supplies per-run rusage."""
    argv = [str(x) for x in argv]
    fd, metric = tempfile.mkstemp(prefix='usage-', dir=directory)
    os.close(fd)
    row = {'command': argv, 'cwd': str(ROOT), 'deadline_s': timeout,
           'started_utc': datetime.now(timezone.utc).isoformat()}
    start = time.perf_counter()
    try:
        # Preflight avoids labeling GNU time's 127 as an ordinary target exit.
        if not Path(argv[0]).is_file():
            raise FileNotFoundError(argv[0])
        proc = subprocess.Popen(['/usr/bin/time', '-f', '%U %S %M', '-o', metric,
                                 '--', *argv], cwd=ROOT, env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                start_new_session=True)
        try:
            stdout, stderr = proc.communicate(timeout=timeout)
            row['status'] = 'completed'
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            stdout, stderr = proc.communicate()
            row['status'] = 'timeout'
        row.update(exit_status=proc.returncode,
                   stdout=stdout.decode(errors='replace'), stderr=stderr.decode(errors='replace'))
    except OSError as error:
        row.update(status='spawn-error', exit_status=None, stdout='', stderr=str(error))
    row['wall_s'] = time.perf_counter() - start
    raw = Path(metric).read_text()
    row['usage_raw'] = raw
    matches = re.findall(r'^([0-9.]+) ([0-9.]+) (\d+)$', raw, re.M)
    if matches:
        user, system, rss = matches[-1]
        row.update(user_cpu_s=float(user), system_cpu_s=float(system), peak_rss_kib=int(rss))
    else:
        row.update(user_cpu_s=None, system_cpu_s=None, peak_rss_kib=None)
    Path(metric).unlink()
    return row


def distribution(values):
    if not values:
        return {'n': 0}
    ordered = sorted(values)
    return {'n': len(values), 'median': statistics.median(values),
            'min': min(values), 'max': max(values),
            'p95': ordered[math.ceil(.95 * len(values)) - 1] if len(values) >= 20 else None}


def summarize(rows):
    result = {}
    for case in ('scalar_aot', 'python_child'):
        measured = [r for r in rows if r['case'] == case and not r['warmup']]
        valid = [r for r in measured if all(x['valid'] for x in r['runs'].values())]
        entry = {'attempted_pairs': len(measured), 'valid_pairs': len(valid),
                 'excluded_pair_ids': [r['pair'] for r in measured if r not in valid]}
        for route in ('cgx2', 'baseline'):
            # Every successful timing retained, including the survivor of a failed pair.
            good = [r['runs'][route] for r in measured if r['runs'][route]['valid']]
            entry[route] = {metric: distribution([r[metric] for r in good if r[metric] is not None])
                            for metric in ('wall_s', 'user_cpu_s', 'system_cpu_s', 'peak_rss_kib')}
        ratios = [r['runs']['cgx2']['wall_s'] / r['runs']['baseline']['wall_s'] for r in valid]
        entry['paired_wall_ratio'] = distribution(ratios)
        if len(ratios) >= 30:
            rng = random.Random(240025)
            estimates = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(5000))
            entry['paired_wall_ratio']['bootstrap_95'] = [estimates[124], estimates[4874]]
        result[case] = entry
    return result


def oracle(args):
    x = len(args) + 17
    for i in range(20_000 + len(args)):
        x = (31 * x + i) % 1_000_000_007
    return f'{x}\n'


def trace_audit(argv, env, directory, python_executable, expect_python):
    """Untimed exec trace and /proc maps; never used as timing observations."""
    trace_path = Path(directory) / f'audit-{time.monotonic_ns()}.trace'
    proc = subprocess.Popen(['/usr/bin/strace', '-f', '-e', 'trace=process', '-o', str(trace_path),
                             *map(str, argv)], env=env, cwd=ROOT,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    maps = {}
    deadline = time.monotonic() + 30
    timed_out = False
    while proc.poll() is None:
        if time.monotonic() > deadline:
            os.killpg(proc.pid, signal.SIGKILL)
            timed_out = True
            break
        pending = [proc.pid]
        while pending:
            pid = pending.pop()
            try:
                pending.extend(map(int, Path(f'/proc/{pid}/task/{pid}/children').read_text().split()))
                exe = str(Path(f'/proc/{pid}/exe').resolve(strict=True))
                if exe not in maps:
                    maps[exe] = Path(f'/proc/{pid}/maps').read_text()
            except (FileNotFoundError, ProcessLookupError, PermissionError):
                pass
        time.sleep(.0005)
    stdout, stderr = proc.communicate()
    trace = trace_path.read_text()
    executed = re.findall(r'execve\("([^"]+)".*= 0', trace)
    selected = str(Path(python_executable).resolve())
    worker_execs = [p for p in executed if str(Path(p).resolve()) == selected]
    native_maps = {k: v for k, v in maps.items() if Path(k).name in ('scalar-cgx2', 'cgx2-native', 'cgx2')}
    worker_maps = {k: v for k, v in maps.items() if k == selected}
    return {'command': list(map(str, argv)), 'stdout': stdout.decode(), 'stderr': stderr.decode(),
            'exit_status': proc.returncode, 'timeout': timed_out, 'trace': trace,
            'executed': executed, 'sampled_maps': maps,
            'valid': not timed_out and proc.returncode == 0 and bool(native_maps)
                     and all('libpython' not in v.lower() for v in native_maps.values())
                     and bool(worker_execs) == expect_python
                     and (bool(worker_maps) if expect_python else True)}


def boundary_audit(argv, env, directory, python_executable, expect_python):
    """Sample real process trees without ptrace; retain a blocked trace attempt."""
    trace = trace_audit(argv, env, directory, python_executable, expect_python)
    if trace['valid']:
        return dict(trace, method='strace+proc')
    proc = subprocess.Popen(list(map(str, argv)), env=env, cwd=ROOT,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    maps, processes, snapshots = {}, {}, []
    timed_out = False
    deadline = time.monotonic() + 30
    while proc.poll() is None:
        if time.monotonic() > deadline:
            os.killpg(proc.pid, signal.SIGKILL)
            timed_out = True
            break
        pending = [proc.pid]
        tree = []
        while pending:
            pid = pending.pop()
            try:
                children = list(map(int, Path(f'/proc/{pid}/task/{pid}/children').read_text().split()))
                pending.extend(children)
                exe = str(Path(f'/proc/{pid}/exe').resolve(strict=True))
                cmdline = Path(f'/proc/{pid}/cmdline').read_bytes().decode(errors='replace').split('\0')
                # Re-sample an existing executable after Python initialization too.
                mapped = Path(f'/proc/{pid}/maps').read_text()
                if exe not in maps or len(mapped) > len(maps[exe]):
                    maps[exe] = mapped
                processes[f'{pid}:{exe}'] = {'pid': pid, 'executable': exe, 'cmdline': cmdline}
                tree.append({'pid': pid, 'executable': exe, 'children': children})
            except (FileNotFoundError, ProcessLookupError, PermissionError):
                pass
        snapshots.append(tree)
        time.sleep(.0001)
    stdout, stderr = proc.communicate()
    selected = str(Path(python_executable).resolve())
    native_maps = {k: v for k, v in maps.items() if Path(k).name in ('scalar-cgx2', 'cgx2-native', 'cgx2')}
    workers = [p for p in processes.values() if p['executable'] == selected]
    return {'method': 'proc-sampling', 'trace_attempt': trace,
            'limitation': 'Sampled process trees cannot exclude arbitrarily short-lived unobserved processes.',
            'command': list(map(str, argv)), 'exit_status': proc.returncode, 'timeout': timed_out,
            'stdout': stdout.decode(), 'stderr': stderr.decode(), 'sampled_maps': maps,
            'processes': list(processes.values()), 'snapshots': snapshots,
            'valid': not timed_out and proc.returncode == 0 and bool(native_maps)
                     and all('libpython' not in v.lower() for v in native_maps.values())
                     and bool(workers) == expect_python
                     and all('-I' in p['cmdline'] and '-S' in p['cmdline'] for p in workers)}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--python', type=Path, default=Path('/usr/bin/python3'))
    parser.add_argument('--binary-dir', type=Path)
    parser.add_argument('--build-provenance', type=Path)
    args = parser.parse_args(argv)
    args.out = args.out.resolve()
    if args.out.exists():
        parser.error('--out must be new; preserve historical observations')
    work_root = ROOT / 'bench/target'
    work_root.mkdir(exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix='wave24-', dir=work_root))
    temporary = work / 'tmp'
    temporary.mkdir()
    env = dict(os.environ, **CONTROLS, TMPDIR=str(temporary))
    for key in ('CGX2_PYTHON_WORKER', 'CGX2_PYTHON_TRACE', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
                'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTFLAGS'):
        env.pop(key, None)
    env['CGX2_PYTHON'] = str(args.python.resolve())
    env['CARGO_TARGET_DIR'] = str(work / 'cargo')
    report = {'schema': 1, 'preregistration_sha256': digest(ROOT / 'bench/WAVE24-NATIVE-WORKER-PREREGISTRATION.md'),
              'work_directory': str(work), 'schedule': schedule(), 'setup': [], 'phases': [],
              'pairs': [], 'negative_probes': [], 'audits': [], 'errors': []}

    def save():
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')

    def execute(command, expected='', code=0, category=None, timeout=30, custom_env=None):
        row = run(command, custom_env or env, temporary, timeout)
        row['valid'] = assess(row, expected, code, category)
        return row

    try:
        report['source'] = source_state()
        report['fixtures'] = {str(p.relative_to(ROOT)): {'sha256': digest(p), 'source': p.read_text()}
                              for p in sorted(FIXTURES.iterdir())}
        report['harness_sha256'] = digest(__file__)
        report['harness_source'] = Path(__file__).read_text()
        report['host'] = {'platform': platform.platform(), 'cpuinfo': Path('/proc/cpuinfo').read_text(),
                          'affinity': sorted(os.sched_getaffinity(0)), 'load': os.getloadavg(),
                          'meminfo': Path('/proc/meminfo').read_text(),
                          'wall_clock': vars(time.get_clock_info('perf_counter')),
                          'time_version': capture(['/usr/bin/time', '--version']),
                          'strace_version': capture(['/usr/bin/strace', '-V'])}
        report['environment'] = {key: value for key, value in env.items() if key in
                                 {*CONTROLS, 'TMPDIR', 'CARGO_TARGET_DIR', 'CGX2_PYTHON', 'PATH',
                                  'LD_LIBRARY_PATH', 'LIBRARY_PATH', 'CARGO_HOME', 'RUSTUP_TOOLCHAIN'}}
        report['toolchains'] = {name: {'version': capture([name, '+1.98.1', '-V']),
                                       **artifact(capture(['rustup', 'which', '--toolchain', '1.98.1', name]))}
                                for name in ('rustc', 'cargo')}
        if not args.python.resolve().is_file():
            missing = execute([args.python, '-I', '-S', '-c', 'pass'])
            report['setup'].append(missing)
            raise ValueError('selected interpreter missing')
        report['python'] = {**artifact(args.python), 'identity': 'local:cpython (unpinned)',
                            'provenance': capture([str(args.python), '-I', '-S', '-c',
                                'import sys,json; print(json.dumps(dict(executable=sys.executable,version=sys.version,prefix=sys.prefix,base_prefix=sys.base_prefix)))'])}
        build_prefix = ['cargo', '+1.98.1', 'build', '--offline', '--locked', '--release', '-p', 'cgx2c', '-p', 'cgx2-cli']
        if args.binary_dir:
            binary_dir = args.binary_dir.resolve()
            manifest = args.build_provenance
        else:
            build = execute([shutil.which('cargo'), *build_prefix[1:]], timeout=600)
            # Cargo emits progress on stderr, not program stdout.
            build['valid'] = build['status'] == 'completed' and build['exit_status'] == 0
            report['setup'].append(build)
            save()
            if not build['valid']:
                raise ValueError('release Cargo build failed')
            binary_dir = work / 'cargo/release'
            manifest = work / 'build-provenance.json'
            manifest.write_text(json.dumps({'base': BASE, 'build_prefix': build_prefix,
                'rustc_version': report['toolchains']['rustc']['version'],
                'cargo_lock_sha256': report['source']['cargo_lock_sha256'],
                'binaries': {name: artifact(binary_dir / name) for name in ('cgx2c', 'cgx2', 'cgx2-native')}}))
        report['build_provenance'] = json.loads(manifest.read_text()) if manifest else None
        report['binaries'] = {name: provenance(binary_dir / name, manifest, name)
                              for name in ('cgx2c', 'cgx2', 'cgx2-native')}
        cli, compiler = binary_dir / 'cgx2', binary_dir / 'cgx2c'
        rustc = report['toolchains']['rustc']['path']
        scalar = FIXTURES / 'scalar.cg'
        python_source = FIXTURES / 'python.cg'
        cg_binary, rust_binary = work / 'scalar-cgx2', work / 'scalar-rust'
        for index in range(5):
            commands = [('check_scalar', [cli, 'check', scalar]),
                        ('check_python', [cli, 'check', python_source]),
                        ('build_cgx2', [compiler, 'build', scalar, '--out', cg_binary if index == 4 else work / f'cg-{index}',
                                        '--release', '--rustc', rustc]),
                        ('build_rust', [rustc, *RUST_FLAGS, FIXTURES / 'scalar.rs', '-o',
                                       rust_binary if index == 4 else work / f'rust-{index}'])]
            for phase, command in commands:
                row = execute(command, timeout=120 if phase.startswith('build') else 30)
                # Build/check tools can print metadata; success is then output-artifact checked below.
                row['valid'] = row['status'] == 'completed' and row['exit_status'] == 0
                if phase.startswith('build') and row['valid']:
                    output = command[command.index('--out') + 1] if '--out' in command else command[command.index('-o') + 1]
                    row['artifact'] = artifact(output)
                report['phases'].append({'phase': phase, 'sample': index, **row})
                save()
                if not row['valid']:
                    raise ValueError(f'{phase} failed')
        for path in (cg_binary, rust_binary, cli, binary_dir / 'cgx2-native'):
            data = artifact(path)
            data['dynamic'] = capture(['/usr/bin/readelf', '-d', str(path)])
            data['ldd'] = capture(['/usr/bin/ldd', str(path)])
            data['no_libpython'] = 'libpython' not in (data['dynamic'] + data['ldd']).lower()
            report.setdefault('elf', []).append(data)
        emitted = work / 'scalar-emitted.rs'
        emit = execute([compiler, 'emit-rust', scalar, '--out', emitted])
        emit['valid'] = emit['status'] == 'completed' and emit['exit_status'] == 0 and emitted.is_file()
        report['setup'].append(emit)
        report['generated_rust'] = {**artifact(emitted), 'source': emitted.read_text()}
        report['assembly_archive'] = archive_assembly(
            {str(path): capture(['/usr/bin/objdump', '-d', '--demangle', str(path)])
             for path in (cg_binary, rust_binary)}, Path(str(args.out) + '.assembly.json.gz'))
        expected = {n: oracle(['a'] * n) for n in range(3)}
        commands = {'scalar_aot': {'cgx2': [cg_binary], 'baseline': [rust_binary]},
                    'python_child': {'cgx2': [cli, 'run', python_source],
                                     'baseline': [args.python.resolve(), '-I', '-S', FIXTURES / 'python.py',
                                                  '9223372030926249001']}}
        report['oracle'] = {'scalar': expected, 'python_child': '3037000499\n'}
        report['ordinary_run'] = execute([cli, 'run', scalar, '--', 'a'], expected[1])
        for planned in report['schedule']:
            pair = {**planned, 'runs': {}}
            report['pairs'].append(pair)
            for route in planned['order']:
                oracle_value = expected[len(planned['args'])] if planned['case'] == 'scalar_aot' else '3037000499\n'
                pair['runs'][route] = execute([*commands[planned['case']][route], *planned['args']], oracle_value)
                save()
        # Explicit worker diagnostics, with each failure followed by a good fresh run.
        good_source = python_source.read_text()
        for name, replacement, category, probe_env in [
            ('negative_isqrt', good_source.replace('9223372030926249001', '0 - 1'), 'CGR020', env),
            ('wrong_shape', good_source.replace('math.isqrt', 'math.sqrt').replace('9223372030926249001', '4'), 'CGR021', env),
            ('missing_interpreter', good_source, 'CGR017', dict(env, CGX2_PYTHON=str(work / 'missing-python')))]:
            fixture = work / f'{name}.cg'
            fixture.write_text(replacement)
            row = execute([cli, 'run', fixture], code=1, category=category, custom_env=probe_env)
            recovery = execute([cli, 'run', python_source], '3037000499\n')
            report['negative_probes'].append({'name': name, 'category': category, 'source': replacement,
                                               'env_overrides': {'CGX2_PYTHON': probe_env['CGX2_PYTHON']},
                                               'failure': row, 'recovery': recovery})
        for command, expect_python in [([cg_binary, 'a'], False), ([cli, 'run', scalar, '--', 'a'], False),
                                       ([cli, 'run', python_source], True)]:
            audit = boundary_audit(command, env, temporary, args.python, expect_python)
            audit['valid'] = audit['valid'] and audit['stdout'] == (expected[1] if not expect_python else '3037000499\n')
            report['audits'].append(audit)
            save()
        report['summary'] = summarize(report['pairs'])
        report['phase_summary'] = {phase: distribution([r['wall_s'] for r in report['phases']
                                  if r['phase'] == phase and r['valid']]) for phase in
                                  ('check_scalar', 'check_python', 'build_cgx2', 'build_rust')}
        report['valid'] = (all(r['valid'] for r in report['setup']) and report['ordinary_run']['valid']
                           and all(r['valid'] for p in report['pairs'] for r in p['runs'].values())
                           and all(p['failure']['valid'] and p['recovery']['valid'] for p in report['negative_probes'])
                           and all(r['valid'] for r in report['audits']) and all(r['no_libpython'] for r in report['elf']))
    except Exception as error:
        report['errors'].append(f'{type(error).__name__}: {error}')
        report['valid'] = False
    finally:
        save()
    print(json.dumps({'out': str(args.out), 'valid': report['valid'], 'errors': report['errors']}))
    return 0 if report['valid'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
