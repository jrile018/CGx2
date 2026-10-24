# CGx2 portable finite values: copied immutable aggregates and checked operations.
import sys as _cgx2_sys
_cgx2_steps = 0
_cgx2_depth = 0
_cgx2_utf8_work = 0

def _cgx2_fail(code, message, start=0, end=0):
    error = RuntimeError(f'{code} at {start}..{end}: {message}')
    error.code, error.span = code, (start, end)
    raise error

def _cgx2_step(start, end):
    global _cgx2_steps
    if _cgx2_steps >= 1000000:
        _cgx2_fail('CGR010', 'execution step limit of 1000000 exceeded', start, end)
    _cgx2_steps += 1

def _cgx2_expr(thunk, start, end):
    _cgx2_step(start, end)
    value = thunk()
    nodes, size = _cgx2_bound(value)
    global _cgx2_steps, _cgx2_utf8_work
    if _cgx2_steps + nodes > 1000000 or _cgx2_utf8_work + size > 268435456:
        _cgx2_fail('CGR010', 'execution work budget exceeded', start, end)
    _cgx2_steps += nodes
    _cgx2_utf8_work += size
    return value

def _cgx2_range(thunk, start, end):
    _cgx2_step(start, end)
    return thunk()

def _cgx2_enter():
    global _cgx2_depth
    if _cgx2_depth >= 64:
        _cgx2_fail('CGR011', 'function call depth limit of 64 exceeded')
    _cgx2_depth += 1

def _cgx2_leave():
    global _cgx2_depth
    _cgx2_depth -= 1

class _cgx2_List(tuple):
    pass

class _cgx2_Array(tuple):
    pass

class _cgx2_Variant:
    def __init__(self, ty, case, payloads):
        self.ty, self.case, self.payloads = ty, case, tuple(payloads)
    def __eq__(self, other):
        return isinstance(other, _cgx2_Variant) and (self.ty, self.case, self.payloads) == (other.ty, other.case, other.payloads)

class _cgx2_Record:
    def __init__(self, ty, fields):
        self.ty, self.fields = ty, tuple(fields)
    def __eq__(self, other):
        return isinstance(other, _cgx2_Record) and (self.ty, self.fields) == (other.ty, other.fields)

def _cgx2_record(ty, fields, supplied):
    values = dict(supplied)
    return _cgx2_Record(ty, [(f, values[f]) for f in fields])

def _cgx2_field(value, name):
    return dict(value.fields)[name]

def _cgx2_bound(value):
    pending, remaining, size = [(value, 0)], 65537, 0
    while pending:
        v, depth = pending.pop()
        remaining -= 1
        if remaining < 0 or depth >= 64:
            _cgx2_fail('CGR010', 'finite value exceeds size/depth budget')
        if isinstance(v, str):
            size += len(v.encode('utf-8'))
            if size > 1048576:
                _cgx2_fail('CGR010', 'string byte limit exceeds 1048576')
        if isinstance(v, (_cgx2_List, _cgx2_Array)):
            if len(v) > 65536:
                _cgx2_fail('CGR010', 'collection length exceeds 65536')
            pending.extend((x, depth + 1) for x in v)
        elif isinstance(v, _cgx2_Variant):
            pending.extend((x, depth + 1) for x in v.payloads)
        elif isinstance(v, _cgx2_Record):
            pending.extend((x, depth + 1) for _, x in v.fields)

    return 65537 - remaining, size

def _cgx2_show(value):
    if value is None:
        return ''
    if type(value) is bool:
        return 'true' if value else 'false'
    if isinstance(value, _cgx2_Variant):
        return value.ty + '.' + value.case + '(' + ', '.join(_cgx2_show(x) for x in value.payloads) + ')'
    if isinstance(value, _cgx2_Record):
        return value.ty + '(' + ', '.join(n + ': ' + _cgx2_show(v) for n, v in value.fields) + ')'
    if isinstance(value, (_cgx2_List, _cgx2_Array)):
        return ('Array' if isinstance(value, _cgx2_Array) else '') + '[' + ', '.join(_cgx2_show(x) for x in value) + ']'
    return str(value)

def _cgx2_output(value):
    _cgx2_bound(value)
    shown = _cgx2_show(value)
    if len(shown.encode('utf-8')) > 1048576:
        _cgx2_fail('CGR010', 'output limit exceeded')
    if value is not None:
        print(shown)

def _cgx2_math(op, a, b, start, end):
    if op == 'div':
        if b == 0:
            _cgx2_fail('CGR006', 'division by zero', start, end)
        value = abs(a) // abs(b)
        if (a < 0) != (b < 0):
            value = -value
    else:
        value = a + b if op == 'add' else a - b if op == 'sub' else a * b
    if not -9223372036854775808 <= value <= 9223372036854775807:
        _cgx2_fail('CGR005', 'i64 arithmetic overflow', start, end)
    return value

def _cgx2_compare(op, a, b):
    if op == 'eq': return a == b
    if op == 'ne': return a != b
    if op == 'lt': return a < b
    if op == 'le': return a <= b
    if op == 'gt': return a > b
    return a >= b

def _cgx2_get(xs, i, tag):
    if 0 <= i < len(xs):
        return _cgx2_Variant(tag, 'Some', [xs[i]])
    return _cgx2_Variant(tag, 'None', [])

def _cgx2_data(name, tag, args):
    first = args[0]
    if name == 'length': return len(first)
    if name == 'show': return _cgx2_show(first)
    if name == 'get': return _cgx2_get(first, args[1], tag)
    if name == 'updated':
        i, value = args[1:]
        if not 0 <= i < len(first): return _cgx2_Variant(tag, 'None', [])
        updated = type(first)(first[:i] + (value,) + first[i+1:])
        return _cgx2_Variant(tag, 'Some', [updated])
    if name == 'append':
        if len(first) >= 65536: _cgx2_fail('CGR010', 'collection length exceeds 65536')
        return _cgx2_List(first + (args[1],))
    if name == 'array': return _cgx2_Array(first)
    if name == 'list': return _cgx2_List(first)
    if name == 'concat':
        if len(first.encode('utf-8')) + len(args[1].encode('utf-8')) > 1048576:
            _cgx2_fail('CGR010', 'string byte limit exceeds 1048576')
        return first + args[1]
    if name == 'slice':
        a, b = args[1:]
        if not 0 <= a <= b <= len(first): return _cgx2_Variant(tag, 'None', [])
        return _cgx2_Variant(tag, 'Some', [first[a:b]])
    if name == 'parse_i64':
        digits = first[1:] if first.startswith(('-', '+')) else first
        valid = bool(digits) and all('0' <= c <= '9' for c in digits)
        # Bound conversion work before Python's integer parser sees long input.
        trimmed = digits.lstrip('0')
        if valid and len(trimmed) <= 19:
            value = int(('-' if first.startswith('-') else '') + (trimmed or '0'))
            if -9223372036854775808 <= value <= 9223372036854775807:
                return _cgx2_Variant(tag, 'Ok', [value])
        return _cgx2_Variant(tag, 'Err', ['invalid integer'])
    _cgx2_fail('CGR009', 'unknown portable operation')

def _cgx2_arg(index):
    if not 0 <= index < len(_cgx2_sys.argv) - 1:
        _cgx2_fail('CGR007', 'arg index out of range')
    return _cgx2_sys.argv[index + 1]

def _cgx2_contract(value, code, start, end):
    if value is not True:
        _cgx2_fail(code, 'contract condition failed', start, end)
def _cgx2_user_69643c4c6973743c6936343e3e(_cgx2_local_0):
    _cgx2_enter()
    try:
        _cgx2_step(79, 81)
        return _cgx2_expr(lambda: _cgx2_local_0, 79, 80)
    finally:
        _cgx2_leave()

def _cgx2_user_69643c636778322e636f72652e4f7074696f6e3c636778322e636f72652e526573756c743c4c6973743c41727261793c6936343e3e2c20426f6f6c3e3e3e(_cgx2_local_1):
    _cgx2_enter()
    try:
        _cgx2_step(79, 81)
        return _cgx2_expr(lambda: _cgx2_local_1, 79, 80)
    finally:
        _cgx2_leave()

def _cgx2_user_69643c6936343e(_cgx2_local_2):
    _cgx2_enter()
    try:
        _cgx2_step(79, 81)
        return _cgx2_expr(lambda: _cgx2_local_2, 79, 80)
    finally:
        _cgx2_leave()

def main():
    _cgx2_enter()
    try:
        _cgx2_step(197, 266)
        _cgx2_local_3 = _cgx2_expr(lambda: _cgx2_Variant("cgx2.core.Option<cgx2.core.Result<List<Array<i64>>, Bool>>", "None", []), 252, 265)
        _cgx2_step(270, 297)
        _cgx2_local_4 = _cgx2_expr(lambda: _cgx2_user_69643c636778322e636f72652e4f7074696f6e3c636778322e636f72652e526573756c743c4c6973743c41727261793c6936343e3e2c20426f6f6c3e3e3e(_cgx2_expr(lambda: _cgx2_local_3, 287, 295)), 284, 296)
        _cgx2_step(301, 334)
        _cgx2_local_5 = _cgx2_expr(lambda: _cgx2_user_6e65737465643c6936343e(_cgx2_expr(lambda: _cgx2_List([_cgx2_expr(lambda: 4, 327, 328), _cgx2_expr(lambda: 5, 330, 331)]), 326, 332)), 314, 333)
        _cgx2_step(338, 349)
        return _cgx2_expr(lambda: _cgx2_user_69643c6936343e(_cgx2_expr(lambda: 7, 346, 347)), 338, 348)
    finally:
        _cgx2_leave()

def _cgx2_user_6e65737465643c6936343e(_cgx2_local_6):
    _cgx2_enter()
    try:
        _cgx2_step(155, 170)
        return _cgx2_expr(lambda: _cgx2_user_69643c4c6973743c6936343e3e(_cgx2_expr(lambda: _cgx2_local_6, 167, 168)), 155, 169)
    finally:
        _cgx2_leave()

if __name__ == '__main__':
    try:
        _cgx2_output(main())
    except Exception as _cgx2_error:
        _cgx2_sys.stderr.write(str(_cgx2_error) + '\n')
        raise SystemExit(1)
