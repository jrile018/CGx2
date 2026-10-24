"""Bounded, incremental evidence; no Cargo invocation in product verification."""
import hashlib
import json
import os
from pathlib import Path
import resource
import sys
import traceback

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / 'owned-result-runner-evidence'))
from supervisor import supervise, evidence


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def normal(row, expected=0):
    return row.get('reason') == 'exit' and row.get('exit') == expected and row.get('signal') is None


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
                        CARGO_TARGET_DIR=str(ROOT / 'target/release-ordinary-recovery-roots'),
                        TMPDIR=str(ROOT / 'target/tmp-ordinary-recovery-roots'),
                        LIBRARY_PATH='/tmp/cgx2-wave22-lib', PYTHONDONTWRITEBYTECODE='1',
                        CGX2_MEMORY_EVIDENCE='1')
        self.report = {'schema_version': 1, 'reviewed_base': '89464a76c9bd84a3b090ec58a14ca6be0043c6ea',
                       'commands': [], 'checks': [], 'exceptions': [], 'complete': False,
                       'ordinary_memory_accepted': False, 'environment': {k: self.env[k] for k in
                       ('RUSTUP_TOOLCHAIN', 'CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'TMPDIR', 'LIBRARY_PATH')}}
        self.save()

    def save(self):
        self.report['passed'] = sum(r['passed'] for r in self.report['checks'])
        self.report['failed'] = sum(not r['passed'] for r in self.report['checks'])
        temporary = self.output / 'report.pending'
        temporary.write_text(json.dumps(self.report, indent=2) + '\n')
        temporary.replace(self.output / 'report.json')

    def run(self, name, argv, *, cwd=ROOT, extra=None, suite=False):
        command = [str(s) for s in argv]
        # Persist invocation before spawning: quota/spawn exceptions remain evidence.
        row = {'name': name, 'argv': command, 'cwd': str(cwd), 'extra_env': extra or {}, 'reason': 'pending'}
        self.report['commands'].append(row)
        self.save()
        try:
            result = supervise(command, cwd=str(cwd), env=dict(self.env, **(extra or {})),
                               limits=suite_limits if suite else process_limits,
                               cap=2 * 1024**2, timeout=3600 if suite else 180)
            row.update(evidence(result))
        except Exception:
            row.update(reason='exception', exit=None, signal=None, traceback=traceback.format_exc())
            self.save()
            raise
        self.save()
        print(json.dumps({'command': name, 'reason': row['reason'], 'exit': row.get('exit')}), flush=True)
        return row

    def check(self, name, condition, **details):
        self.report['checks'].append(dict(name=name, passed=bool(condition), **details))
        self.save()

    def exception(self):
        self.report['exceptions'].append(traceback.format_exc())
        self.save()

    def finish(self):
        self.report['complete'] = True
        self.save()
        return int(bool(self.report['failed'] or self.report['exceptions']))
