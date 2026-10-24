#!/usr/bin/env python3
"""Replay unchanged Result checks except the explicitly superseded shadow case.

The original runner is also executed unchanged by verify.py. This adapter does
not change historical files or count the obsolete rejection as a pass.
"""
import hashlib
from pathlib import Path

original = Path(__file__).resolve().parents[1] / 'owned-result-evidence/run.py'
source = original.read_text()
old = '            for name, expected in json.loads((HERE / "expected.json").read_text()).items():\n'
assert source.count(old) == 1
new = old + '                if name == "shadow-builtin":\n                    continue  # explicitly superseded schema-1 parameter rejection\n'
print('Original runner sha256:', hashlib.sha256(original.read_bytes()).hexdigest(), flush=True)
print('Explicit exclusion: shadow-builtin (9 historical rejection checks); required parameter compatibility is checked in Rust and product fixtures.', flush=True)
exec(compile(source.replace(old, new), str(original), 'exec'), {'__file__': str(original), '__name__': '__main__'})
