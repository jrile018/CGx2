#!/usr/bin/env python3
"""Compact only this checkpoint's completed generated evidence; preserve hashes."""
from pathlib import Path
import gzip
import hashlib
import json

BASE = Path(__file__).resolve().parent / 'evidence'
manifest = BASE / 'artifact-inventory.json'
rows = json.loads(manifest.read_text()) if manifest.exists() else []
for path in sorted(BASE.rglob('*')):
    if not path.is_file() or path == manifest:
        continue
    with path.open('rb') as stream:
        magic = stream.read(8)
    binary = magic.startswith(b'\x7fELF') or magic == b'!<arch>\n'
    compress = path.suffix in ('.stdout', '.stderr') or (path.suffix == '.rs' and path.stat().st_size > 65536)
    if not binary and not compress:
        continue
    data = path.read_bytes()
    row = {'path': str(path.relative_to(BASE)), 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    if compress:
        dest = path.with_name(path.name + '.gz')
        dest.write_bytes(gzip.compress(data, mtime=0))
        row['retained'] = str(dest.relative_to(BASE))
    else:
        row['retained'] = 'hash-only; reproducible generated ELF/archive'
    rows.append(row)
    path.unlink()
manifest.write_text(json.dumps(rows, indent=2) + '\n')
