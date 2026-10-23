#!/usr/bin/env python3
"""Derive medians, ranges, counts and provenance checks from retained samples."""
import collections
import json
import statistics
from pathlib import Path

OUT = Path(__file__).resolve().parent


def stats(values):
    return {'n': len(values), 'median': statistics.median(values),
            'min': min(values), 'max': max(values)}


summary = {}
stdout = None
package_hashes = None
for label in ('frozen-base', 'before-trace', 'final-trace', 'final-untraced'):
    data = json.loads((OUT / f'{label}.json').read_text())
    assert len(data['runs']) == 5
    result = {'wall_ns': stats([r['wall_ns'] for r in data['runs']]),
              'binary_sha256': data['binary_sha256']}
    if package_hashes is None:
        package_hashes = data['hashes']
    assert data['hashes'] == package_hashes
    phases = collections.defaultdict(list)
    for run in data['runs']:
        assert run['numeric_ok'] and run['exit_code'] == 0 and not run['timeout']
        if stdout is None:
            stdout = run['stdout']
        assert run['stdout'] == stdout
        events = collections.defaultdict(lambda: {'count': 0, 'ns': 0, 'bytes': 0})
        for line in run['stderr'].splitlines():
            assert line.startswith('cgx2-loader ')
            event = dict(part.split('=', 1) for part in line.split()[1:])
            total = events[event['phase']]
            total['count'] += 1
            total['ns'] += int(event['ns'])
            total['bytes'] += int(event['bytes'])
        for phase, total in events.items():
            phases[phase].append(total)
    result['phases'] = {phase: {key: stats([t[key] for t in totals])
                                for key in ('count', 'ns', 'bytes')}
                        for phase, totals in phases.items()}
    summary[label] = result
buffers = collections.defaultdict(list)
hashes = set()
for line in (OUT / 'hash-buffers.jsonl').read_text().splitlines():
    row = json.loads(line)
    buffers[row['buffer_bytes']].append(row['ns'])
    hashes.add(row['sha256'])
assert len(hashes) == 1
summary['isolated_buffer_ns'] = {size: stats(values) for size, values in buffers.items()}
summary['numeric_stdout_identical'] = stdout
summary['package_hashes_identical'] = True
summary['scope'] = 'inclusive nested timers; no sums of inclusive phases or p95'
(OUT / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
