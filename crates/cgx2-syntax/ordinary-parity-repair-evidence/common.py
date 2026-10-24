"""Bounded, incremental evidence; no Cargo invocation in product verification."""
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import sys
import traceback

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / 'owned-result-runner-evidence'))
from supervisor import supervise, evidence


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def normal(row, expected=0):
    return (row.get('reason') == 'exit' and type(row.get('exit')) is int
            and row['exit'] == expected and row.get('signal') is None)


def observed(row):
    lines = [s.removeprefix('CGX2_MEMORY_EVIDENCE ') for s in row.get('stderr', '').splitlines()
             if s.startswith('CGX2_MEMORY_EVIDENCE ')]
    try:
        return json.loads(lines[0]) if len(lines) == 1 else None
    except ValueError:
        return None


def stable(value):
    # Pause histograms are clock measurements. Session IDs distinguish invocations,
    # not semantics. All charges, roots, failures, collection counters stay checked.
    if isinstance(value, dict):
        return {k: ({part:stable(n) for part,n in v.items() if part != 'session'}
                    if k == 'region_id' and isinstance(v, dict) else stable(v))
                for k, v in value.items() if k != 'pause'}
    if isinstance(value, list):
        return [stable(v) for v in value]
    return value


def rejection(row, code, span, file=None):
    if not normal(row, 1):
        return False
    try:
        data = json.loads(row.get('stdout') or row.get('stderr', ''))
        rows = data if isinstance(data, list) else data.get('diagnostics', [data] if 'code' in data else [])
        return (len(rows) == 1 and rows[0].get('code') == code and rows[0].get('span') == span
                and (file is None or rows[0].get('file') == file))
    except (ValueError, TypeError, AttributeError):
        return False


def process_limits():
    resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3,) * 2)
    resource.setrlimit(resource.RLIMIT_CPU, (120,) * 2)
    resource.setrlimit(resource.RLIMIT_FSIZE, (512 * 1024**2,) * 2)


def suite_limits():
    resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3,) * 2)
    resource.setrlimit(resource.RLIMIT_CPU, (1800,) * 2)
    resource.setrlimit(resource.RLIMIT_FSIZE, (512 * 1024**2,) * 2)


class Journal:
    def __init__(self, output):
        self.output = Path(output).resolve()
        self.output.mkdir(parents=True, exist_ok=False)
        self.env = dict(os.environ, RUSTUP_TOOLCHAIN='1.98.1', CARGO_BUILD_JOBS='2',
                        CARGO_TARGET_DIR=os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target/release-ordinary-harness-hardening')),
                        TMPDIR=os.environ.get('TMPDIR', str(ROOT / 'target/tmp-ordinary-harness-hardening')),
                        LIBRARY_PATH='/tmp/cgx2-wave22-lib', PYTHONDONTWRITEBYTECODE='1',
                        CGX2_MEMORY_EVIDENCE='1')
        self.required_commands = set()
        self.required_checks = set()
        self.report = {'schema_version': 2, 'reviewed_base': '89464a76c9bd84a3b090ec58a14ca6be0043c6ea',
                       'commands': [], 'checks': [], 'exceptions': [], 'complete': False,
                       'ordinary_memory_accepted': False, 'environment': {k: self.env[k] for k in
                       ('RUSTUP_TOOLCHAIN', 'CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'TMPDIR', 'LIBRARY_PATH')}}
        self.save()

    def save(self):
        self.report['passed'] = sum(r.get('passed') is True for r in self.report['checks'])
        self.report['failed'] = sum(r.get('passed') is not True for r in self.report['checks'])
        temporary = self.output / 'report.pending'
        temporary.write_text(json.dumps(self.report, indent=2) + '\n')
        temporary.replace(self.output / 'report.json')

    def run(self, name, argv, *, cwd=ROOT, extra=None, suite=False):
        if any(row['name'] == name for row in self.report['commands']):
            raise ValueError('duplicate command identity: ' + name)
        command = [str(s) for s in argv]
        # Persist invocation before spawning: quota/spawn exceptions remain evidence.
        row = {'name': name, 'argv': command, 'cwd': str(cwd), 'extra_env': extra or {}, 'reason': 'pending'}
        executable = shutil.which(command[0], path=self.env.get('PATH'))
        if executable and Path(executable).is_file():
            row['executable_input'] = {'path': executable, 'resolved': str(Path(executable).resolve()), 'sha256': digest(executable)}
        self.report['commands'].append(row)
        self.save()
        try:
            result = supervise(command, cwd=str(cwd), env=dict(self.env, **(extra or {})),
                               limits=suite_limits if suite else process_limits,
                               cap=2 * 1024**2, timeout=3600 if suite else 180)
            row.update(evidence(result))
            if 'executable_input' in row:
                try:
                    row['executable_unchanged'] = digest(executable) == row['executable_input']['sha256'] and str(Path(executable).resolve()) == row['executable_input']['resolved']
                except OSError:
                    row['executable_unchanged'] = False
        except Exception:
            row.update(reason='exception', exit=None, signal=None, traceback=traceback.format_exc())
            self.save()
            raise
        self.save()
        print(json.dumps({'command': name, 'reason': row['reason'], 'exit': row.get('exit')}), flush=True)
        return row

    def check(self, name, condition, **details):
        if any(row['name'] == name for row in self.report['checks']):
            raise ValueError('duplicate check identity: ' + name)
        self.report['checks'].append(dict(name=name, passed=bool(condition), **details))
        self.save()

    def exception(self):
        self.report['exceptions'].append(traceback.format_exc())
        self.save()

    def finish(self):
        from validation import inventory_errors
        errors = inventory_errors(self.report, self.required_commands, self.required_checks)
        self.report['required_inventory'] = {'commands': sorted(self.required_commands), 'checks': sorted(self.required_checks)}
        self.report['coverage_errors'] = errors
        self.check('required-evidence-coverage', not errors, errors=errors)
        self.report['complete'] = not (errors or self.report['exceptions'] or self.report.get('integrity_failed')
                                       or self.report.get('incomplete_reason'))
        self.save()
        return int(bool(self.report['failed'] or not self.report['complete']))
