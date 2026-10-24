"""Fail-closed production validators and explicit evidence inventories."""
from collections import Counter
import json
from pathlib import Path
import re

ROOT_FIELDS = ('baseline_roots', 'final_roots', 'roots_registered', 'roots_released', 'cleanup_failures')
SOURCE_CASES = ('normal', 'return', 'allocation', 'region', 'guard', 'nested-guards',
                'copy-cycle', 'session', 'arithmetic', 'contract', 'failed-argument')
MODES = ('debug', 'release', 'standalone')


def roots_ok(value):
    return (isinstance(value, dict)
            and all(type(value.get(k)) is int and value[k] >= 0 for k in ROOT_FIELDS)
            and value['baseline_roots'] == value['final_roots'] == 0
            and value['roots_registered'] == value['roots_released']
            and value['cleanup_failures'] == 0)


def session_unavailable(value):
    return (type(value) is dict and set(value) == {'session_entered', 'metrics'}
            and value['session_entered'] is False and value['metrics'] is None)


def direct_inventory(bound):
    if type(bound) is not int or not 1 <= bound <= 64:
        raise ValueError('direct reserve bound must be an integer in 1..64')
    names = {f'{kind}-{edge}' for kind in ('allocation', 'region-limit', 'region-bookkeeping',
             'root-bookkeeping', 'guard-bookkeeping', 'copy-budget') for edge in ('zero', 'exact', 'one-under')}
    names.update(f'library-cleanup-{case}' for case in
                 ('normal', 'return', 'failed-argument', 'contract', 'arithmetic', 'allocation', 'cancellation'))
    names.update(('nested-multiple-regions-cumulative-nonrefund', 'manual-guard-exit-never-collects',
                  'library-cooperative-cancellation-drop', 'library-panic-unwind', 'OM06-safe-same-region-copy',
                  'session-entry-reserve-1', 'sweep-bound-at-most-64', 'reserve-18446744073709551615'))
    names.update(f'reserve-{i}' for i in range(1, bound + 2))
    return names


def validate_direct(stdout):
    rows, errors = [], []
    for index, line in enumerate(stdout.splitlines(), 1):
        try:
            row = json.loads(line)
        except (ValueError, TypeError):
            errors.append(f'line {index}: malformed JSON')
            continue
        if (type(row) is not dict or set(row) != {'name', 'passed', 'details'}
                or type(row.get('name')) is not str or type(row.get('passed')) is not bool
                or type(row.get('details')) is not str):
            errors.append(f'line {index}: invalid direct row structure/types')
            continue
        rows.append(row)
    counts = Counter(r['name'] for r in rows)
    errors += [f'duplicate direct row: {n}' for n, count in counts.items() if count != 1]
    bound_rows = [r for r in rows if r['name'] == 'sweep-bound-at-most-64']
    baseline = [r for r in rows if r['name'] == 'reserve-18446744073709551615']
    bound = None
    if len(bound_rows) == len(baseline) == 1:
        value = bound_rows[0]['details']
        match = re.match(r'^\((\d+), None, Ok\(\(\)\),', baseline[0]['details'])
        if re.fullmatch(r'[1-9]\d*', value) and 1 <= int(value) <= 64 and match and int(match[1]) == int(value):
            bound = int(value)
    if bound is None:
        errors.append('missing/malformed/inconsistent baseline reserve-attempt bound')
    else:
        required = direct_inventory(bound)
        errors += [f'missing direct row: {n}' for n in sorted(required - counts.keys())]
        errors += [f'unknown direct row: {n}' for n in sorted(counts.keys() - required)]
    return rows, errors, bound


def sentinel_status(row, path, expected, code):
    from common import normal
    # cgx2c build/emit use one complete human diagnostic on stderr, not the
    # JSON format of a separate cgx2 check command. Consume the entire result.
    if type(row) is not dict:
        row = {}
    stderr, stdout = row.get('stderr'), row.get('stdout', '')
    match = (re.fullmatch(r'(CG[A-Z]+[0-9]{3}): [^\r\n]+\n?', stderr)
             if type(stderr) is str else None)
    rejected = (normal(row, 1) and type(stdout) is str and stdout == ''
                and match is not None and match[1] == code
                and not command_errors(row))
    try:
        preserved = Path(path).read_bytes() == expected
    except OSError:
        preserved = False
    return rejected, preserved


def stream_errors(row):
    """Validate retained bytes when supervision metadata is present."""
    import base64
    import hashlib
    errors = []
    for name in ('stdout', 'stderr'):
        if name not in row:
            if 'argv' in row:
                errors.append('missing ' + name)
            continue
        if type(row[name]) is not str:
            errors.append('invalid ' + name)
            continue
        data = row[name].encode('utf-8')
        if name + '_base64' in row:
            try:
                data = base64.b64decode(row[name + '_base64'], validate=True)
                if data.decode('utf-8', errors='replace') != row[name]:
                    errors.append('inconsistent ' + name)
                # Diagnostic parsing cannot accept a lossy UTF-8 observation.
                data.decode('utf-8')
            except (ValueError, TypeError, UnicodeError):
                errors.append('invalid UTF-8/base64 ' + name)
        if 'argv' in row or name + '_bytes' in row or name + '_sha256' in row:
            if type(row.get(name + '_bytes')) is not int or row[name + '_bytes'] != len(data):
                errors.append('invalid byte count ' + name)
            if row.get(name + '_sha256') != hashlib.sha256(data).hexdigest():
                errors.append('invalid hash ' + name)
    return errors


def command_errors(row, strict=False):
    errors = []
    if strict:
        required = {'name', 'argv', 'cwd', 'reason', 'exit', 'signal', 'stdout', 'stderr',
                    'stdout_bytes', 'stderr_bytes', 'stdout_sha256', 'stderr_sha256',
                    'executable_input', 'executable_unchanged'}
        if not required <= row.keys():
            errors.append('missing completed command fields: ' + repr(sorted(required - row.keys())))
        executable = row.get('executable_input')
        if (type(executable) is not dict or type(executable.get('path')) is not str
                or type(executable.get('resolved')) is not str
                or not re.fullmatch('[0-9a-f]{64}', str(executable.get('sha256', '')))
                or row.get('executable_unchanged') is not True):
            errors.append('missing/invalid executable observation')
    if (row.get('reason') != 'exit' or type(row.get('exit')) is not int
            or not 0 <= row['exit'] <= 255 or 'signal' not in row or row['signal'] is not None
            or row.get('overflow_stream') is not None or row.get('error') is not None
            or row.get('traceback') is not None):
        errors.append('abnormal/incomplete termination')
    if 'argv' in row:
        if (type(row['argv']) is not list or not row['argv']
                or any(type(s) is not str or not s for s in row['argv'])
                or type(row.get('cwd')) is not str or not Path(row['cwd']).is_absolute()):
            errors.append('invalid invocation')
    errors.extend(stream_errors(row))
    if row.get('executable_unchanged') is False:
        errors.append('changed executable')
    return errors


def inventory_errors(report, commands, checks):
    errors = []
    for key, required in (('commands', commands), ('checks', checks)):
        rows = report.get(key, [])
        if type(rows) is not list:
            errors.append('invalid ' + key); continue
        if any(type(r) is not dict or type(r.get('name')) is not str or not r['name'] for r in rows):
            errors.append('invalid ' + key + ' row/name')
            rows = [r for r in rows if type(r) is dict and type(r.get('name')) is str and r['name']]
        counts = Counter(r.get('name') for r in rows)
        errors += [f'duplicate {key}: {n}' for n, count in counts.items() if count != 1]
        errors += [f'missing {key}: {n}' for n in sorted(set(required) - counts.keys())]
        for row in rows:
            if key == 'checks' and type(row.get('passed')) is not bool:
                errors.append(f'invalid passed type: {row.get("name")}')
            if key == 'commands':
                errors += [f'{row["name"]}: {error}' for error in command_errors(row, report.get('command_record_schema') == 1)]
                if row['name'] in {'rustc-identity', 'rustc-sysroot', 'toolchain-identity',
                                    'toolchain-sysroot', 'cargo-identity'} and row.get('exit') != 0:
                    errors.append('failed required toolchain command: ' + row['name'])
    return errors


def matrix_inventory(bound):
    commands = {'standalone-outside-repository', 'rustc-identity', 'rustc-sysroot',
                'runtime_probe-build', 'direct-runtime-public-controls', 'source_probe-build', 'sweep-source-admission'}
    checks = {'frozen-manifest', 'standalone-outside-repository', 'runtime_probe-build', 'direct-runtime-process',
              'direct-runtime-inventory', 'source_probe-build', 'sweep-source-admission', 'source-sweep-has-unaffected-tail'}
    def result(tag, unavailable=False, parity=False, actual=False):
        checks.add(tag + '-result')
        checks.add(tag + ('-pre-session-unavailable' if unavailable else '-balanced-roots'))
        if not unavailable:
            checks.add(tag + '-manual-zero-collections')
        if parity:
            checks.add(tag + '-deterministic-parity')
        if actual:
            checks.add(tag + '-actual-error-parity')
    def targets(name, fault=False, index=None):
        for mode in MODES:
            tag = f'{name}-{mode}'
            commands.update((tag + '-build', tag + '-run'))
            checks.add(tag + '-build')
            if mode == 'standalone':
                commands.add(tag + '-emit'); checks.add(tag + '-emit')
            result(tag, unavailable=fault and index == 1, parity=True, actual=fault)
    for name in SOURCE_CASES:
        commands.update((name + '-check', name + '-interpreter')); checks.add(name + '-check')
        result(name + '-interpreter'); targets(name)
    for name in ('cross-region', 'cross-region-utf8', 'fallback-resource', 'dependency'):
        commands.add(name + '-negative'); checks.add(name + '-negative-exact-code-span' if name == 'dependency' else name + '-exact-code-span')
        if name == 'dependency':
            checks.add('dependency-negative-exact-file')
        for operation in ('build', 'emit-rust', 'emit-python'):
            commands.add(name + '-' + operation)
            checks.update((name + '-' + operation + '-rejected', name + '-' + operation + '-preserved'))
    for index in range(1, 65):
        tag = f'source-reserve-{index}'
        commands.add(tag); checks.add(tag + '-fixed-outcome'); result(tag, unavailable=index == 1)
    if type(bound) is int and 1 <= bound < 64:
        for index in range(1, bound + 2):
            targets(f'source-fault-{index}', fault=True, index=index)
    else:
        checks.add('valid-source-reserve-bound')
    return commands, checks


def auto_inventory():
    commands = {'outside', 'direct-build', 'direct-runtime-phases', 'source-check', 'interpreter'}
    checks = {'outside', 'direct-build', 'direct-runtime-phases', 'source-check'}
    for mode in ('interpreter',) + MODES:
        checks.update(f'{mode}-{part}' for part in ('value', 'roots', 'collections', 'guards', 'parity'))
        if mode != 'interpreter':
            commands.update((mode + '-build', mode + '-run')); checks.add(mode + '-build')
    commands.add('standalone-emit'); checks.add('standalone-emit')
    return commands, checks


def package_inventory():
    commands = {'outside-repository'}; checks = {'outside-repository'}
    for name in ('package-value', 'package-error'):
        commands.update((name + '-check', name + '-interpreter', name + '-python'))
        checks.update((name + '-check', name + '-python-rejection', name + '-python-preservation'))
        for mode in ('interpreter',) + MODES:
            tag = name + '-' + mode
            checks.update(tag + '-' + part for part in ('result', 'cleanup', 'deterministic-memory'))
            if mode != 'interpreter':
                commands.update((tag + '-build', tag + '-run')); checks.add(tag + '-build')
        commands.add(name + '-standalone-emit'); checks.add(name + '-standalone-emit')
    return commands, checks
