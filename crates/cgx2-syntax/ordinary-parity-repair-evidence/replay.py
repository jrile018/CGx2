#!/usr/bin/env python3
"""Run the unchanged 117 checks and 18 regressions, retaining historical reconciliation."""
import argparse
import json
from pathlib import Path
import sys
from common import HERE, Journal, normal, digest

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary-dir', type=Path, required=True)
p.add_argument('--output-dir', type=Path, required=True)
a=p.parse_args()
j=Journal(a.output_dir)
try:
    runner=HERE.parent/'ordinary-memory-evidence/run.py'
    j.report['historical_runner_sha256']=digest(runner)
    row=j.run('unchanged-ordinary-memory-117-and-18-regressions',
              [sys.executable,runner,'--binary-dir',a.binary_dir.resolve(),'--output-dir',j.output/'product','--regressions'],suite=True)
    j.check('historical-process-exit',normal(row))
    product=j.output/'product/product.json'
    if product.exists():
        data=json.loads(product.read_text())
        j.report['product_report_sha256']=digest(product)
        j.report['product_summary']={k:data[k] for k in ('passed','failed')}
        regression=[r for r in data['commands'] if r['name'].startswith('regression-')]
        j.check('exact-regression-count',len(regression)==18,count=len(regression))
        j.check('unchanged-117-plus19-checks',len(data['checks'])==136,count=len(data['checks']))
        j.check('frozen-history-unchanged',data['frozen_before']==data['frozen_after'])
    else:
        j.check('historical-report-present',False)
except Exception:
    j.exception()
raise SystemExit(j.finish())
