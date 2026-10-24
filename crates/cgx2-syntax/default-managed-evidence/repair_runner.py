#!/usr/bin/env python3
"""Bounded managed repair matrix. Every failure is retained; never a release gate."""
import argparse
import hashlib
import gzip
import json
import os
from pathlib import Path
import re
import resource
import shlex
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / 'owned-result-runner-evidence'))
from supervisor import supervise, evidence

FIXTURES = HERE / 'repair-fixtures'
SENTINEL = b'previous artifact\x00\xff\n'
LIMITS = dict(wall_seconds=180, cpu_seconds_per_process=120,
              address_space_bytes_per_process=4 * 1024**3,
              file_size_bytes_per_process=512 * 1024**2,
              retained_bytes_per_stream=2 * 1024**2,
              aggregate_cgroup_isolation=False)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def cli_library(binary_dir):
    """Use the final Cargo/frozen library, never a stale feature-profile guess."""
    canonical = Path(binary_dir) / 'libcgx2_cli.rlib'
    if canonical.is_file():
        return canonical
    candidates = sorted((Path(binary_dir) / 'deps').glob('libcgx2_cli-*.rlib'))
    return candidates[0] if len(candidates) == 1 else None


def limits():
    for kind, value in ((resource.RLIMIT_AS, LIMITS['address_space_bytes_per_process']),
                        (resource.RLIMIT_CPU, LIMITS['cpu_seconds_per_process']),
                        (resource.RLIMIT_FSIZE, LIMITS['file_size_bytes_per_process'])):
        resource.setrlimit(kind, (value, value))


def normal_exit(record, expected=0):
    return record['reason'] == 'exit' and record['exit'] == expected and not record.get('signal')


def accounting(record):
    rows = [json.loads(line.split(' ', 1)[1]) for line in record['stderr'].splitlines()
            if line.startswith('CGX2_MANAGED_EVIDENCE ')]
    if len(rows) > 1:
        raise ValueError('multiple managed observations')
    return rows[0] if rows else None


def stable(observation):
    result = json.loads(json.dumps(observation))
    for region in result['memory']['regions']:
        region['region_id']['session'] = 0
        region.pop('pause')
    return result


def diagnostics(record):
    """Retain code, message and byte spans from both public output formats."""
    found = []
    for line in record['stderr'].splitlines():
        if line.startswith('{'):
            try:
                row = json.loads(line)
            except ValueError:
                continue
            if 'code' in row and 'message' in row:
                found.append(dict(code=row['code'], message=row['message'],
                                  span=row.get('span'), file=row.get('file')))
                continue
        m = re.fullmatch(r'(CG[A-Z0-9]+): (.*)', line)
        if not m:
            continue
        code, text = m.groups()
        location = re.fullmatch(r'(.+):(\d+)\.\.(\d+): (.*)', text)
        native = re.fullmatch(r'(.*) at (\d+)\.\.(\d+)', text)
        if location:
            file, start, end, message = location.groups()
        elif native:
            message, start, end = native.groups()
            file = None
        else:
            file, start, end, message = None, None, None, text
        found.append(dict(code=code, message=message, file=file,
                          span=None if start is None else dict(start=int(start), end=int(end))))
    return found


def rejection_matches(record, expected):
    actual = diagnostics(record)
    return normal_exit(record, 1) and len(actual) == len(expected) and all(
        row['code'] == spec['code'] and row['span'] == spec['span'] and
        row['message'] == spec['message'] and
        (row['file'] or '').endswith(spec['file'])
        for row, spec in zip(actual, expected))


def preserved(path):
    return path.is_file() and path.read_bytes() == SENTINEL


def cleanup_ok(observation):
    return observation is not None and observation['final_roots'] == observation['baseline_roots'] == 0 and observation['cleanup_failures'] == 0 and observation['roots_registered'] == observation['roots_released']


def classify(record):
    """An uncaught error exit is an observed failure, never recovered success."""
    obs = accounting(record)
    if not normal_exit(record, record['exit']) or record['exit'] not in (0, 1):
        return 'unexpected-exit-or-signal'
    if record['exit'] == 1:
        if not diagnostics(record):
            return 'unexpected-error-without-diagnostic'
        if obs is None:
            return 'uncaught-session-entry'
        return 'uncaught-guard-entry' if obs['guard_entry_failures'] else 'uncaught-runtime'
    if obs is None:
        return 'unexpected-success-without-observation'
    return 'recovered-inside-expression' if obs['recovered'] else 'fault-index-not-reached'


class Runner:
    def __init__(self, binary_dir, output):
        self.binary = binary_dir.resolve() / 'cgx2'
        self.output = output.resolve()
        self.output.mkdir(parents=True, exist_ok=False)
        self.work = Path(tempfile.mkdtemp(prefix='managed-repair-', dir=os.environ['TMPDIR']))
        self.env = {**os.environ, 'RUSTUP_TOOLCHAIN': '1.98.1', 'CARGO_BUILD_JOBS': '2',
                    'CGX2_MANAGED_EVIDENCE': '1', 'CGX2_OWNED_EVIDENCE': '1'}
        self.report = dict(schema_version=2, base='3c1cfe2aeba2f7135e4069709bf633c082d2aab0',
                           checkpoint_complete=False, release_complete=False,
                           scope='managed evidence repair only; coordinator acceptance required',
                           leak_negative_control='historical host block remains open; not rerun',
                           external_audit_performance='unrun', bounds=LIMITS, checks=[],
                           outcomes=[], artifacts_sha256={}, sources_sha256={},
                           binary_sha256=digest(self.binary),
                           runner_sha256=digest(__file__),
                           supervisor_sha256=digest(HERE.parent / 'owned-result-runner-evidence/supervisor.py'),
                           runtime_sha256={str(p.relative_to(ROOT)): digest(p) for p in
                                           sorted((ROOT / 'crates/cgx2-runtime/src').rglob('*.rs'))})
        (self.output / 'runner.py.gz').write_bytes(gzip.compress(Path(__file__).read_bytes(), mtime=0))
        self.save()

    def save(self):
        checks = self.report['checks']
        self.report.update(passed=sum(r['passed'] for r in checks),
                           failed=sum(not r['passed'] for r in checks))
        (self.output / 'repair.json').write_text(json.dumps(self.report, indent=2) + '\n')

    def check(self, label, condition, **detail):
        self.report['checks'].append(dict(label=label, passed=bool(condition), **detail))
        self.save()
        if not condition:
            print('FAIL', label, flush=True)
        return bool(condition)

    def command(self, label, argv, expected=0, extra_env=None):
        result = supervise([str(a) for a in argv], cwd=self.work,
                           env={**self.env, **(extra_env or {})}, limits=limits,
                           cap=LIMITS['retained_bytes_per_stream'], timeout=LIMITS['wall_seconds'])
        record = dict(label=label, command=[str(a) for a in argv], cwd=str(self.work),
                      **evidence(result))
        record['diagnostics'] = diagnostics(record)
        self.check(label, normal_exit(record, expected) if expected is not None else (normal_exit(record, 0) or normal_exit(record, 1)), **{k: v for k, v in record.items() if k != 'label'})
        return record

    def source(self, source):
        for p in sorted(source.rglob('*')) if source.is_dir() else [source]:
            if p.is_file():
                self.report['sources_sha256'][str(p.relative_to(ROOT))] = digest(p)
                snapshot = self.output / 'sources' / p.relative_to(ROOT)
                snapshot.parent.mkdir(parents=True, exist_ok=True)
                snapshot.with_suffix(snapshot.suffix + '.gz').write_bytes(gzip.compress(p.read_bytes(), mtime=0))
        self.save()

    def artifact(self, path):
        if path.is_file():
            self.report['artifacts_sha256'][path.name] = digest(path)
            self.save()

    def prepare(self):
        pinned = self.command('pinned-rustc', ['rustup', 'which', 'rustc'])['stdout'].strip()
        self.report['rustc_identity'] = self.command('rustc-identity', [pinned, '-vV'])['stdout']
        self.wrapper = self.work / 'rustc-linker'
        self.wrapper.write_text('#!/bin/sh\nexec ' + shlex.quote(pinned) + ' -Clink-arg=-Wl,--threads=2 "$@"\n')
        self.wrapper.chmod(0o700)
        return pinned

    def build(self, name, source, release=False, index=None):
        artifact = self.work / (name + ('-release' if release else '-debug'))
        argv = [self.binary, 'build', source, '--out', artifact, '--rustc', self.wrapper]
        if release:
            argv.append('--release')
        if index is not None:
            argv += ['--test-fail-reserve', str(index)]
        result = self.command(artifact.name + ':build', argv)
        self.artifact(artifact)
        return artifact if normal_exit(result) and artifact.exists() else None

    def emit(self, name, source):
        emitted = self.work / (name + '.rs')
        result = self.command(name + ':emit', [self.binary, 'emit', source, '--target', 'rust', '--out', emitted])
        self.artifact(emitted)
        return emitted if normal_exit(result) and emitted.exists() else None

    def standalone(self, name, emitted, index=None):
        artifact = self.work / (name + '-standalone')
        argv = [self.wrapper, '--edition=2024', emitted, '-o', artifact]
        extra = None
        if index is not None:
            argv += ['--cfg', 'feature="fault-injection"']
            extra = {'CGX2_TEST_FAIL_RESERVE': str(index)}
        result = self.command(artifact.name + ':compile', argv, extra_env=extra)
        self.artifact(artifact)
        return artifact if normal_exit(result) and artifact.exists() else None

    def parity(self, label, actual, reference):
        a, b = accounting(actual), accounting(reference)
        self.check(label + ':primary-result-and-memory-parity',
                   actual['reason'] == reference['reason'] == 'exit' and
                   actual['exit'] == reference['exit'] and actual['stdout'] == reference['stdout'] and
                   [(d['code'], d['message'], d['span']) for d in diagnostics(actual)] ==
                   [(d['code'], d['message'], d['span']) for d in diagnostics(reference)] and
                   stable(a) == stable(b) if a is not None and b is not None else
                   normal_exit(actual, 1) and normal_exit(reference, 1) and
                   actual['stdout'] == reference['stdout'] and
                   [(d['code'], d['message'], d['span']) for d in diagnostics(actual)] ==
                   [(d['code'], d['message'], d['span']) for d in diagnostics(reference)] and a == b,
                   actual_observation=a, expected_observation=b)

    def budgets(self):
        cases = [(f'alloc-{n}', 42 if n < 226 else 7, int(n < 226), 1) for n in (0, 225, 226)]
        cases += [(f'roots-{n}', 42, int(n < 64), 1) for n in (0, 63, 64)]
        cases += [(f'nested-{n}', 42 if n < 258 else 7, int(n < 258), 2) for n in (257, 258)]
        cases += [('nonrefund-451', 42, 1, 1), ('hidden-scalar', 42, 1, 1), ('hidden-package', 42, 1, 1)]
        for name, value, recovered, guards in cases:
            print('budget', name, flush=True)
            source = FIXTURES / (name if name.endswith('package') else name + '.cg')
            self.source(source)
            reference = self.command(name + ':interpreter', [self.binary, 'run', source])
            self.budget_result(name + ':interpreter', reference, value, recovered, guards)
            targets = [self.build(name, source, release) for release in (False, True)]
            emitted = self.emit(name, source)
            if emitted:
                targets.append(self.standalone(name, emitted))
            for artifact in filter(None, targets):
                actual = self.command(artifact.name + ':execute', [artifact])
                self.budget_result(artifact.name, actual, value, recovered, guards)
                self.parity(artifact.name, actual, reference)

    def budget_result(self, label, record, value, recovered, guards):
        obs = accounting(record)
        self.check(label + ':real-budget-result', normal_exit(record) and record['stdout'] == f'{value}\n' and
                   cleanup_ok(obs) and obs['recovered'] == recovered and obs['guard_entries'] == obs['guard_exits'] == guards and
                   obs['memory']['totals']['collection_count'] == 0,
                   evidence_kind='source-level-real-budget', expected_recovered=recovered, observation=obs)

    def driver(self):
        source = HERE / 'fault_driver.rs'
        self.source(source)
        driver = self.work / 'fault-driver'
        deps = self.binary.parent / 'deps'
        library = cli_library(self.binary.parent)
        if library is None:
            self.check('fault-driver:unambiguous-feature-library', False,
                       libraries=[str(p) for p in sorted(deps.glob('libcgx2_cli-*.rlib'))])
            return None
        self.artifact(library)
        record = self.command('fault-driver:compile', [self.wrapper, '--edition=2021', source,
                              '--extern', 'cgx2_cli=' + str(library), '-L', 'dependency=' + str(deps), '-o', driver])
        self.artifact(driver)
        return driver if normal_exit(record) else None

    def faults(self):
        driver = self.driver()
        if driver is None:
            return
        for name in ('fault-graph-root', 'fault-guard'):
            source = FIXTURES / (name + '.cg')
            self.source(source)
            emitted = self.emit(name, source)
            for index in range(1, 16):
                label = f'{name}-{index}'
                print('synthetic', label, flush=True)
                reference = self.command(label + ':interpreter', [driver, source, str(index)], expected=None)
                self.fault_result(label + ':interpreter', reference, name, index)
                targets = [self.build(label, source, release, index) for release in (False, True)]
                if emitted:
                    targets.append(self.standalone(label, emitted, index))
                for artifact in filter(None, targets):
                    actual = self.command(artifact.name + ':execute', [artifact], expected=None)
                    self.fault_result(artifact.name, actual, name, index)
                    self.parity(artifact.name, actual, reference)

    def fault_result(self, label, record, fixture, index):
        classification = classify(record)
        guarded = fixture == 'fault-guard'
        if index == 1:
            expected_class = 'uncaught-session-entry'
        elif guarded and index == 2:
            expected_class = 'uncaught-guard-entry'
        elif index in (range(3, 8) if guarded else range(2, 7)):
            expected_class = 'recovered-inside-expression'
        elif index in ((8, 9) if guarded else (7, 8)):
            expected_class = 'uncaught-runtime'
        else:
            expected_class = 'fault-index-not-reached'
        obs, ds = accounting(record), diagnostics(record)
        if classification.startswith('uncaught-'):
            valid = normal_exit(record, 1) and record['stdout'] == '' and len(ds) == 1 and ds[0]['code'] == 'CGR090' and ds[0]['span'] is not None
            if obs is not None:
                valid = valid and cleanup_ok(obs) and obs['recovered'] == 0 and obs['memory']['totals']['collection_count'] == 0
        else:
            valid = normal_exit(record) and cleanup_ok(obs) and not ds and obs['memory']['totals']['collection_count'] == 0
            if classification == 'recovered-inside-expression':
                valid = valid and record['stdout'] == '43\n' and obs['recovered'] == 1
            elif classification == 'fault-index-not-reached':
                valid = valid and record['stdout'] == '7\n' and obs['recovered'] == 0
            else:
                valid = False
        self.report['outcomes'].append(dict(label=label, classification=classification,
                                           evidence_kind='synthetic-host-reserve-graph-root-path',
                                           diagnostics=ds, observation=obs, exit=record['exit'],
                                           signal=record.get('signal'), reason=record['reason']))
        self.check(label + ':synthetic-outcome', valid and classification == expected_class,
                   classification=classification, expected_classification=expected_class,
                   reserve_index=index, coverage='supplementary-session-entry' if index == 1 else 'required-2-through-15')
        return classification

    def rejections(self):
        for name, source, expected in rejection_cases():
            print('reject', name, flush=True)
            self.source(source)
            record = self.command(name + ':check-reject', [self.binary, '--diagnostic-format=json', 'check', source], expected=1)
            self.check(name + ':check-diagnostic', rejection_matches(record, expected), expected=expected)
            commands = [('native-debug', ['build']), ('native-release', ['build']),
                        ('rust', ['emit', '--target', 'rust']), ('python', ['emit', '--target', 'python'])]
            for target, operation in commands:
                artifact = self.work / (name + '-' + target)
                artifact.write_bytes(SENTINEL)
                argv = [self.binary, '--diagnostic-format=json', operation[0], source, *operation[1:], '--out', artifact, '--force']
                if target.startswith('native'):
                    argv += ['--rustc', self.wrapper] + (['--release'] if target.endswith('release') else [])
                record = self.command(name + ':' + target + '-reject', argv, expected=1)
                target_expected = expected[:1] if name == 'dishonest-package' else expected
                self.check(name + ':' + target + ':diagnostic-and-preserved', rejection_matches(record, target_expected) and preserved(artifact),
                           expected=target_expected, sentinel_sha256=hashlib.sha256(SENTINEL).hexdigest(),
                           artifact_sha256=digest(artifact) if artifact.is_file() else None)

    def runtime_errors(self):
        for name, code, message, start, end in [('arithmetic', 'CGR006', 'division by zero', 78, 83),
                                                ('arg-negative', 'CGR007', 'arg index must be nonnegative', 62, 64),
                                                ('contract', 'CGR015', 'requires condition failed', 35, 40),
                                                ('failed-arguments', 'CGR006', 'division by zero', 104, 109)]:
            source = HERE / (name + '.cg')
            self.source(source)
            reference = self.command(name + ':interpreter-error', [self.binary, 'run', source], expected=1)
            obs, ds = accounting(reference), diagnostics(reference)
            self.check(name + ':primary-and-cleanup', cleanup_ok(obs) and obs['recovered'] == 0 and len(ds) == 1 and ds[0]['code'] == code and ds[0]['message'] == message and ds[0]['span'] == dict(start=start, end=end))
            targets = [self.build(name, source, release) for release in (False, True)]
            emitted = self.emit(name, source)
            if emitted:
                targets.append(self.standalone(name, emitted))
            for artifact in filter(None, targets):
                actual = self.command(artifact.name + ':error', [artifact], expected=1)
                self.parity(artifact.name, actual, reference)


def rejection_cases():
    def spec(code, file, start, end, message):
        return dict(code=code, file=file, span=dict(start=start, end=end), message=message)
    ref = 'reference-bearing public result snapshots are unsupported'
    collect = 'explicit or unknown collection is forbidden transitively under no_gc'
    prefix = len('# λ🙂 UTF-8 byte spans\n'.encode())
    return [
        ('ref-output-reject', HERE / 'ref-output-reject.cg', [spec('CGT090', 'ref-output-reject.cg', 0, 72, ref)]),
        ('ref-output-utf8', FIXTURES / 'ref-output-utf8.cg', [spec('CGT090', 'ref-output-utf8.cg', 0, prefix + 72, ref)]),
        ('collect-guard-reject', HERE / 'collect-guard-reject.cg', [spec('CGT090', 'collect-guard-reject.cg', 168, 176, collect)]),
        ('dishonest-package', HERE / 'dishonest-package', [
            spec('CGT012', 'src/lib.cg', 68, 85, 'effect `Collect` is not permitted by explicit declaration by call to `managed.collect`'),
            spec('CGT012', 'src/main.cg', 124, 134, 'effect `Unknown` is not permitted by explicit declaration by call to `lib.heap`'),
            spec('CGT090', 'src/main.cg', 124, 134, collect)])]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary-dir', type=Path, required=True)
    parser.add_argument('--output-dir', type=Path, required=True, help='New attempt directory; existing attempts are immutable')
    parser.add_argument('--suite', default='budgets,faults,rejections,runtime-errors')
    args = parser.parse_args(argv)
    suites = args.suite.split(',')
    if any(s not in ('budgets', 'faults', 'rejections', 'runtime-errors') for s in suites):
        parser.error('unknown suite')
    runner = Runner(args.binary_dir, args.output_dir)
    runner.prepare()
    for suite in suites:
        getattr(runner, suite.replace('-', '_'))()
    runner.save()
    print(json.dumps({k: runner.report[k] for k in ('passed', 'failed', 'checkpoint_complete', 'release_complete')}))
    return int(runner.report['failed'] > 0)


if __name__ == '__main__':
    raise SystemExit(main())
