import sys

value = 9223372036854775807 + 1
if value > 9223372036854775807:
    print("PILOT_I64_OVERFLOW", file=sys.stderr)
    raise SystemExit(1)
print(value)
