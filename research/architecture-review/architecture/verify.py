#!/usr/bin/env python3
"""Offline provenance/schema checks; no implementation-safety inference."""
import hashlib
import json
from pathlib import Path

LANE = Path(__file__).resolve().parent
ROOT = LANE.parents[2]
MANIFEST = ROOT / 'research/architecture-review/input-manifest.json'
STATUSES = {'intended', 'implemented', 'implemented_unaccepted', 'historically_tested',
            'model_proved', 'implementation_connected', 'blocked'}
REQUIRED = {'id', 'name', 'status', 'scope', 'source_refs', 'assumptions', 'obligations', 'evidence'}

def digest(data):
    return hashlib.sha256(data).hexdigest()

def require(condition, message):
    if not condition:
        raise ValueError(message)

def main():
    manifest = json.loads(MANIFEST.read_text())
    ledger = json.loads((LANE / 'claims.json').read_text())
    require(ledger['manifest_sha256'] == digest(MANIFEST.read_bytes()), 'manifest identity changed')
    indices = {}
    files = {}
    for variant, key, base in [
        ('baseline', 'baseline_files', ROOT),
        ('wip-overlay', 'wip_files', Path(manifest['wip_snapshot_root'])),
    ]:
        indices[variant] = {entry['path']: entry for entry in manifest[key]}
        require(len(indices[variant]) == len(manifest[key]), f'duplicate {variant} manifest paths')
        for path, entry in indices[variant].items():
            require(not Path(path).is_absolute() and '..' not in Path(path).parts, f'unsafe input path: {path}')
            data = (base / path).read_bytes()
            require(digest(data) == entry['sha256'], f'input hash mismatch: {variant}:{path}')
            require(len(data) == entry['bytes'], f'input length mismatch: {variant}:{path}')
            files[(variant, path)] = data.decode('utf-8').splitlines()
    claims = ledger['claims']
    require(len(claims) >= 10, 'fewer than ten claims')
    seen = set()
    anchors = 0
    for claim in claims:
        require(REQUIRED <= claim.keys(), f'missing claim fields: {claim.get("id")}')
        require(claim['id'] not in seen, 'duplicate claim id')
        seen.add(claim['id'])
        require(claim['status'] in STATUSES, f'invalid status: {claim["id"]}')
        for key in ['id', 'name', 'scope']:
            require(isinstance(claim[key], str) and claim[key].strip(), f'empty {key}')
        for key in ['source_refs', 'assumptions', 'obligations', 'evidence']:
            require(isinstance(claim[key], list) and claim[key], f'empty {key}: {claim["id"]}')
        for ref in claim['source_refs']:
            variant, path = ref['variant'], ref['path']
            require(variant in indices and path in indices[variant], f'unmanifested source: {ref}')
            require(ref['sha256'] == indices[variant][path]['sha256'], f'source hash reference: {ref}')
            lines = files[(variant, path)]
            start, end = ref['line_start'], ref['line_end']
            require(type(start) is int and type(end) is int and 1 <= start <= end <= len(lines),
                    f'invalid line range: {ref}')
            anchor = ('\n'.join(lines[start-1:end]) + '\n').encode()
            require(ref['anchor_sha256'] == digest(anchor), f'anchor fingerprint mismatch: {ref}')
            anchors += 1
    artifact_files = sorted(path for path in LANE.iterdir() if path.is_file())
    for path in artifact_files:
        for number, line in enumerate(path.read_text().splitlines(), 1):
            require(line.rstrip() == line, f'trailing whitespace: {path.name}:{number}')
    print(json.dumps({'status': 'pass', 'baseline_files': len(indices['baseline']),
                      'wip_files': len(indices['wip-overlay']), 'claims': len(claims),
                      'source_anchors': anchors, 'artifact_whitespace': 'pass',
                      'scope': 'offline input identity, claim schema, exact source anchors and artifact whitespace only',
                      'not_established': ['architecture safety', 'compiler semantic preservation',
                                          'fresh Rust/Lean acceptance', 'performance or language superiority']}, indent=2))

if __name__ == '__main__':
    main()
