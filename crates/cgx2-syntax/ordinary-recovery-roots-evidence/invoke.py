#!/usr/bin/env python3
"""Persist a bounded command before running it; never overwrite an attempt."""
import sys
from pathlib import Path
from common import Journal, normal

j = Journal(Path(sys.argv[1]))
try:
    row = j.run(sys.argv[2], sys.argv[3:], suite=True)
    j.check(sys.argv[2], normal(row))
except Exception:
    j.exception()
raise SystemExit(j.finish())
