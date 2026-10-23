#!/usr/bin/env python3
"""Additional diagnostic run; excluded from latency samples."""
import json
import os
from pathlib import Path
import subprocess
import time

from measure import OUT, PACKAGE, digest


def observe(binary, label):
    env = dict(os.environ, LD_LIBRARY_PATH=str(PACKAGE), OMP_NUM_THREADS='1',
               MKL_NUM_THREADS='1', OPENBLAS_NUM_THREADS='1')
    for key in ('LD_PRELOAD', 'LD_AUDIT', 'CGX2_LOADER_TRACE'):
        env.pop(key, None)
    process = subprocess.Popen([str(binary), 'run', str(PACKAGE)], env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    observed = {}
    start = time.monotonic()
    timed_out = False
    while process.poll() is None:
        if time.monotonic() - start > 120:
            process.kill()
            timed_out = True
            break
        pending = [process.pid]
        while pending:
            pid = pending.pop()
            try:
                maps = Path(f'/proc/{pid}/maps').read_text()
                record = observed.setdefault(str(pid), {'snapshots': 0, 'lines': set(),
                                                       'exe': os.readlink(f'/proc/{pid}/exe')})
                record['snapshots'] += 1
                record['lines'].update(maps.splitlines())
                pending.extend(map(int, Path(f'/proc/{pid}/task/{pid}/children').read_text().split()))
            except (FileNotFoundError, ProcessLookupError):
                pass
        time.sleep(0.005)
    stdout, stderr = process.communicate(timeout=5)
    for record in observed.values():
        record['lines'] = sorted(record['lines'])
        record['binary_sha256'] = digest(Path(record['exe']))
    result = {'argv': [str(binary), 'run', str(PACKAGE)], 'timeout': timed_out,
              'exit_code': process.returncode, 'stdout': stdout.decode(),
              'stderr': stderr.decode(), 'processes': observed,
              'limitation': '5 ms polling can miss transient mappings; full raw union retained'}
    (OUT / f'{label}-maps.json').write_text(json.dumps(result, indent=2) + '\n')
    assert process.returncode == 0 and not timed_out
    assert len(observed) >= 2
    assert all('libpython' not in '\n'.join(r['lines']).lower() for r in observed.values())
    assert any('libtorch_cpu.so' in '\n'.join(r['lines']) for r in observed.values())


if __name__ == '__main__':
    import sys
    observe(Path(sys.argv[1]).resolve(), sys.argv[2])
