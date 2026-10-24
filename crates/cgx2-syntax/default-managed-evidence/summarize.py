#!/usr/bin/env python3
"""Select final bounded checks without hiding failed/blocked historical attempts."""
from pathlib import Path
import hashlib
import json
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
selected = ['quality-complete', 'workspace-complete', 'regressions-complete',
            'product-acceptance-complete', 'asan-final', 'asan-leaks-final',
            'asan-roots-final2', 'asan-roots-leaks-final2']
rows = []
for name in selected:
    p = HERE / 'evidence' / name / 'verification.json'
    a = json.loads(p.read_text())
    for check in a['checks']:
        rows.append({'evidence': str(p.relative_to(HERE)), **check})
expected = {'build','workspace-tests','clippy','fmt','diff','managed-product',
            'owned-symbol195','owned-result-original','owned-result-reconciled',
            'ordinary256','foundation','semantic','specialization','visibility',
            'binders','output','emitters','graph99','guards518','reports265',
            'asan-no-leaks','asan-leaks','asan-roots-no-leaks','asan-roots-leaks'}
assert {r['name'] for r in rows} == expected
assert all(r['status'] in ('passed','blocked','known-superseded-negative') for r in rows)
assert all(r['status'] != 'blocked' or r['name'] in ('asan-leaks','asan-roots-leaks') for r in rows)
product = json.loads((HERE/'evidence/product-acceptance-complete/managed-product/product.json').read_text())
assert product['failed'] == 0
files = subprocess.check_output(['git','diff','--name-only','c9d60229ecf3ed2bf76b29525fec0bdda41d0906'],cwd=ROOT,text=True).splitlines()
files += subprocess.check_output(['git','ls-files','--others','--exclude-standard'],cwd=ROOT,text=True).splitlines()
files = sorted(set(f for f in files if '/default-managed-evidence/' not in f and not f.startswith('.serena/')))
for f in ['Cargo.toml','Cargo.lock']:
    assert (ROOT/f).read_bytes() == subprocess.check_output(['git','show','c9d60229ecf3ed2bf76b29525fec0bdda41d0906:'+f],cwd=ROOT)
source_hashes = {f: hashlib.sha256((ROOT/f).read_bytes()).hexdigest() for f in files if (ROOT/f).is_file()}
counts = {}
regressions = HERE/'evidence/regressions-complete'
for name in ['owned-symbol195','ordinary256','foundation','semantic','binders','output','graph99','guards518','reports265']:
    a=json.loads((regressions/(name+'.json')).read_text())
    counts[name]={k:a[k] for k in ('passed','failed','checks') if k in a and isinstance(a[k],int)}
for name,file in [('owned-result-reconciled','reconciled/reconciled-product.json'),('restored-shadow','reconciled/restored-shadow.json')]:
    a=json.loads((regressions/file).read_text());counts[name]={k:a[k] for k in ('passed','failed') if k in a}
workspace=next(r for r in rows if r['name']=='workspace-tests')
report = {'schema_version':1,'base':'c9d60229ecf3ed2bf76b29525fec0bdda41d0906',
          'bounded_deliverable_review_ready':True,'checkpoint_complete':False,'release_complete':False,
          'scope':'Default managed source/targets checkpoint; full S2b and R01–R15 remain open',
          'checks':rows,'product_assertions':{'passed':product['passed'],'failed':product['failed']},
          'workspace':{k:workspace[k] for k in ('passed','failed','ignored','suites')},
          'regression_counts':counts,'implementation_source_sha256':source_hashes,
          'root_manifests_sha256':{f:hashlib.sha256((ROOT/f).read_bytes()).hexdigest() for f in ['Cargo.toml','Cargo.lock']},
          'failed_attempts':'All retained under evidence; final selection does not relabel them.',
          'evidence_counter_contract':'allocations counts logical source values; roots count compiler registrations; byte/collection/failure reports and final census come from actual Session.',
          'open_gates':['full S2b ordinary ownership','named regions/transfers','entry and failed-owner recovery','mixed packages and ReadArgs bridge','broader signatures/reborrows/partial moves','foreign/tensor/worker execution','source cancellation integration','leak-enabled negative control blocked by host','external audit/performance','S3/S4','all R01–R15 release requirements']}
(HERE/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'review_ready':True,'product_assertions':report['product_assertions'],'workspace':report['workspace'],'checks':len(rows)},indent=2))
