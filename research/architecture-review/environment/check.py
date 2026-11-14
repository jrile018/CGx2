#!/usr/bin/env python3
"""Validate durable evidence offline; no compilation, downloads or shared writes."""
import hashlib, json, pathlib
p=pathlib.Path(__file__).resolve().parent
m=json.loads((p/'evidence-manifest.json').read_text())
for entry in m['files']:
    f=p/entry['path']
    assert f.is_file(), f'missing {f}'
    assert f.stat().st_size==entry['bytes'], f'size changed {f}'
    assert hashlib.sha256(f.read_bytes()).hexdigest()==entry['sha256'], f'hash changed {f}'
r=json.loads((p/'result.json').read_text())
assert r['status']=='partial'
f=p/r['input_freeze']['path']
assert hashlib.sha256(f.read_bytes()).hexdigest()==r['input_freeze']['sha256'], 'input manifest changed'
ids=set()
for c in r['checks']:
    assert c['id'] not in ids
    ids.add(c['id'])
    log=(p/c['evidence_file']).read_text()
    assert f"exit_code={c['exit_code']}\n" in log
    assert log.startswith('$ '+c['command']+'\n')
    if c['status']=='pass': assert c['exit_code']==0
    elif c['status']=='blocked': assert c['id']=='torch_gpu_attempt' and c['exit_code']!=0
    else: raise AssertionError('unexpected check status '+c['status'])
for name in ['rust_cpu','ocaml_cpu','python_cpu','native_cpu']:
    assert 'PASS' in (p/'raw'/f'{name}.log').read_text()
ldd=(p/'raw/native_ldd.log').read_text()
assert 'not found' not in ldd and 'libpython' not in ldd
assert 'libtorch_cpu.so' in ldd and 'libc10.so' in ldd
assert 'mapped_libpython=false' in (p/'raw/native_cpu.log').read_text()
log=(p/'raw/gpu_vulkan.log').read_text()
gpu=next(json.loads(x) for x in log.splitlines() if x.startswith('{') and json.loads(x).get('status')=='PASS')
a=gpu['adapter']
assert (a['vendor_id'],a['device_id'],a['adapter_type'],a['backend_type'])==(0x1002,0x7550,'DiscreteGPU','Vulkan')
assert gpu['cpu_fallback'] is False and gpu['precision']==['i32','f32']
assert gpu['int_values']==[3,-3,7,9] and gpu['float_values']==[2.,4.,6.,8.]
assert 'on_submitted_work_done_sync' in gpu['synchronization']
assert 'Torch not compiled with CUDA enabled' in (p/'raw/torch_gpu_attempt.log').read_text()
assert r['claims']['cgx2_native_closure_no_libpython']=='NOT_PROVEN'
assert len(r['blockers'])>=3
print(f'PASS: {len(m["files"])} pinned files, {len(ids)} actual command receipts; discrete Vulkan compute confirmed; ML GPU blocker retained; no performance claims.')
